use bytes::Bytes;
use fast_dav_rs::compression::*;
use hyper::http::{self, HeaderMap};

#[test]
fn test_content_encoding_as_str() {
    assert_eq!(ContentEncoding::Identity.as_str(), "identity");
    assert_eq!(ContentEncoding::Br.as_str(), "br");
    assert_eq!(ContentEncoding::Gzip.as_str(), "gzip");
    assert_eq!(ContentEncoding::Zstd.as_str(), "zstd");
}

#[test]
fn test_detect_encoding_identity() {
    let headers = HeaderMap::new();
    assert_eq!(detect_encoding(&headers), ContentEncoding::Identity);
}

#[test]
fn test_detect_encoding_gzip() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_ENCODING, "gzip".parse().unwrap());
    assert_eq!(detect_encoding(&headers), ContentEncoding::Gzip);
}

#[test]
fn test_detect_encoding_br() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_ENCODING, "br".parse().unwrap());
    assert_eq!(detect_encoding(&headers), ContentEncoding::Br);
}

#[test]
fn test_detect_encoding_zstd() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_ENCODING, "zstd".parse().unwrap());
    assert_eq!(detect_encoding(&headers), ContentEncoding::Zstd);
}

#[test]
fn test_detect_encoding_zst_variant() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_ENCODING, "zst".parse().unwrap());
    assert_eq!(detect_encoding(&headers), ContentEncoding::Zstd);
}

#[test]
fn test_detect_encoding_multiple_encodings() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_ENCODING,
        "gzip, deflate".parse().unwrap(),
    );
    // Should pick the first one
    assert_eq!(detect_encoding(&headers), ContentEncoding::Gzip);
}

#[test]
fn test_detect_encoding_case_insensitive() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_ENCODING, "GZIP".parse().unwrap());
    assert_eq!(detect_encoding(&headers), ContentEncoding::Gzip);
}

#[test]
fn test_detect_encoding_unknown_encoding() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_ENCODING, "unknown".parse().unwrap());
    assert_eq!(detect_encoding(&headers), ContentEncoding::Identity);
}

#[test]
fn test_add_accept_encoding_new_header() {
    let mut headers = HeaderMap::new();
    add_accept_encoding(&mut headers);
    assert!(headers.contains_key(http::header::ACCEPT_ENCODING));
    let value = headers.get(http::header::ACCEPT_ENCODING).unwrap();
    assert_eq!(value, "br, zstd, gzip");
}

#[test]
fn test_add_accept_encoding_existing_header() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::ACCEPT_ENCODING, "deflate".parse().unwrap());
    add_accept_encoding(&mut headers);
    // Should not override the existing header
    let value = headers.get(http::header::ACCEPT_ENCODING).unwrap();
    assert_eq!(value, "deflate");
}

#[test]
fn test_add_content_encoding() {
    let mut headers = HeaderMap::new();
    add_content_encoding(&mut headers, ContentEncoding::Gzip);
    assert_eq!(headers.get("Content-Encoding").unwrap(), "gzip");
}

#[test]
fn test_add_content_encoding_identity() {
    let mut headers = HeaderMap::new();
    add_content_encoding(&mut headers, ContentEncoding::Identity);
    assert!(!headers.contains_key("Content-Encoding"));
}

#[tokio::test]
async fn test_compress_payload_identity() {
    let data = Bytes::from("Hello, world!");
    let compressed = compress_payload(data.clone(), ContentEncoding::Identity)
        .await
        .unwrap();
    assert_eq!(compressed, data);
}
