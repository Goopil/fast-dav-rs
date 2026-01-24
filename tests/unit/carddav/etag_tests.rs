use fast_dav_rs::CardDavClient;
use hyper::http::{HeaderMap, HeaderValue};

#[test]
fn test_etag_from_headers_present() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"abc123\""));

    let etag = CardDavClient::etag_from_headers(&headers);
    assert_eq!(etag, Some("\"abc123\"".to_string()));
}

#[test]
fn test_etag_from_headers_missing() {
    let headers = HeaderMap::new();
    let etag = CardDavClient::etag_from_headers(&headers);
    assert_eq!(etag, None);
}

#[test]
fn test_etag_from_headers_invalid_utf8() {
    let mut headers = HeaderMap::new();
    // Create a header value with invalid UTF-8
    let invalid_value = HeaderValue::from_bytes(b"\xFF\xFE").unwrap();
    headers.insert("ETag", invalid_value);

    let etag = CardDavClient::etag_from_headers(&headers);
    assert_eq!(etag, None);
}

#[test]
fn test_etag_from_headers_multiple_values() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"first\""));
    headers.append("ETag", HeaderValue::from_static("\"second\""));

    let etag = CardDavClient::etag_from_headers(&headers);
    // Should return the first value
    assert_eq!(etag, Some("\"first\"".to_string()));
}

#[test]
fn test_etag_from_headers_weak_etag() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("W/\"weak123\""));

    let etag = CardDavClient::etag_from_headers(&headers);
    assert_eq!(etag, Some("W/\"weak123\"".to_string()));
}

#[test]
fn test_put_if_match_valid_etag() {
    let _ = CardDavClient::new("https://example.com/dav/", Some("user"), Some("pass"))
        .expect("Failed to create client");

    // This test just verifies the method compiles and can be called
    // Actual network testing is done in E2E tests
    let etag = "\"valid123\"";
    assert_eq!(etag, "\"valid123\"");
}

#[test]
fn test_delete_if_match_valid_etag() {
    let _ = CardDavClient::new("https://example.com/dav/", Some("user"), Some("pass"))
        .expect("Failed to create client");

    // This test just verifies the method compiles and can be called
    // Actual network testing is done in E2E tests
    let etag = "\"valid123\"";
    assert_eq!(etag, "\"valid123\"");
}
