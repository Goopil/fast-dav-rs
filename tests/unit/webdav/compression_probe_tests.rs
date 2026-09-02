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
    // 1) probe → 500, 2) real request (uncompressed), 3) probe → 200,
    // 4) real request (gzip).
    let (base, captured) = serve_sequence(vec![
        (status_head("500 Internal Server Error", ""), Vec::new()),
        ok(b"first"),
        (status_head("200 OK", ""), Vec::new()),
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
