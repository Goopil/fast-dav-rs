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

    let etag = fast_dav_rs::webdav::etag_from_headers(&headers);
    assert_eq!(etag, Some("abc123".to_string()));
}

#[test]
fn test_etag_from_headers_missing() {
    let headers = HeaderMap::new();
    let etag = fast_dav_rs::webdav::etag_from_headers(&headers);
    assert_eq!(etag, None);
}

#[test]
fn test_etag_from_headers_invalid_utf8() {
    let mut headers = HeaderMap::new();
    // Create a header value with invalid UTF-8
    let invalid_value = HeaderValue::from_bytes(b"\xFF\xFE").unwrap();
    headers.insert("ETag", invalid_value);

    let etag = fast_dav_rs::webdav::etag_from_headers(&headers);
    assert_eq!(etag, None);
}

#[test]
fn test_etag_from_headers_multiple_values() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"first\""));
    headers.append("ETag", HeaderValue::from_static("\"second\""));

    let etag = fast_dav_rs::webdav::etag_from_headers(&headers);
    // Should return the first value
    assert_eq!(etag, Some("first".to_string()));
}

#[test]
fn test_etag_from_headers_weak_etag() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("W/\"weak123\""));

    let etag = fast_dav_rs::webdav::etag_from_headers(&headers);
    assert_eq!(etag, Some("W/weak123".to_string()));
}

#[test]
fn test_etag_from_headers_strips_quotes_and_returns_none_if_empty() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"\""));
    let etag = fast_dav_rs::webdav::etag_from_headers(&headers);
    assert_eq!(etag, None);
}

#[tokio::test]
async fn test_conditional_operations_normalize_if_match() {
    // Weak ETags (`W/"abc"`, `W/abc`) are rejected client-side since AUDIT-008
    // (RFC 9110 strong comparison) — see test_weak_etags_rejected_with_reason.
    for (etag, expected) in [("  abc  ", "\"abc\""), ("\"abc\"", "\"abc\""), ("*", "*")] {
        let (base_url, request) = capture_request().await;
        let client = CardDavClient::new(&base_url, None, None).unwrap();
        client.set_request_compression_mode(fast_dav_rs::webdav::RequestCompressionMode::Disabled);
        client
            .put_if_match("contact.vcf", Bytes::from_static(b"BEGIN:VCARD"), etag)
            .await
            .unwrap();
        let request = request.await.unwrap();
        assert!(request.starts_with("PUT "));
        assert_if_match_header(&request, expected);

        let (base_url, request) = capture_request().await;
        let client = CardDavClient::new(&base_url, None, None).unwrap();
        client.set_request_compression_mode(fast_dav_rs::webdav::RequestCompressionMode::Disabled);
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

#[tokio::test]
async fn test_if_match_rejects_bare_weak_prefix() {
    let client = CardDavClient::new("http://127.0.0.1:9/", None, None).unwrap();
    assert!(
        client
            .put_if_match("contact.vcf", Bytes::from_static(b"BEGIN:VCARD"), "W/")
            .await
            .is_err()
    );
    assert!(client.delete_if_match("contact.vcf", "W/").await.is_err());
}

#[tokio::test]
async fn test_weak_etags_rejected_with_reason_before_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let client = CardDavClient::new(&format!("http://{address}/"), None, None).unwrap();

    for etag in [r#"W/"abc""#, "W/abc"] {
        let err = client
            .put_if_match("contact.vcf", Bytes::from_static(b"BEGIN:VCARD"), etag)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                fast_dav_rs::Error::InvalidEtag {
                    reason: fast_dav_rs::EtagReason::Weak,
                    ..
                }
            ),
            "expected InvalidEtag(Weak), got {err:?}"
        );

        let err = client
            .delete_if_match("contact.vcf", etag)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                fast_dav_rs::Error::InvalidEtag {
                    reason: fast_dav_rs::EtagReason::Weak,
                    ..
                }
            ),
            "expected InvalidEtag(Weak), got {err:?}"
        );
    }

    // No connection was ever attempted: the listening socket stays silent.
    tokio::select! {
        accepted = listener.accept() => {
            panic!("client attempted network I/O: {accepted:?}")
        }
        () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
    }
}

#[tokio::test]
async fn test_etag_round_trip_from_headers_to_if_match() {
    let (base_url, request) = capture_request().await;
    let client = CardDavClient::new(&base_url, None, None).unwrap();
    client.set_request_compression_mode(fast_dav_rs::webdav::RequestCompressionMode::Disabled);

    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"etag-from-server\""));
    let etag = fast_dav_rs::webdav::etag_from_headers(&headers).expect("etag present");
    assert_eq!(etag, "etag-from-server");

    client
        .put_if_match("contact.vcf", Bytes::from_static(b"BEGIN:VCARD"), &etag)
        .await
        .unwrap();
    let request = request.await.unwrap();
    assert_if_match_header(&request, "\"etag-from-server\"");
}

#[test]
fn test_normalize_etag_strips_double_quotes_strong() {
    assert_eq!(fast_dav_rs::webdav::normalize_etag(r#""abc123""#), "abc123");
}

#[test]
fn test_normalize_etag_strips_double_quotes_weak() {
    assert_eq!(
        fast_dav_rs::webdav::normalize_etag(r#"W/"weak123""#),
        "W/weak123"
    );
}

#[test]
fn test_normalize_etag_bare_value_unchanged() {
    assert_eq!(fast_dav_rs::webdav::normalize_etag("abc123"), "abc123");
}

#[test]
fn test_normalize_etag_empty_string() {
    assert_eq!(fast_dav_rs::webdav::normalize_etag(""), "");
}

#[test]
fn test_normalize_sync_token_strips_double_quotes() {
    assert_eq!(
        fast_dav_rs::webdav::normalize_sync_token(r#""token-123""#),
        "token-123"
    );
}

#[test]
fn test_normalize_sync_token_bare_unchanged() {
    assert_eq!(
        fast_dav_rs::webdav::normalize_sync_token("http://example.com/sync/42"),
        "http://example.com/sync/42"
    );
}
