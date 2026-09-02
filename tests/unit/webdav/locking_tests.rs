use fast_dav_rs::webdav::{LockScope, parse_lock_discovery_bytes};
use fast_dav_rs::{CalDavClient, Error, Operation, RequestCompressionMode, WebDavClient};

const LOCK_OK_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:">
  <D:lockdiscovery>
    <D:activelock>
      <D:locktype><D:write/></D:locktype>
      <D:lockscope><D:exclusive/></D:lockscope>
      <D:depth>0</D:depth>
      <D:owner><D:href>https://example.com/alice</D:href></D:owner>
      <D:timeout>Second-300</D:timeout>
      <D:locktoken><D:href>opaquelocktoken:e71d4fae-5dec-22d6-fea5-00a0c91e6be4</D:href></D:locktoken>
    </D:activelock>
  </D:lockdiscovery>
</D:prop>"#;

fn http_head(status_line: &str, extra_headers: &str, body: &str) -> String {
    format!(
        "{status_line}\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n",
        body.len()
    )
}

async fn dav_client(base: &str) -> WebDavClient {
    let client = WebDavClient::builder(base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

#[tokio::test]
async fn lock_sends_lockinfo_body_and_parses_activelock() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", LOCK_OK_BODY.len()),
        LOCK_OK_BODY.as_bytes().to_vec(),
    )
    .await;
    let client = dav_client(&base).await;

    let lock = client
        .lock(
            "docs/plan.txt",
            LockScope::Exclusive,
            "<D:href>https://example.com/alice</D:href>",
            Some(300),
        )
        .await
        .unwrap();
    assert_eq!(
        lock.token,
        "opaquelocktoken:e71d4fae-5dec-22d6-fea5-00a0c91e6be4"
    );
    assert_eq!(lock.timeout_secs, Some(300));
    assert_eq!(lock.scope, Some(LockScope::Exclusive));
    assert_eq!(lock.owner.as_deref(), Some("https://example.com/alice"));

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    let lower = req.to_ascii_lowercase();
    assert!(lower.contains("lock /"), "expected LOCK method: {req}");
    assert!(
        lower.contains("timeout: second-300"),
        "expected Timeout header: {req}"
    );
    assert!(
        lower.contains("content-type: application/xml"),
        "expected XML content type: {req}"
    );
    for fragment in [
        "<D:lockinfo",
        "<D:lockscope><D:exclusive/></D:lockscope>",
        "<D:locktype><D:write/></D:locktype>",
        "<D:owner><D:href>https://example.com/alice</D:href></D:owner>",
    ] {
        assert!(
            req.contains(fragment),
            "expected `{fragment}` in body: {req}"
        );
    }
}

#[tokio::test]
async fn lock_shared_scope_sends_shared_element() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", LOCK_OK_BODY.len()),
        LOCK_OK_BODY.as_bytes().to_vec(),
    )
    .await;
    let client = dav_client(&base).await;

    client
        .lock("docs/plan.txt", LockScope::Shared, "", None)
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("<D:lockscope><D:shared/></D:lockscope>"),
        "expected shared scope in body: {req}"
    );
    assert!(
        !req.contains("<D:owner"),
        "empty owner should omit the element: {req}"
    );
    assert!(
        !req.to_ascii_lowercase().contains("timeout:"),
        "no timeout requested: {req}"
    );
}

#[tokio::test]
async fn lock_423_returns_unexpected_status_lock() {
    let head = http_head("HTTP/1.1 423 Locked", "", "");
    let (base, _captured) = crate::common::http_helpers::serve_capture(head, Vec::new()).await;
    let client = dav_client(&base).await;

    let err = client
        .lock("docs/plan.txt", LockScope::Exclusive, "", Some(60))
        .await
        .unwrap_err();
    match err {
        Error::UnexpectedStatus {
            operation, status, ..
        } => {
            assert_eq!(operation, Operation::Lock);
            assert_eq!(status.as_u16(), 423);
        }
        other => panic!("expected UnexpectedStatus, got: {other:?}"),
    }
}

#[tokio::test]
async fn lock_timeout_falls_back_to_response_header() {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:">
  <D:lockdiscovery>
    <D:activelock>
      <D:locktoken><D:href>opaquelocktoken:from-header</D:href></D:locktoken>
    </D:activelock>
  </D:lockdiscovery>
</D:prop>"#;
    let head = http_head("HTTP/1.1 200 OK", "Timeout: Second-600\r\n", body);
    let (base, _captured) =
        crate::common::http_helpers::serve_capture(head, body.as_bytes().to_vec()).await;
    let client = dav_client(&base).await;

    let lock = client
        .lock("docs/plan.txt", LockScope::Exclusive, "", None)
        .await
        .unwrap();
    assert_eq!(lock.timeout_secs, Some(600));
    assert_eq!(lock.scope, None);
    assert_eq!(lock.owner, None);
}

#[tokio::test]
async fn refresh_lock_sends_if_header_and_no_body() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", LOCK_OK_BODY.len()),
        LOCK_OK_BODY.as_bytes().to_vec(),
    )
    .await;
    let client = dav_client(&base).await;

    let lock = client
        .refresh_lock("docs/plan.txt", "opaquelocktoken:old-token", Some(600))
        .await
        .unwrap();
    assert_eq!(
        lock.token,
        "opaquelocktoken:e71d4fae-5dec-22d6-fea5-00a0c91e6be4"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    let lower = req.to_ascii_lowercase();
    assert!(lower.contains("lock /"), "expected LOCK method: {req}");
    assert!(
        req.contains("if: (<opaquelocktoken:old-token>)"),
        "expected If header with lock token: {req}"
    );
    assert!(
        lower.contains("timeout: second-600"),
        "expected Timeout header: {req}"
    );
    assert!(
        !req.contains("lockinfo"),
        "refresh LOCK must not carry a lockinfo body: {req}"
    );
}

#[tokio::test]
async fn refresh_lock_empty_token_rejected() {
    let client = dav_client("http://127.0.0.1:1/").await;
    let err = client
        .refresh_lock("docs/plan.txt", "   ", None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "got: {err:?}");
}

#[tokio::test]
async fn unlock_sends_lock_token_header() {
    let head = http_head("HTTP/1.1 204 No Content", "", "");
    let (base, captured) = crate::common::http_helpers::serve_capture(head, Vec::new()).await;
    let client = dav_client(&base).await;

    client
        .unlock(
            "docs/plan.txt",
            "opaquelocktoken:e71d4fae-5dec-22d6-fea5-00a0c91e6be4",
        )
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    let lower = req.to_ascii_lowercase();
    assert!(lower.contains("unlock /"), "expected UNLOCK method: {req}");
    assert!(
        lower.contains("lock-token: <opaquelocktoken:e71d4fae-5dec-22d6-fea5-00a0c91e6be4>"),
        "expected Lock-Token header: {req}"
    );
}

#[tokio::test]
async fn unlock_non_success_returns_unexpected_status_unlock() {
    let head = http_head("HTTP/1.1 409 Conflict", "", "");
    let (base, _captured) = crate::common::http_helpers::serve_capture(head, Vec::new()).await;
    let client = dav_client(&base).await;

    let err = client
        .unlock("docs/plan.txt", "opaquelocktoken:xyz")
        .await
        .unwrap_err();
    match err {
        Error::UnexpectedStatus {
            operation, status, ..
        } => {
            assert_eq!(operation, Operation::Unlock);
            assert_eq!(status.as_u16(), 409);
        }
        other => panic!("expected UnexpectedStatus, got: {other:?}"),
    }
}

#[tokio::test]
async fn unlock_empty_token_rejected() {
    let client = dav_client("http://127.0.0.1:1/").await;
    let err = client.unlock("docs/plan.txt", "").await.unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "got: {err:?}");
}

#[tokio::test]
async fn caldav_client_lock_delegates_to_webdav() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", LOCK_OK_BODY.len()),
        LOCK_OK_BODY.as_bytes().to_vec(),
    )
    .await;
    let client = CalDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let lock = client
        .lock(
            "cal/event.ics",
            LockScope::Exclusive,
            "<D:href>https://example.com/alice</D:href>",
            Some(120),
        )
        .await
        .unwrap();
    assert_eq!(
        lock.token,
        "opaquelocktoken:e71d4fae-5dec-22d6-fea5-00a0c91e6be4"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase().contains("lock /"),
        "expected LOCK method via CalDavClient: {req}"
    );
}

// ----------- parse_lock_discovery_bytes (pure) -----------

#[test]
fn parse_lock_discovery_full_doc() {
    let lock = parse_lock_discovery_bytes(LOCK_OK_BODY.as_bytes()).unwrap();
    assert_eq!(
        lock.token,
        "opaquelocktoken:e71d4fae-5dec-22d6-fea5-00a0c91e6be4"
    );
    assert_eq!(lock.timeout_secs, Some(300));
    assert_eq!(lock.scope, Some(LockScope::Exclusive));
    assert_eq!(lock.owner.as_deref(), Some("https://example.com/alice"));
}

#[test]
fn parse_lock_discovery_shared_scope_and_text_owner() {
    let xml = br#"<D:prop xmlns:D="DAV:">
  <D:lockdiscovery>
    <D:activelock>
      <D:lockscope><D:shared/></D:lockscope>
      <D:owner>Alice</D:owner>
    </D:activelock>
  </D:lockdiscovery>
</D:prop>"#;
    let lock = parse_lock_discovery_bytes(xml).unwrap();
    assert_eq!(lock.scope, Some(LockScope::Shared));
    assert_eq!(lock.owner.as_deref(), Some("Alice"));
    assert_eq!(lock.token, "");
    assert_eq!(lock.timeout_secs, None);
}

#[test]
fn parse_lock_discovery_missing_fields_is_lenient() {
    let xml = br#"<D:prop xmlns:D="DAV:">
  <D:lockdiscovery>
    <D:activelock>
      <D:depth>0</D:depth>
    </D:activelock>
  </D:lockdiscovery>
</D:prop>"#;
    let lock = parse_lock_discovery_bytes(xml).unwrap();
    assert_eq!(lock, fast_dav_rs::webdav::LockInfo::default());
}

#[test]
fn parse_lock_discovery_no_activelock_returns_default() {
    let xml = br#"<D:prop xmlns:D="DAV:"><D:lockdiscovery/></D:prop>"#;
    assert_eq!(
        parse_lock_discovery_bytes(xml).unwrap(),
        fast_dav_rs::webdav::LockInfo::default()
    );
    assert_eq!(
        parse_lock_discovery_bytes(b"").unwrap(),
        fast_dav_rs::webdav::LockInfo::default()
    );
}

#[test]
fn parse_lock_discovery_timeout_infinite_is_none() {
    let xml = br#"<D:prop xmlns:D="DAV:"><D:lockdiscovery>
    <D:activelock><D:timeout>Infinite</D:timeout></D:activelock>
</D:lockdiscovery></D:prop>"#;
    let lock = parse_lock_discovery_bytes(xml).unwrap();
    assert_eq!(lock.timeout_secs, None);
}

#[test]
fn parse_lock_discovery_timeout_list_takes_first_second() {
    let xml = br#"<D:prop xmlns:D="DAV:"><D:lockdiscovery>
    <D:activelock><D:timeout>Second-600, Second-1200</D:timeout></D:activelock>
</D:lockdiscovery></D:prop>"#;
    let lock = parse_lock_discovery_bytes(xml).unwrap();
    assert_eq!(lock.timeout_secs, Some(600));
}

#[test]
fn parse_lock_discovery_first_activelock_wins() {
    let xml = br#"<D:prop xmlns:D="DAV:"><D:lockdiscovery>
    <D:activelock><D:locktoken><D:href>opaquelocktoken:first</D:href></D:locktoken></D:activelock>
    <D:activelock><D:locktoken><D:href>opaquelocktoken:second</D:href></D:locktoken></D:activelock>
</D:lockdiscovery></D:prop>"#;
    let lock = parse_lock_discovery_bytes(xml).unwrap();
    assert_eq!(lock.token, "opaquelocktoken:first");
}

#[test]
fn parse_lock_discovery_invalid_xml_errors() {
    assert!(parse_lock_discovery_bytes(b"<D:prop").is_err());
}
