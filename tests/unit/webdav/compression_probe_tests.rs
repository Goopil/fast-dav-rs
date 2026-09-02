//! Wire tests for the request-compression probe (`RequestCompressionMode::Auto`).
//!
//! Regression coverage for AUDIT-012: a failed probe must not permanently pin
//! `Identity` — the next body-carrying request re-probes, while the current
//! request proceeds uncompressed. A completed probe still caches its answer.

use bytes::Bytes;
use fast_dav_rs::{ContentEncoding, WebDavClient};
use hyper::{HeaderMap, Method};

use crate::common::http_helpers::{response_head, serve_sequence};

fn status_head(status_line: &str, extra_headers: &str) -> String {
    format!(
        "HTTP/1.1 {status_line}\r\n{extra_headers}Content-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn ok(body: &[u8]) -> (String, Vec<u8>) {
    (response_head("", body.len()), body.to_vec())
}

#[tokio::test]
async fn probe_failure_reprobes_and_negotiates_on_success() {
    // 1) probe → 500, 2) real request (uncompressed), 3) probe → 200
    // (advertising gzip), 4) real request (gzip).
    let (base, captured) = serve_sequence(vec![
        (status_head("500 Internal Server Error", ""), Vec::new()),
        ok(b"first"),
        (
            status_head("200 OK", "Accept-Encoding: gzip\r\n"),
            Vec::new(),
        ),
        ok(b"second"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    let resp = client
        .send(
            Method::PUT,
            "one.txt",
            HeaderMap::new(),
            Some(Bytes::from_static(b"first")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .send(
            Method::PUT,
            "one.txt",
            HeaderMap::new(),
            Some(Bytes::from_static(b"second")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        4,
        "probe + request + re-probe + request: {reqs:?}"
    );
    let probe1 = String::from_utf8_lossy(&reqs[0]);
    let real1 = String::from_utf8_lossy(&reqs[1]);
    let probe2 = String::from_utf8_lossy(&reqs[2]);
    let real2 = String::from_utf8_lossy(&reqs[3]);

    assert!(
        probe1.starts_with("PROPFIND") && probe1.contains("content-encoding: gzip"),
        "first exchange must be the gzip probe: {probe1}"
    );
    assert!(
        real1.starts_with("PUT")
            && !real1.to_ascii_lowercase().contains("content-encoding:")
            && real1.contains("first"),
        "after a failed probe the current request must go uncompressed: {real1}"
    );
    assert!(
        probe2.starts_with("PROPFIND"),
        "no permanent pin: the next request must re-probe: {probe2}"
    );
    assert!(
        real2.starts_with("PUT") && real2.contains("content-encoding: gzip"),
        "after a successful re-probe the request must be gzip-compressed: {real2}"
    );
}

#[tokio::test]
async fn probe_failure_sends_identity_and_does_not_pin() {
    // Both probes fail: each body-carrying request still completes
    // (uncompressed) and each next request re-probes.
    let (base, captured) = serve_sequence(vec![
        (status_head("500 Internal Server Error", ""), Vec::new()),
        ok(b"first"),
        (status_head("500 Internal Server Error", ""), Vec::new()),
        ok(b"second"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    for payload in ["first", "second"] {
        let resp = client
            .send(
                Method::PUT,
                "one.txt",
                HeaderMap::new(),
                Some(Bytes::from_static(payload.as_bytes())),
                None,
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        4,
        "probe + request + re-probe + request: {reqs:?}"
    );
    let probe2 = String::from_utf8_lossy(&reqs[2]);
    let real2 = String::from_utf8_lossy(&reqs[3]);

    assert!(
        probe2.starts_with("PROPFIND"),
        "a second failure must not pin Identity: the third exchange re-probes: {probe2}"
    );
    assert!(
        real2.starts_with("PUT")
            && !real2.to_ascii_lowercase().contains("content-encoding:")
            && real2.contains("second"),
        "the request after the re-failed probe must still complete uncompressed: {real2}"
    );
}

#[tokio::test]
async fn probe_success_without_compression_caches_identity() {
    // Probe answered 200 with `Accept-Encoding: identity`: the server's
    // answer is cached, and no further probe is sent.
    let (base, captured) = serve_sequence(vec![
        (
            status_head("200 OK", "Accept-Encoding: identity\r\n"),
            Vec::new(),
        ),
        ok(b"first"),
        ok(b"second"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    for payload in ["first", "second"] {
        let resp = client
            .send(
                Method::PUT,
                "one.txt",
                HeaderMap::new(),
                Some(Bytes::from_static(payload.as_bytes())),
                None,
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    assert_eq!(
        client.request_compression(),
        ContentEncoding::Identity,
        "the completed probe's Identity answer must be cached"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        3,
        "probe + request + request, no re-probe: {reqs:?}"
    );
    let probe1 = String::from_utf8_lossy(&reqs[0]);
    let real1 = String::from_utf8_lossy(&reqs[1]);
    let real2 = String::from_utf8_lossy(&reqs[2]);

    assert!(
        probe1.starts_with("PROPFIND") && probe1.contains("content-encoding: gzip"),
        "the first exchange must be the gzip probe: {probe1}"
    );
    assert!(
        real1.starts_with("PUT") && !real1.to_ascii_lowercase().contains("content-encoding:"),
        "the first request must honor the negotiated Identity: {real1}"
    );
    assert!(
        real2.starts_with("PUT") && !real2.to_ascii_lowercase().contains("content-encoding:"),
        "the cached Identity must avoid a second probe: {real2}"
    );
}

#[tokio::test]
async fn probe_redirect_pins_identity_and_stops_reprobing() {
    // The probe bypasses the redirect pipeline, so a base URL that answers
    // 3xx makes every probe fail. A redirect is stable, not transient: pin
    // Identity once instead of paying the doomed probe before every request.
    let (base, captured) = serve_sequence(vec![
        (
            status_head("302 Found", "Location: /elsewhere/\r\n"),
            Vec::new(),
        ),
        ok(b"first"),
        ok(b"second"),
        ok(b"spare"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    for payload in ["first", "second"] {
        let resp = client
            .send(
                Method::PUT,
                "one.txt",
                HeaderMap::new(),
                Some(Bytes::from_static(payload.as_bytes())),
                None,
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    assert_eq!(
        client.request_compression(),
        ContentEncoding::Identity,
        "the redirect must pin Identity"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "probe + 2 requests, no re-probe: {reqs:?}");
    let real1 = String::from_utf8_lossy(&reqs[1]);
    let real2 = String::from_utf8_lossy(&reqs[2]);
    assert!(
        real1.starts_with("PUT") && !real1.to_ascii_lowercase().contains("content-encoding:"),
        "the first request must honor the pinned Identity: {real1}"
    );
    assert!(
        real2.starts_with("PUT") && !real2.to_ascii_lowercase().contains("content-encoding:"),
        "the pinned Identity must avoid a second probe: {real2}"
    );
}

#[tokio::test]
async fn probe_caches_gzip_when_server_advertises_it() {
    // The probe proved gzip; when the server's advertised preference names
    // gzip, that proven encoding is cached and reused without re-probing.
    let (base, captured) = serve_sequence(vec![
        (
            status_head("200 OK", "Accept-Encoding: gzip, br;q=0\r\n"),
            Vec::new(),
        ),
        ok(b"first"),
        ok(b"second"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    for payload in ["first", "second"] {
        let resp = client
            .send(
                Method::PUT,
                "one.txt",
                HeaderMap::new(),
                Some(Bytes::from_static(payload.as_bytes())),
                None,
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    assert_eq!(
        client.request_compression(),
        ContentEncoding::Gzip,
        "the proven gzip must be cached when the preference names it"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "probe + 2 requests, no re-probe: {reqs:?}");
    let real1 = String::from_utf8_lossy(&reqs[1]);
    let real2 = String::from_utf8_lossy(&reqs[2]);
    assert!(
        real1.starts_with("PUT") && real1.contains("content-encoding: gzip"),
        "the first request must use the cached gzip: {real1}"
    );
    assert!(
        real2.starts_with("PUT") && real2.contains("content-encoding: gzip"),
        "the cached gzip must avoid a second probe: {real2}"
    );
}

#[tokio::test]
async fn probe_caches_identity_when_server_does_not_advertise_gzip() {
    // The probe proved only gzip; a server advertising `br`/`zstd` has
    // proven nothing for request bodies. Cache the safe `Identity` rather
    // than an unproven encoding that could fail with `415` on a later PUT.
    let (base, captured) = serve_sequence(vec![
        (
            status_head("200 OK", "Accept-Encoding: br, zstd\r\n"),
            Vec::new(),
        ),
        ok(b"first"),
        ok(b"second"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    for payload in ["first", "second"] {
        let resp = client
            .send(
                Method::PUT,
                "one.txt",
                HeaderMap::new(),
                Some(Bytes::from_static(payload.as_bytes())),
                None,
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    assert_eq!(
        client.request_compression(),
        ContentEncoding::Identity,
        "an unproven br/zstd preference must not be cached"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "probe + 2 requests, no re-probe: {reqs:?}");
    let real1 = String::from_utf8_lossy(&reqs[1]);
    let real2 = String::from_utf8_lossy(&reqs[2]);
    assert!(
        real1.starts_with("PUT") && !real1.to_ascii_lowercase().contains("content-encoding:"),
        "the first request must honor the cached Identity: {real1}"
    );
    assert!(
        real2.starts_with("PUT") && !real2.to_ascii_lowercase().contains("content-encoding:"),
        "the cached Identity must avoid a second probe: {real2}"
    );
}

#[tokio::test]
async fn bad_request_does_not_pin_identity() {
    // A 400 can come from a malformed body unrelated to compression: it must
    // not disable compression for the client's lifetime (only 415/501 do).
    let (base, captured) = serve_sequence(vec![
        (
            status_head("200 OK", "Accept-Encoding: gzip\r\n"),
            Vec::new(),
        ),
        (status_head("400 Bad Request", ""), Vec::new()),
        ok(b"second"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    let resp = client
        .send(
            Method::PUT,
            "one.txt",
            HeaderMap::new(),
            Some(Bytes::from_static(b"first")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "the 400 is returned as-is");

    let resp = client
        .send(
            Method::PUT,
            "one.txt",
            HeaderMap::new(),
            Some(Bytes::from_static(b"second")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    assert_eq!(
        client.request_compression(),
        ContentEncoding::Gzip,
        "an unrelated 400 must not pin Identity"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "probe + 2 requests, no re-probe: {reqs:?}");
    let real2 = String::from_utf8_lossy(&reqs[2]);
    assert!(
        real2.starts_with("PUT") && real2.contains("content-encoding: gzip"),
        "compression must survive an unrelated 400: {real2}"
    );
}

#[tokio::test]
async fn caller_content_encoding_is_honored_and_body_sent_verbatim() {
    // A caller-supplied `Content-Encoding` means the body is already
    // encoded: forward it verbatim with the header intact — no re-compression
    // (silent double encoding) and no probe.
    let (base, captured) = serve_sequence(vec![
        (
            status_head("200 OK", "Accept-Encoding: gzip\r\n"),
            Vec::new(),
        ),
        ok(b"stored"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        hyper::header::CONTENT_ENCODING,
        hyper::header::HeaderValue::from_static("gzip"),
    );
    let resp = client
        .send(
            Method::PUT,
            "one.txt",
            headers,
            Some(Bytes::from_static(b"pre-compressed-payload")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        1,
        "no probe and a single request for a pre-compressed body: {reqs:?}"
    );
    let req = String::from_utf8_lossy(&reqs[0]);
    assert!(
        req.starts_with("PUT") && req.to_ascii_lowercase().contains("content-encoding: gzip"),
        "the caller's Content-Encoding must be forwarded untouched: {req}"
    );
    assert!(
        req.contains("pre-compressed-payload"),
        "the body must be sent verbatim, not re-compressed: {req}"
    );
}

#[tokio::test]
async fn probe_sends_configured_user_agent() {
    // RFC 9110 §10.1.5: the probe must identify with the configured
    // User-Agent like every other request, so User-Agent-aware servers do
    // not treat it differently.
    let (base, captured) =
        serve_sequence(vec![(status_head("200 OK", ""), Vec::new()), ok(b"stored")]).await;
    let client = WebDavClient::builder(&base)
        .user_agent("fast-dav-tests/1.0")
        .build()
        .unwrap();

    let resp = client
        .send(
            Method::PUT,
            "one.txt",
            HeaderMap::new(),
            Some(Bytes::from_static(b"payload")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    let probe = String::from_utf8_lossy(&reqs[0]);
    assert!(
        probe.starts_with("PROPFIND")
            && probe
                .to_ascii_lowercase()
                .contains("user-agent: fast-dav-tests/1.0"),
        "the probe must carry the configured User-Agent: {probe}"
    );
}
