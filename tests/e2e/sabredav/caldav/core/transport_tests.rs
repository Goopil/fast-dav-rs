//! Transport-level e2e coverage (#117, AUDIT-017): chunked transfer responses
//! through the real client stack, Basic/Bearer auth on the wire, and the
//! honest HTTP-version check against the plain-http Sabre/DAV stack.

use fast_dav_rs::webdav::WebDavClient;
use fast_dav_rs::{CalDavClient, Depth, RequestCompressionMode};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

const MULTISTATUS_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/calendars/test/chunked.ics</d:href>
    <d:propstat>
      <d:prop><d:getetag>"chunked-etag-1"</d:getetag></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/calendars/test/chunked-2.ics</d:href>
    <d:propstat>
      <d:prop><d:getetag>"chunked-etag-2"</d:getetag></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

/// Read until the end of the HTTP request headers (or EOF/timeout).
async fn read_request_head(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    let mut seen = Vec::new();
    loop {
        let n = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buf))
            .await
            .unwrap_or(Ok(0))
            .unwrap_or(0);
        if n == 0 || seen.windows(4).any(|w| w == b"\r\n\r\n") {
            return seen;
        }
        seen.extend_from_slice(&buf[..n]);
    }
}

/// Serve ONE HTTP/1.1 response with `Transfer-Encoding: chunked` on an
/// ephemeral port; the body is split into fixed-size chunks (possibly cutting
/// mid-XML) to force the client to reassemble the chunked stream.
async fn spawn_chunked_server(body: &str, chunk_size: usize) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "http://127.0.0.1:{}/",
        listener.local_addr().unwrap().port()
    );
    let body = body.to_owned();
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let _ = read_request_head(&mut socket).await;

        let head = "HTTP/1.1 207 Multi-Status\r\n\
                    Content-Type: application/xml; charset=utf-8\r\n\
                    Transfer-Encoding: chunked\r\n\
                    Connection: close\r\n\r\n";
        let _ = socket.write_all(head.as_bytes()).await;
        for chunk in body.as_bytes().chunks(chunk_size) {
            let _ = socket
                .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                .await;
            let _ = socket.write_all(chunk).await;
            let _ = socket.write_all(b"\r\n").await;
        }
        let _ = socket.write_all(b"0\r\n\r\n").await;
        let _ = socket.shutdown().await;
    });
    base
}

/// Serve ONE HTTP/1.1 exchange; the response body echoes the request's
/// `Authorization` header value (or `<absent>`), letting the test assert on
/// the credentials that actually hit the wire.
async fn spawn_echo_auth_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "http://127.0.0.1:{}/",
        listener.local_addr().unwrap().port()
    );
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let head = read_request_head(&mut socket).await;
        let headers = String::from_utf8_lossy(&head);
        let auth = headers
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .starts_with("authorization: ")
                    .then(|| l["authorization: ".len()..].trim().to_owned())
            })
            .unwrap_or_else(|| "<absent>".to_owned());
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:"><d:response><d:href>/</d:href><d:propstat><d:prop><d:displayname>{auth}</d:displayname></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#
        );
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/xml; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(resp.as_bytes()).await;
        let _ = socket.shutdown().await;
    });
    base
}

fn propfind_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop><D:getetag/></D:prop>
</D:propfind>"#
}

/// AUDIT-017: chunked transfer responses must be reassembled and parsed by
/// the real client stack (hyper aggregates the chunks; the XML parser must
/// handle chunk boundaries cutting mid-element).
#[tokio::test]
async fn test_chunked_transfer_response_through_real_client() {
    // Chunk size 17 deliberately cuts mid-tag in the XML body.
    let base = spawn_chunked_server(MULTISTATUS_BODY, 17).await;
    let client =
        WebDavClient::new(&base, Some(TEST_USER), Some(TEST_PASS)).expect("client construction");
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .propfind("calendars/test/", Depth::Zero, propfind_xml())
        .await
        .expect("chunked PROPFIND must succeed");
    assert_eq!(
        resp.status().as_u16(),
        207,
        "expected the chunked 207 Multi-Status to come through intact"
    );

    let items = fast_dav_rs::parse_multistatus_bytes(resp.body())
        .expect("chunk boundaries must be invisible to the XML parser");
    assert_eq!(
        items.items.len(),
        2,
        "both multistatus entries must survive"
    );
    assert!(
        items
            .items
            .iter()
            .any(|i| i.href.ends_with("chunked.ics") && i.etag.as_deref() == Some("chunked-etag-1")),
        "first entry must be parsed with its (normalized) ETag, got: {:?}",
        items.items
    );
}

/// AUDIT-017: Basic auth credentials are actually sent on the wire
/// (preemptive `Authorization: Basic` header) — verified with an echo server
/// through the real client stack, no proxy involved.
#[tokio::test]
async fn test_basic_auth_reaches_the_wire() {
    // base64("test:test") — fixed constant, no base64 crate in dev-deps.
    const EXPECTED: &str = "Basic dGVzdDp0ZXN0";
    let base = spawn_echo_auth_server().await;
    let client =
        WebDavClient::new(&base, Some(TEST_USER), Some(TEST_PASS)).expect("client construction");
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .propfind("calendars/test/", Depth::Zero, propfind_xml())
        .await
        .expect("authenticated request must succeed");
    assert!(resp.status().is_success());
    let body = String::from_utf8_lossy(resp.body());
    assert!(
        body.contains(EXPECTED),
        "Authorization: Basic <creds> must reach the wire verbatim, echoed body: {body}"
    );
}

/// AUDIT-017: same wire proof for Bearer tokens.
#[tokio::test]
async fn test_bearer_auth_reaches_the_wire() {
    let base = spawn_echo_auth_server().await;
    let client = WebDavClient::builder(&base)
        .bearer_token("wave3-token-123")
        .build()
        .expect("client construction");
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .propfind("calendars/test/", Depth::Zero, propfind_xml())
        .await
        .expect("bearer request must succeed");
    assert!(resp.status().is_success());
    let body = String::from_utf8_lossy(resp.body());
    assert!(
        body.contains("Bearer wave3-token-123"),
        "Authorization: Bearer <token> must reach the wire verbatim, echoed body: {body}"
    );
}

/// h2 honesty check (AUDIT-017/029): the Docker Sabre/DAV stack is plain
/// `http://` behind nginx + PHP, so it cannot negotiate HTTP/2 (h2 requires
/// TLS/ALPN; hyper does h1.1 on cleartext). Assert HTTP/1.1 explicitly so the
/// limitation is pinned at the e2e layer instead of silently assuming it.
/// The client's h2 capability (over TLS) is a builder fact (`enable_http2`),
/// asserted by unit tests; an h2 end-to-end run needs a TLS-terminating
/// server this stack does not provide (documented exemption, PR #117 body).
#[tokio::test]
async fn test_sabredav_negotiates_http_1_1_only() {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("client construction");
    let resp = client
        .options("")
        .await
        .expect("OPTIONS against Sabre/DAV must succeed");
    assert!(resp.status().is_success());
    let version = format!("{:?}", resp.version());
    assert_eq!(
        version, "HTTP/1.1",
        "the plain-http Sabre/DAV stack must answer HTTP/1.1 (no h2c)"
    );
}
