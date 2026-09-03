use fast_dav_rs::webdav::retry::{
    backoff_delay, is_idempotent_method, is_retryable_status, parse_http_date, retry_after_delay,
    retry_delay,
};
use fast_dav_rs::{RequestCompressionMode, WebDavClient};
use hyper::{HeaderMap, Method};
use std::time::Duration;

fn status_head(status_line: &str, extra_headers: &str) -> String {
    format!(
        "HTTP/1.1 {status_line}\r\n{extra_headers}Content-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn unavailable(extra_headers: &str) -> (String, Vec<u8>) {
    (
        status_head("503 Service Unavailable", extra_headers),
        Vec::new(),
    )
}

fn too_many_requests(extra_headers: &str) -> (String, Vec<u8>) {
    (
        status_head("429 Too Many Requests", extra_headers),
        Vec::new(),
    )
}

fn ok(body: &[u8]) -> (String, Vec<u8>) {
    (
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        ),
        body.to_vec(),
    )
}

/// Client with retrying enabled, compression disabled (no probe requests)
/// and shrunken backoff delays so tests stay fast.
fn retry_client(base: &str, max_retries: usize, retry_all: bool) -> WebDavClient {
    let mut client = WebDavClient::builder(base)
        .max_retries(max_retries)
        .retry_all(retry_all)
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client.set_retry_delays_for_testing(Duration::from_millis(1), Duration::from_millis(2));
    client
}

// ---------------------------------------------------------------------------
// Classification helpers
// ---------------------------------------------------------------------------

#[test]
fn retryable_statuses_are_429_503_504() {
    for status in [429, 503, 504] {
        assert!(
            is_retryable_status(hyper::StatusCode::from_u16(status).unwrap()),
            "{status} must be retryable"
        );
    }
    for status in [200, 301, 400, 415, 500, 501] {
        assert!(
            !is_retryable_status(hyper::StatusCode::from_u16(status).unwrap()),
            "{status} must not be retryable"
        );
    }
}

#[test]
fn idempotent_methods_are_classified() {
    for name in ["GET", "HEAD", "OPTIONS", "PROPFIND", "REPORT"] {
        let method = Method::from_bytes(name.as_bytes()).unwrap();
        assert!(is_idempotent_method(&method), "{name} must be idempotent");
    }
    for name in [
        "POST", "PUT", "DELETE", "MKCOL", "COPY", "MOVE", "LOCK", "PATCH",
    ] {
        let method = Method::from_bytes(name.as_bytes()).unwrap();
        assert!(
            !is_idempotent_method(&method),
            "{name} must not be idempotent"
        );
    }
}

// ---------------------------------------------------------------------------
// Retry-After parsing
// ---------------------------------------------------------------------------

#[test]
fn retry_after_seconds_are_parsed() {
    assert_eq!(retry_after_delay("0"), Some(Duration::ZERO));
    assert_eq!(retry_after_delay("12"), Some(Duration::from_secs(12)));
    assert_eq!(retry_after_delay(" 5 "), Some(Duration::from_secs(5)));
    assert_eq!(retry_after_delay("-3"), None);
    assert_eq!(retry_after_delay("soon"), None);
    assert_eq!(retry_after_delay(""), None);
}

#[test]
fn retry_after_http_date_is_parsed() {
    // RFC 9110 §5.6.7 example date.
    assert_eq!(
        parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
        Some(784_111_777)
    );

    // A future date yields the remaining duration.
    let future = (chrono::Utc::now() + chrono::Duration::seconds(300))
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    let delay = retry_after_delay(&future).unwrap();
    assert!(
        delay >= Duration::from_secs(240) && delay <= Duration::from_secs(300),
        "expected roughly 300s, got {delay:?}"
    );

    // A past date means "retry immediately".
    let past = (chrono::Utc::now() - chrono::Duration::seconds(60))
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    assert_eq!(retry_after_delay(&past), Some(Duration::ZERO));

    // Garbage dates fall back (None → caller uses backoff).
    assert_eq!(parse_http_date("soon"), None);
    assert_eq!(parse_http_date("32 Jan 2020 00:00:00 GMT"), None);
    assert_eq!(parse_http_date("06 Xyz 2020 00:00:00 GMT"), None);
    assert_eq!(parse_http_date("06 Nov 2020 25:00:00 GMT"), None);
}

// ---------------------------------------------------------------------------
// Retry delay policy (`retry_delay`)
// ---------------------------------------------------------------------------

fn retry_after_headers(value: &str) -> HeaderMap {
    HeaderMap::from_iter([(
        hyper::header::RETRY_AFTER,
        hyper::header::HeaderValue::from_str(value).unwrap(),
    )])
}

#[test]
fn retry_delay_honors_small_retry_after_on_429_503_504() {
    let headers = retry_after_headers("5");
    for code in [429u16, 503, 504] {
        let status = hyper::StatusCode::from_u16(code).unwrap();
        assert_eq!(
            retry_delay(
                status,
                &headers,
                3,
                Duration::from_millis(250),
                Duration::from_secs(8)
            ),
            Duration::from_secs(5),
            "{code} must honor a small Retry-After"
        );
    }
}

#[test]
fn retry_delay_clamps_huge_retry_after_to_backoff_cap() {
    let cap = Duration::from_secs(8);
    let far_future = (chrono::Utc::now() + chrono::Duration::seconds(86_400 * 365 * 10))
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    for value in ["999999999999", &far_future] {
        let headers = retry_after_headers(value);
        for code in [429u16, 503, 504] {
            let status = hyper::StatusCode::from_u16(code).unwrap();
            assert_eq!(
                retry_delay(status, &headers, 0, Duration::from_millis(250), cap),
                cap,
                "{code} must clamp a huge Retry-After ({value}) to the cap"
            );
        }
    }
}

#[test]
fn retry_delay_falls_back_to_backoff_without_usable_retry_after() {
    let absent = HeaderMap::new();
    let unparseable = retry_after_headers("soon");
    for headers in [&absent, &unparseable] {
        for code in [429u16, 503, 504] {
            let status = hyper::StatusCode::from_u16(code).unwrap();
            // attempt 1 → 500 ms base, ±25 % jitter.
            let delay = retry_delay(
                status,
                headers,
                1,
                Duration::from_millis(250),
                Duration::from_secs(8),
            );
            assert!(
                delay >= Duration::from_millis(375) && delay <= Duration::from_millis(625),
                "{code} must fall back to backoff with {headers:?}, got {delay:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Exponential backoff + jitter
// ---------------------------------------------------------------------------

#[test]
fn backoff_doubles_and_is_jittered() {
    let initial = Duration::from_millis(250);
    // ±25 % around the base delay.
    let d0 = backoff_delay(0, initial, Duration::from_secs(8));
    assert!(
        d0 >= Duration::from_millis(187) && d0 <= Duration::from_millis(313),
        "attempt 0 must be ~250ms ±25%, got {d0:?}"
    );
    let d2 = backoff_delay(2, initial, Duration::from_secs(8));
    assert!(
        d2 >= Duration::from_millis(750) && d2 <= Duration::from_millis(1250),
        "attempt 2 must be ~1000ms ±25%, got {d2:?}"
    );
}

#[test]
fn backoff_is_capped() {
    let cap = Duration::from_secs(8);
    // Far-future attempts saturate at the cap (jitter applied before capping).
    let d = backoff_delay(30, Duration::from_millis(250), cap);
    assert!(
        d >= Duration::from_millis(6000) && d <= cap,
        "saturated attempts must stay within [0.75*cap, cap], got {d:?}"
    );
    // An initial delay above the cap is clamped.
    let d = backoff_delay(0, Duration::from_secs(30), cap);
    assert!(d <= cap, "delay above the cap must be clamped, got {d:?}");
}

#[test]
fn backoff_zero_initial_stays_zero() {
    assert_eq!(
        backoff_delay(5, Duration::ZERO, Duration::from_secs(8)),
        Duration::ZERO
    );
}

// ---------------------------------------------------------------------------
// Wire behavior (shared request pipeline)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_429_honors_retry_after_zero_then_succeeds() {
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        too_many_requests("Retry-After: 0\r\n"),
        ok(b"done"),
    ])
    .await;
    let client = retry_client(&base, 3, false);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body().as_ref(), b"done");

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "429 must be retried once: {reqs:?}");
}

#[tokio::test]
async fn retry_429_without_retry_after_uses_backoff() {
    let (base, captured) =
        crate::common::http_helpers::serve_sequence(vec![too_many_requests(""), ok(b"done")]).await;
    let client = retry_client(&base, 2, false);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        2,
        "429 without Retry-After must use backoff: {reqs:?}"
    );
}

#[tokio::test]
async fn retry_503_twice_then_succeeds() {
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        unavailable(""),
        unavailable(""),
        ok(b"recovered"),
    ])
    .await;
    let client = retry_client(&base, 2, false);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body().as_ref(), b"recovered");

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "two 503s must consume two retries: {reqs:?}");
}

#[tokio::test]
async fn exhausted_retries_return_last_response() {
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        unavailable(""),
        unavailable(""),
        unavailable(""),
    ])
    .await;
    let client = retry_client(&base, 2, false);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        503,
        "exhausted retries must return the last response as-is"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        3,
        "total attempts must be 1 + max_retries: {reqs:?}"
    );
}

#[tokio::test]
async fn non_idempotent_method_is_not_retried() {
    let (base, captured) =
        crate::common::http_helpers::serve_sequence(vec![unavailable(""), ok(b"unused")]).await;
    let client = retry_client(&base, 3, false);

    let resp = client
        .send(
            Method::PUT,
            "",
            HeaderMap::new(),
            Some(bytes::Bytes::from_static(b"PAYLOAD")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 503, "PUT must not be retried by default");

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        1,
        "a failed PUT must be sent exactly once: {reqs:?}"
    );
}

#[tokio::test]
async fn retry_all_enables_non_idempotent_retry() {
    let (base, captured) =
        crate::common::http_helpers::serve_sequence(vec![unavailable(""), ok(b"done")]).await;
    let client = retry_client(&base, 3, true);

    let resp = client
        .send(
            Method::PUT,
            "",
            HeaderMap::new(),
            Some(bytes::Bytes::from_static(b"PAYLOAD")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "retry_all must retry the PUT once: {reqs:?}");
}

#[tokio::test]
async fn max_retries_zero_sends_single_attempt() {
    let (base, captured) =
        crate::common::http_helpers::serve_sequence(vec![unavailable(""), ok(b"unused")]).await;
    let client = retry_client(&base, 0, false);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 503, "default max_retries=0 must not retry");

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 1, "exactly one attempt expected: {reqs:?}");
}

#[tokio::test]
async fn webdav_verbs_are_retried_by_default_policy() {
    let (base, captured) =
        crate::common::http_helpers::serve_sequence(vec![unavailable(""), ok(b"ok")]).await;
    let client = retry_client(&base, 1, false);

    let body = bytes::Bytes::from_static(
        br#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#,
    );
    let resp = client
        .send(
            Method::from_bytes(b"PROPFIND").unwrap(),
            "",
            HeaderMap::new(),
            Some(body),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "PROPFIND must be retried: {reqs:?}");
}

#[tokio::test]
async fn retry_429_with_http_date_retry_after() {
    // A past HTTP-date means "retry immediately" (zero delay).
    let past = (chrono::Utc::now() - chrono::Duration::seconds(60))
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        too_many_requests(&format!("Retry-After: {past}\r\n")),
        ok(b"done"),
    ])
    .await;
    let client = retry_client(&base, 3, false);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        2,
        "HTTP-date Retry-After must be honored: {reqs:?}"
    );
}

#[tokio::test]
async fn retry_works_on_streaming_path() {
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        too_many_requests("Retry-After: 0\r\n"),
        ok(b"streamed"),
    ])
    .await;
    let client = retry_client(&base, 2, false);

    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    use http_body_util::BodyExt;
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"streamed");

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "send_stream must retry the 429: {reqs:?}");
}

#[tokio::test]
async fn retry_budget_is_shared_across_redirect_hops() {
    let redirect = (
        status_head("302 Found", "Location: /final/\r\n"),
        Vec::new(),
    );
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        redirect,
        too_many_requests("Retry-After: 0\r\n"),
        ok(b"after-hop"),
    ])
    .await;
    let client = retry_client(&base, 1, false);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body().as_ref(), b"after-hop");

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        3,
        "redirect hop + one retried 429 must fit in the shared budget: {reqs:?}"
    );
}

// ---------------------------------------------------------------------------
// Builder delegation (CalDAV/CardDAV inherit via the macro)
// ---------------------------------------------------------------------------

#[test]
fn retry_options_delegate_to_caldav_and_carddav_builders() {
    let _cal = fast_dav_rs::CalDavClient::builder("https://cal.example.com/dav/")
        .max_retries(3)
        .retry_all(true)
        .build()
        .unwrap();
    let _card = fast_dav_rs::CardDavClient::builder("https://card.example.com/dav/")
        .max_retries(1)
        .retry_all(false)
        .build()
        .unwrap();
}
