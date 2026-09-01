use bytes::Bytes;
use fast_dav_rs::Error;
use fast_dav_rs::common::compression::*;
use http_body_util::Full;
use hyper::Request;
use hyper::client::conn::http1;
use hyper::http::{self, HeaderMap};
use hyper_util::rt::TokioIo;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

async fn make_incoming_body(data: Vec<u8>) -> hyper::body::Incoming {
    let (client_io, mut server_io) = io::duplex(16 * 1024);

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        let mut seen = Vec::new();
        loop {
            let n = server_io.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
            if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }

        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            data.len()
        );
        server_io.write_all(header.as_bytes()).await.unwrap();
        server_io.write_all(&data).await.unwrap();
        server_io.shutdown().await.unwrap();
    });

    let (mut sender, conn) = http1::handshake(TokioIo::new(client_io)).await.unwrap();
    let _conn_task = tokio::spawn(conn);

    let req = Request::builder()
        .method("GET")
        .uri("http://localhost/")
        .body(Full::<Bytes>::default())
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    server_task.await.unwrap();
    resp.into_body()
}

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
fn test_detect_encodings_chain_order() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_ENCODING, "gzip, br".parse().unwrap());
    let chain = detect_encodings(&headers);
    assert_eq!(chain, vec![ContentEncoding::Gzip, ContentEncoding::Br]);
}

#[test]
fn test_detect_encodings_ignores_unknowns() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_ENCODING,
        "gzip, unknown, br".parse().unwrap(),
    );
    let chain = detect_encodings(&headers);
    assert_eq!(chain, vec![ContentEncoding::Gzip, ContentEncoding::Br]);
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

#[test]
fn test_detect_request_compression_preference_absent() {
    let headers = HeaderMap::new();
    assert!(detect_request_compression_preference(&headers).is_none());
}

#[test]
fn test_detect_request_compression_preference_prefers_quality() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::ACCEPT_ENCODING,
        "gzip;q=0.4, br;q=0.9".parse().unwrap(),
    );
    assert_eq!(
        detect_request_compression_preference(&headers),
        Some(ContentEncoding::Br)
    );
}

#[test]
fn test_detect_request_compression_preference_prefers_order_on_tie() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::ACCEPT_ENCODING,
        "gzip, br, zstd".parse().unwrap(),
    );
    assert_eq!(
        detect_request_compression_preference(&headers),
        Some(ContentEncoding::Br)
    );
}

#[test]
fn test_detect_request_compression_preference_identity_fallback() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::ACCEPT_ENCODING,
        "gzip;q=0, identity;q=1.0".parse().unwrap(),
    );
    assert_eq!(
        detect_request_compression_preference(&headers),
        Some(ContentEncoding::Identity)
    );
}

#[test]
fn test_detect_request_compression_preference_wildcard() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::ACCEPT_ENCODING, "*;q=0.5".parse().unwrap());
    assert_eq!(
        detect_request_compression_preference(&headers),
        Some(ContentEncoding::Br)
    );
}

#[tokio::test]
async fn test_compress_payload_identity() {
    let data = Bytes::from("Hello, world!");
    let compressed = compress_payload(data.clone(), ContentEncoding::Identity)
        .await
        .unwrap();
    assert_eq!(compressed, data);
}

#[tokio::test]
async fn test_decompress_body_identity() {
    let original = Bytes::from("Hello, uncompressed world!");
    let body = make_incoming_body(original.to_vec()).await;
    let decompressed = decompress_body(body, &[ContentEncoding::Identity])
        .await
        .unwrap();
    assert_eq!(decompressed, original);
}

#[tokio::test]
async fn test_decompress_body_gzip() {
    let original = Bytes::from("Hello, compressed world!");
    let compressed = compress_payload(original.clone(), ContentEncoding::Gzip)
        .await
        .unwrap();
    let body = make_incoming_body(compressed.to_vec()).await;
    let decompressed = decompress_body(body, &[ContentEncoding::Gzip])
        .await
        .unwrap();
    assert_eq!(decompressed, original);
}

#[tokio::test]
async fn test_decompress_body_br() {
    let original = Bytes::from("Hello, brotli world!");
    let compressed = compress_payload(original.clone(), ContentEncoding::Br)
        .await
        .unwrap();
    let body = make_incoming_body(compressed.to_vec()).await;
    let decompressed = decompress_body(body, &[ContentEncoding::Br]).await.unwrap();
    assert_eq!(decompressed, original);
}

#[tokio::test]
async fn test_decompress_body_zstd() {
    let original = Bytes::from("Hello, zstd world!");
    let compressed = compress_payload(original.clone(), ContentEncoding::Zstd)
        .await
        .unwrap();
    let body = make_incoming_body(compressed.to_vec()).await;
    let decompressed = decompress_body(body, &[ContentEncoding::Zstd])
        .await
        .unwrap();
    assert_eq!(decompressed, original);
}

#[tokio::test]
async fn test_decompress_body_empty() {
    let body = make_incoming_body(Vec::new()).await;
    let decompressed = decompress_body(body, &[ContentEncoding::Identity])
        .await
        .unwrap();
    assert!(decompressed.is_empty());
}

#[tokio::test]
async fn test_decompress_body_multi_layer() {
    let original = Bytes::from("Hello, multi-layer compressed world!");
    let inner = compress_payload(original.clone(), ContentEncoding::Gzip)
        .await
        .unwrap();
    let outer = compress_payload(inner, ContentEncoding::Br).await.unwrap();
    let body = make_incoming_body(outer.to_vec()).await;
    let decompressed = decompress_body(body, &[ContentEncoding::Gzip, ContentEncoding::Br])
        .await
        .unwrap();
    assert_eq!(decompressed, original);
}

#[tokio::test]
async fn test_decompress_stream_identity() {
    use tokio::io::AsyncReadExt;
    let original = Bytes::from("Stream identity data");
    let body = make_incoming_body(original.to_vec()).await;
    let mut reader = decompress_stream(body, &[ContentEncoding::Identity]).unwrap();
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await.unwrap();
    assert_eq!(Bytes::from(out), original);
}

#[tokio::test]
async fn test_decompress_stream_gzip() {
    use tokio::io::AsyncReadExt;
    let original = Bytes::from("Stream gzip data");
    let compressed = compress_payload(original.clone(), ContentEncoding::Gzip)
        .await
        .unwrap();
    let body = make_incoming_body(compressed.to_vec()).await;
    let mut reader = decompress_stream(body, &[ContentEncoding::Gzip]).unwrap();
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await.unwrap();
    assert_eq!(Bytes::from(out), original);
}

#[tokio::test]
async fn test_decompress_stream_br() {
    use tokio::io::AsyncReadExt;
    let original = Bytes::from("Stream br data");
    let compressed = compress_payload(original.clone(), ContentEncoding::Br)
        .await
        .unwrap();
    let body = make_incoming_body(compressed.to_vec()).await;
    let mut reader = decompress_stream(body, &[ContentEncoding::Br]).unwrap();
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await.unwrap();
    assert_eq!(Bytes::from(out), original);
}

#[tokio::test]
async fn test_decompress_stream_zstd() {
    use tokio::io::AsyncReadExt;
    let original = Bytes::from("Stream zstd data");
    let compressed = compress_payload(original.clone(), ContentEncoding::Zstd)
        .await
        .unwrap();
    let body = make_incoming_body(compressed.to_vec()).await;
    let mut reader = decompress_stream(body, &[ContentEncoding::Zstd]).unwrap();
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await.unwrap();
    assert_eq!(Bytes::from(out), original);
}

#[test]
fn test_detect_request_compression_preference_br_q_zero() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::ACCEPT_ENCODING, "br;q=0".parse().unwrap());
    assert_eq!(
        detect_request_compression_preference(&headers),
        Some(ContentEncoding::Identity)
    );
}

#[test]
fn test_detect_request_compression_preference_identity_q_zero() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::ACCEPT_ENCODING,
        "identity;q=0".parse().unwrap(),
    );
    assert_eq!(detect_request_compression_preference(&headers), None);
}

#[test]
fn test_detect_request_compression_preference_wildcard_q_zero() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::ACCEPT_ENCODING, "*;q=0".parse().unwrap());
    assert_eq!(detect_request_compression_preference(&headers), None);
}

#[test]
fn test_detect_request_compression_preference_empty_entries_skipped() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::ACCEPT_ENCODING, "gzip,,br".parse().unwrap());
    assert_eq!(
        detect_request_compression_preference(&headers),
        Some(ContentEncoding::Br)
    );
}

#[test]
fn test_detect_request_compression_preference_all_q_zero() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::ACCEPT_ENCODING,
        "gzip;q=0,br;q=0,zstd;q=0".parse().unwrap(),
    );
    assert_eq!(
        detect_request_compression_preference(&headers),
        Some(ContentEncoding::Identity)
    );
}

#[test]
fn test_cap_check_rejects_oversized_length() {
    assert!(matches!(cap_check(17, 16), Err(Error::BodyTooLarge { .. })));
}

#[test]
fn test_cap_check_accepts_length_at_or_under_limit() {
    assert!(cap_check(16, 16).is_ok());
    assert!(cap_check(0, 16).is_ok());
}

#[tokio::test]
async fn test_capped_stream_rejects_oversized_data() {
    let mut reader = cap_stream(Box::new(std::io::Cursor::new(vec![0u8; 64])), 16);
    let mut out = Vec::new();
    let err = reader.read_to_end(&mut out).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(out.len() <= 16);
}

#[tokio::test]
async fn test_capped_stream_allows_data_at_limit() {
    let mut reader = cap_stream(Box::new(std::io::Cursor::new(vec![7u8; 16])), 16);
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await.unwrap();
    assert_eq!(out.len(), 16);
}

#[tokio::test]
async fn test_capped_stream_allows_data_under_limit() {
    let mut reader = cap_stream(Box::new(std::io::Cursor::new(vec![7u8; 8])), 16);
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await.unwrap();
    assert_eq!(out, vec![7u8; 8]);
}
