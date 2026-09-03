//! Wire-level tests for the pluggable [`TokenProvider`] auth mode and the
//! [`OAuth2RefreshProvider`] (RFC 6749 §6 refresh grant), using the shared
//! mock HTTP helpers. No real OAuth server involved.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use fast_dav_rs::webdav::{OAuth2RefreshProvider, TokenProvider, WebDavClient};
use fast_dav_rs::{CalDavClient, Error, RequestCompressionMode};
use hyper::{HeaderMap, Method};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A token provider returning a fixed token, counting `token()` calls.
struct StaticProvider {
    token: &'static str,
    calls: AtomicUsize,
}

impl StaticProvider {
    fn new(token: &'static str) -> Self {
        Self {
            token,
            calls: AtomicUsize::new(0),
        }
    }
}

impl TokenProvider for StaticProvider {
    fn token(&self) -> Pin<Box<dyn Future<Output = fast_dav_rs::Result<String>> + Send + '_>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let token = self.token;
        Box::pin(async move { Ok(token.to_owned()) })
    }
}

/// A token provider that always fails.
struct FailProvider;

impl TokenProvider for FailProvider {
    fn token(&self) -> Pin<Box<dyn Future<Output = fast_dav_rs::Result<String>> + Send + '_>> {
        Box::pin(async { Err(Error::InvalidInput("boom".to_owned())) })
    }
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

fn unauthorized() -> (String, Vec<u8>) {
    (
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
}

fn token_json(access: &str) -> (String, Vec<u8>) {
    let body = format!(r#"{{"access_token":"{access}","token_type":"Bearer","expires_in":3600}}"#);
    (ok(body.as_bytes()).0, body.into_bytes())
}

/// Read a full HTTP/1.1 request (headers + `Content-Length` body).
async fn read_full_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut seen = Vec::new();
    let mut buf = [0u8; 4096];
    let mut content_len = 0usize;
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        seen.extend_from_slice(&buf[..n]);
        if let Some(pos) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
            if content_len == 0 {
                let headers = String::from_utf8_lossy(&seen[..pos]);
                content_len = headers
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
            }
            if seen.len() >= pos + 4 + content_len {
                break;
            }
        }
    }
    seen
}

/// Answer every connection with the same response and capture every request.
/// Unlike `serve_capture` it supports the multiple connections a token
/// provider / auth retry produces; unlike `serve_always` it captures.
async fn serve_capture_always(
    status_line: &str,
    body: &str,
) -> (String, Arc<std::sync::Mutex<Vec<Vec<u8>>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured: Arc<std::sync::Mutex<Vec<Vec<u8>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let cap = captured.clone();
    let head = format!(
        "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let body = body.to_owned();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let head = head.clone();
            let body = body.clone();
            let cap = cap.clone();
            tokio::spawn(async move {
                let seen = read_full_request(&mut socket).await;
                cap.lock().unwrap().push(seen);
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(body.as_bytes()).await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}/"), captured)
}

#[tokio::test]
async fn token_provider_resolves_bearer_header() {
    let (base, captured) =
        crate::common::http_helpers::serve_capture(ok(b"dav").0, b"dav".to_vec()).await;
    let client = WebDavClient::builder(&base)
        .token_provider(Arc::new(StaticProvider::new("test-token")))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase()
            .contains("authorization: bearer test-token"),
        "expected provider-resolved Bearer header: {req}"
    );
}

#[tokio::test]
async fn token_provider_error_fails_request() {
    let (base, _captured) =
        crate::common::http_helpers::serve_capture(ok(b"dav").0, b"dav".to_vec()).await;
    let client = WebDavClient::builder(&base)
        .token_provider(Arc::new(FailProvider))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("boom")),
        "{err}"
    );
}

#[tokio::test]
async fn refresh_provider_fetches_then_caches() {
    let (token_url, token_reqs) =
        serve_capture_always("200 OK", r#"{"access_token":"t1","token_type":"Bearer"}"#).await;
    let (dav_base, dav_reqs) = serve_capture_always("200 OK", "<xml/>").await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "my-cid", "my-cs", "my-refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    for _ in 0..2 {
        let resp = client
            .send(Method::GET, "", HeaderMap::new(), None, None)
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    let treqs = token_reqs.lock().unwrap();
    assert_eq!(treqs.len(), 1, "token must be fetched once and cached");
    let token_req = String::from_utf8_lossy(&treqs[0]);
    assert!(token_req.contains("POST"), "{token_req}");
    assert!(
        token_req.contains(
            "grant_type=refresh_token&refresh_token=my-refresh&client_id=my-cid&client_secret=my-cs"
        ),
        "token request must be an RFC 6749 §6 refresh grant: {token_req}"
    );

    let dreqs = dav_reqs.lock().unwrap();
    assert_eq!(dreqs.len(), 2);
    for req in dreqs.iter() {
        let req = String::from_utf8_lossy(req);
        assert!(
            req.to_ascii_lowercase()
                .contains("authorization: bearer t1"),
            "both requests must carry the cached token: {req}"
        );
    }
}

#[tokio::test]
async fn refresh_on_401_retries_exactly_once() {
    // DAV: 401 then 200. Token endpoint: t1 then t2.
    let (token_url, token_reqs) =
        crate::common::http_helpers::serve_sequence(vec![token_json("t1"), token_json("t2")]).await;
    let (dav_base, dav_reqs) =
        crate::common::http_helpers::serve_sequence(vec![unauthorized(), ok(b"dav")]).await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "the retried request must succeed");

    let treqs = token_reqs.lock().unwrap();
    assert_eq!(treqs.len(), 2, "initial fetch + one renewal: {treqs:?}");
    let dreqs = dav_reqs.lock().unwrap();
    assert_eq!(dreqs.len(), 2, "original + single 401 retry: {dreqs:?}");
    assert!(
        String::from_utf8_lossy(&dreqs[0])
            .to_ascii_lowercase()
            .contains("authorization: bearer t1"),
        "first attempt used the initial token: {dreqs:?}"
    );
    assert!(
        String::from_utf8_lossy(&dreqs[1])
            .to_ascii_lowercase()
            .contains("authorization: bearer t2"),
        "retry used the refreshed token: {dreqs:?}"
    );
}

#[tokio::test]
async fn no_refresh_loop_on_persistent_401() {
    let (token_url, token_reqs) =
        serve_capture_always("200 OK", r#"{"access_token":"t1","token_type":"Bearer"}"#).await;
    let (dav_base, dav_reqs) = serve_capture_always("401 Unauthorized", "").await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "still-failing 401 is returned as-is");

    assert_eq!(token_reqs.lock().unwrap().len(), 2, "initial + one renewal");
    assert_eq!(dav_reqs.lock().unwrap().len(), 2, "no refresh loop");
}

#[tokio::test]
async fn refresh_failure_rejected_is_typed() {
    let (token_url, _token_reqs) = serve_capture_always("400 Bad Request", "").await;
    let (dav_base, _dav_reqs) = serve_capture_always("200 OK", "<xml/>").await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap_err();
    match err {
        Error::TokenRefresh {
            reason: fast_dav_rs::TokenRefreshReason::Rejected,
            status,
            ..
        } => assert_eq!(status.map(|s| s.as_u16()), Some(400)),
        other => panic!("expected TokenRefresh/Rejected, got: {other}"),
    }
}

#[tokio::test]
async fn refresh_failure_malformed_is_typed() {
    let (token_url, _token_reqs) =
        serve_capture_always("200 OK", r#"{"no_access_token_here":true}"#).await;
    let (dav_base, _dav_reqs) = serve_capture_always("200 OK", "<xml/>").await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::TokenRefresh {
                reason: fast_dav_rs::TokenRefreshReason::MalformedResponse,
                ..
            }
        ),
        "{err}"
    );
}

#[tokio::test]
async fn refresh_failure_transport_is_typed() {
    let token_url = crate::common::http_helpers::unreachable_base().await;
    let (dav_base, _dav_reqs) = serve_capture_always("200 OK", "<xml/>").await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh")
                .unwrap()
                .with_timeout(Duration::from_millis(500)),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::TokenRefresh {
                reason: fast_dav_rs::TokenRefreshReason::Transport,
                ..
            }
        ),
        "{err}"
    );
}

#[tokio::test]
async fn refresh_errors_never_leak_secrets() {
    let (token_url, _token_reqs) = serve_capture_always("400 Bad Request", "").await;
    let (dav_base, _dav_reqs) = serve_capture_always("200 OK", "<xml/>").await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "super-secret", "super-refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap_err();
    let rendered = format!("{err:?} {err}");
    assert!(!rendered.contains("super-secret"), "{rendered}");
    assert!(!rendered.contains("super-refresh"), "{rendered}");
}

#[tokio::test]
async fn auth_modes_mutually_exclusive_last_wins() {
    let (base, captured) = serve_capture_always("200 OK", "<xml/>").await;

    // token_provider after basic_auth: Bearer wins, provider is used.
    let client = WebDavClient::builder(&base)
        .basic_auth("user", "pass")
        .token_provider(Arc::new(StaticProvider::new("tok")))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    let _ = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    {
        let guard = captured.lock().unwrap();
        let req = String::from_utf8_lossy(&guard[0]);
        assert!(
            req.to_ascii_lowercase()
                .contains("authorization: bearer tok"),
            "{req}"
        );
        assert!(!req.to_ascii_lowercase().contains("basic "), "{req}");
    }
    captured.lock().unwrap().clear();

    // basic_auth after token_provider: Basic wins, provider never called.
    let provider = StaticProvider::new("tok");
    let client = WebDavClient::builder(&base)
        .token_provider(Arc::new(provider))
        .basic_auth("user", "pass")
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    let _ = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    {
        let guard = captured.lock().unwrap();
        let req = String::from_utf8_lossy(&guard[0]);
        assert!(
            req.to_ascii_lowercase().contains("authorization: basic"),
            "{req}"
        );
        assert!(!req.to_ascii_lowercase().contains("bearer"), "{req}");
    }
}

#[tokio::test]
async fn concurrent_requests_share_one_token_fetch() {
    let (token_url, token_reqs) =
        serve_capture_always("200 OK", r#"{"access_token":"t1","token_type":"Bearer"}"#).await;
    let (dav_base, dav_reqs) = serve_capture_always("200 OK", "<xml/>").await;

    let client = Arc::new(
        WebDavClient::builder(&dav_base)
            .token_provider(Arc::new(
                OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh").unwrap(),
            ))
            .build()
            .unwrap(),
    );
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let mut tasks = Vec::new();
    for _ in 0..5 {
        let client = Arc::clone(&client);
        tasks.push(tokio::spawn(async move {
            client
                .send(Method::GET, "", HeaderMap::new(), None, None)
                .await
                .unwrap()
                .status()
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), 200);
    }

    assert_eq!(
        token_reqs.lock().unwrap().len(),
        1,
        "single-flight: one refresh for five concurrent requests"
    );
    assert_eq!(dav_reqs.lock().unwrap().len(), 5);
    for req in dav_reqs.lock().unwrap().iter() {
        assert!(
            String::from_utf8_lossy(req)
                .to_ascii_lowercase()
                .contains("authorization: bearer t1"),
            "all requests carry the shared token"
        );
    }
}

#[tokio::test]
async fn expiry_triggers_refresh_without_401() {
    let (token_url, token_reqs) = crate::common::http_helpers::serve_sequence(vec![
        (
            ok(br#"{"access_token":"t1","expires_in":1}"#).0,
            br#"{"access_token":"t1","expires_in":1}"#.to_vec(),
        ),
        (
            ok(br#"{"access_token":"t2","expires_in":3600}"#).0,
            br#"{"access_token":"t2","expires_in":3600}"#.to_vec(),
        ),
    ])
    .await;
    let (dav_base, dav_reqs) = serve_capture_always("200 OK", "<xml/>").await;

    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    tokio::time::sleep(Duration::from_millis(1100)).await;

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    assert_eq!(token_reqs.lock().unwrap().len(), 2, "expiry forces renewal");
    let dreqs = dav_reqs.lock().unwrap();
    assert!(
        String::from_utf8_lossy(&dreqs[0])
            .to_ascii_lowercase()
            .contains("bearer t1")
    );
    assert!(
        String::from_utf8_lossy(&dreqs[1])
            .to_ascii_lowercase()
            .contains("bearer t2")
    );
}

#[tokio::test]
async fn refresh_token_rotation_is_adopted() {
    let (token_url, token_reqs) = crate::common::http_helpers::serve_sequence(vec![
        (
            ok(br#"{"access_token":"t1","refresh_token":"rotated"}"#).0,
            br#"{"access_token":"t1","refresh_token":"rotated"}"#.to_vec(),
        ),
        (
            ok(br#"{"access_token":"t2"}"#).0,
            br#"{"access_token":"t2"}"#.to_vec(),
        ),
    ])
    .await;

    // Force a 401 renewal: the second grant must use the rotated refresh
    // token, not the initial one.
    let (dav_base, dav_reqs) =
        crate::common::http_helpers::serve_sequence(vec![unauthorized(), ok(b"dav")]).await;
    let client = WebDavClient::builder(&dav_base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "initial").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let treqs = token_reqs.lock().unwrap();
    assert_eq!(treqs.len(), 2);
    let first = String::from_utf8_lossy(&treqs[0]);
    let second = String::from_utf8_lossy(&treqs[1]);
    assert!(first.contains("refresh_token=initial"), "{first}");
    assert!(
        second.contains("refresh_token=rotated"),
        "rotation must replace the stored refresh token: {second}"
    );
    assert!(dav_reqs.lock().unwrap().len() == 2);
}

#[test]
fn builder_debug_redacts_token_provider() {
    let builder = WebDavClient::builder("https://dav.example.com/")
        .token_provider(Arc::new(StaticProvider::new("secret-token")));
    let debug = format!("{builder:?}");
    assert!(debug.contains("<redacted>"), "{debug}");
    assert!(!debug.contains("secret-token"), "{debug}");
}

#[test]
fn caldav_and_carddav_builders_delegate_token_provider() {
    let cal = CalDavClient::builder("https://cal.example.com/dav/")
        .token_provider(Arc::new(StaticProvider::new("tok")))
        .build()
        .unwrap();
    let _ = cal;
    let card = fast_dav_rs::CardDavClient::builder("https://card.example.com/dav/")
        .token_provider(Arc::new(StaticProvider::new("tok")))
        .build()
        .unwrap();
    let _ = card;
}

#[tokio::test]
async fn caldav_delegation_uses_provider_on_the_wire() {
    let (token_url, _token_reqs) =
        serve_capture_always("200 OK", r#"{"access_token":"wt","token_type":"Bearer"}"#).await;
    let body = "<?xml version=\"1.0\"?>\
<D:multistatus xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\
<D:response><D:href>/cal/</D:href><D:propstat><D:prop>\
<C:calendar-data><![CDATA[]]></C:calendar-data>\
</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>\
</D:multistatus>";
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body.as_bytes().to_vec(),
    )
    .await;

    let client = CalDavClient::builder(&base)
        .token_provider(Arc::new(
            OAuth2RefreshProvider::new(&token_url, "cid", "cs", "refresh").unwrap(),
        ))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let periods = client
        .free_busy_query("cal/", "20240101T000000Z", "20240201T000000Z")
        .await
        .unwrap();
    let _ = periods;

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase()
            .contains("authorization: bearer wt"),
        "delegated CalDAV request must carry the provider token: {req}"
    );
}
