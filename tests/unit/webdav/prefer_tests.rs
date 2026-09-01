use bytes::Bytes;
use fast_dav_rs::webdav::Prefer;
use fast_dav_rs::{CalDavClient, CardDavClient, RequestCompressionMode, WebDavClient};
use hyper::header::HeaderValue;
use hyper::{HeaderMap, Method};

use crate::common::http_helpers::{response_head, serve_capture};

#[test]
fn prefer_as_str_matches_rfc_7240_values() {
    assert_eq!(Prefer::Minimal.as_str(), "return=minimal");
    assert_eq!(Prefer::Representation.as_str(), "return=representation");
}

#[test]
fn preference_applied_parses_both_return_values() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Preference-Applied",
        HeaderValue::from_static("return=minimal"),
    );
    assert_eq!(
        fast_dav_rs::preference_applied_from_headers(&headers),
        Some(Prefer::Minimal)
    );
    headers.insert(
        "Preference-Applied",
        HeaderValue::from_static("return=representation"),
    );
    assert_eq!(
        fast_dav_rs::preference_applied_from_headers(&headers),
        Some(Prefer::Representation)
    );
}

#[test]
fn preference_applied_is_case_insensitive() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Preference-Applied",
        HeaderValue::from_static("RETURN=MINIMAL"),
    );
    assert_eq!(
        fast_dav_rs::preference_applied_from_headers(&headers),
        Some(Prefer::Minimal)
    );
}

#[test]
fn preference_applied_absent_or_unrecognized_yields_none() {
    assert_eq!(
        fast_dav_rs::preference_applied_from_headers(&HeaderMap::new()),
        None
    );
    for value in [
        "",
        "garbage",
        "return",
        "return=weird",
        "wait=10",
        "handling=lenient",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert("Preference-Applied", HeaderValue::from_static(value));
        assert_eq!(
            fast_dav_rs::preference_applied_from_headers(&headers),
            None,
            "value: {value}"
        );
    }
}

#[test]
fn preference_applied_invalid_utf8_yields_none() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Preference-Applied",
        HeaderValue::from_bytes(b"\xFF\xFE").unwrap(),
    );
    assert_eq!(fast_dav_rs::preference_applied_from_headers(&headers), None);
}

#[tokio::test]
async fn builder_prefer_minimal_is_sent() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = WebDavClient::builder(&base)
        .prefer(Some(Prefer::Minimal))
        .build()
        .unwrap();

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(req.contains("prefer: return=minimal"), "request: {req}");
}

#[tokio::test]
async fn builder_prefer_representation_is_sent_on_put() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = WebDavClient::builder(&base)
        .prefer(Some(Prefer::Representation))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .send(
            Method::PUT,
            "event.ics",
            HeaderMap::new(),
            Some(Bytes::from_static(b"body")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(
        req.contains("prefer: return=representation"),
        "request: {req}"
    );
}

#[tokio::test]
async fn no_prefer_header_by_default() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = WebDavClient::builder(&base).build().unwrap();

    client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(!req.contains("prefer:"), "request: {req}");
}

#[tokio::test]
async fn explicit_request_prefer_wins_over_builder_default() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = WebDavClient::builder(&base)
        .prefer(Some(Prefer::Minimal))
        .build()
        .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert("Prefer", HeaderValue::from_static("return=representation"));
    client
        .send(Method::GET, "", headers, None, None)
        .await
        .unwrap();

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(
        req.contains("prefer: return=representation"),
        "request: {req}"
    );
    assert!(!req.contains("return=minimal"), "request: {req}");
}

#[tokio::test]
async fn caldav_builder_prefer_is_sent() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = CalDavClient::builder(&base)
        .prefer(Some(Prefer::Minimal))
        .build()
        .unwrap();

    let resp = client.get("").await.unwrap();
    assert_eq!(resp.status(), 200);

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(req.contains("prefer: return=minimal"), "request: {req}");
}

#[tokio::test]
async fn caldav_put_if_match_prefer_sends_representation() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = CalDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .put_if_match_prefer(
            "event.ics",
            Bytes::from_static(
                b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nEND:VCALENDAR\r\n",
            ),
            "etag-1",
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(req.starts_with("put "), "request: {req}");
    assert!(
        req.contains("prefer: return=representation"),
        "request: {req}"
    );
    assert!(req.contains("if-match: \"etag-1\""), "request: {req}");
    assert!(
        req.contains("content-type: text/calendar"),
        "request: {req}"
    );
}

#[tokio::test]
async fn caldav_put_if_match_omits_prefer() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = CalDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client
        .put_if_match(
            "event.ics",
            Bytes::from_static(
                b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nEND:VCALENDAR\r\n",
            ),
            "etag-1",
        )
        .await
        .unwrap();

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(!req.contains("prefer:"), "request: {req}");
}

#[tokio::test]
async fn carddav_put_if_match_prefer_sends_representation_over_builder_default() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = CardDavClient::builder(&base)
        .prefer(Some(Prefer::Minimal))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .put_if_match_prefer("contact.vcf", Bytes::from_static(b"BEGIN:VCARD"), "etag-2")
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let req = String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase();
    assert!(
        req.contains("prefer: return=representation"),
        "request: {req}"
    );
    assert!(!req.contains("return=minimal"), "request: {req}");
    assert!(req.contains("if-match: \"etag-2\""), "request: {req}");
    assert!(req.contains("content-type: text/vcard"), "request: {req}");
}
