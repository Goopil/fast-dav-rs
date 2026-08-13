use bytes::Bytes;
use fast_dav_rs::CardDavClient;
use hyper::http::{HeaderMap, HeaderValue};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn capture_request() -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            request.extend_from_slice(&buffer[..count]);
            if let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                request.truncate(headers_end + 4);
                break;
            }
        }
        sender.send(String::from_utf8(request).unwrap()).unwrap();
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    });

    (format!("http://{address}/"), receiver)
}

fn assert_if_match_header(request: &str, expected: &str) {
    assert!(request.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("if-match") && value.trim() == expected
        })
    }));
}

#[test]
fn test_etag_from_headers_present() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"abc123\""));

    let etag = CardDavClient::etag_from_headers(&headers);
    assert_eq!(etag, Some("abc123".to_string()));
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
    assert_eq!(etag, Some("first".to_string()));
}

#[test]
fn test_etag_from_headers_weak_etag() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("W/\"weak123\""));

    let etag = CardDavClient::etag_from_headers(&headers);
    assert_eq!(etag, Some("W/weak123".to_string()));
}

#[test]
fn test_etag_from_headers_strips_quotes_and_returns_none_if_empty() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"\""));
    let etag = CardDavClient::etag_from_headers(&headers);
    assert_eq!(etag, None);
}

#[tokio::test]
async fn test_conditional_operations_normalize_if_match() {
    for (etag, expected) in [
        ("  abc  ", "\"abc\""),
        ("\"abc\"", "\"abc\""),
        ("W/\"abc\"", "W/\"abc\""),
        ("W/abc", "W/\"abc\""),
        ("*", "*"),
    ] {
        let (base_url, request) = capture_request().await;
        let client = CardDavClient::new(&base_url, None, None).unwrap();
        client.disable_request_compression();
        client
            .put_if_match("contact.vcf", Bytes::from_static(b"BEGIN:VCARD"), etag)
            .await
            .unwrap();
        let request = request.await.unwrap();
        assert!(request.starts_with("PUT "));
        assert_if_match_header(&request, expected);

        let (base_url, request) = capture_request().await;
        let client = CardDavClient::new(&base_url, None, None).unwrap();
        client.disable_request_compression();
        client.delete_if_match("contact.vcf", etag).await.unwrap();
        let request = request.await.unwrap();
        assert!(request.starts_with("DELETE "));
        assert_if_match_header(&request, expected);
    }
}

#[tokio::test]
async fn test_conditional_operations_reject_invalid_etags_before_request() {
    let client = CardDavClient::new("http://127.0.0.1:9/", None, None).unwrap();

    for etag in ["", "   ", "\"abc", "abc\ndef"] {
        assert!(
            client
                .put_if_match("contact.vcf", Bytes::from_static(b"BEGIN:VCARD"), etag)
                .await
                .is_err()
        );
        assert!(client.delete_if_match("contact.vcf", etag).await.is_err());
    }
}
