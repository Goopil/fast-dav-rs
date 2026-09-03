use fast_dav_rs::webdav::{SyncLevel, build_sync_collection_body};
use fast_dav_rs::{Error, WebDavClient};
use hyper::StatusCode;

const GONE_410: &str = "HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const NOT_FOUND_404: &str =
    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

const INITIAL_SYNC_BODY: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/b.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-b"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>http://example.com/sync/2</D:sync-token>
</D:multistatus>"#;

fn make_client(base: &str) -> WebDavClient {
    let client = WebDavClient::builder(base).build().unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);
    client
}

#[test]
fn sync_level_as_str_values() {
    assert_eq!(SyncLevel::One.as_str(), "1");
    assert_eq!(SyncLevel::Infinite.as_str(), "infinite");
}

#[test]
fn build_sync_collection_body_sends_level_one() {
    let body = build_sync_collection_body(
        Some("http://token"),
        None,
        false,
        "urn:ietf:params:xml:ns:caldav",
        "calendar-data",
        None,
        SyncLevel::One,
    );
    assert!(body.contains("<D:sync-level>1</D:sync-level>"));
    assert!(!body.contains("infinite"));
}

#[test]
fn build_sync_collection_body_sends_level_infinite() {
    let body = build_sync_collection_body(
        None,
        None,
        false,
        "urn:ietf:params:xml:ns:carddav",
        "address-data",
        None,
        SyncLevel::Infinite,
    );
    assert!(body.contains("<D:sync-level>infinite</D:sync-level>"));
}

#[tokio::test]
async fn webdav_sync_collection_with_level_sends_configured_level() {
    let head = crate::common::http_helpers::response_head("", INITIAL_SYNC_BODY.len());
    let (base, captured) =
        crate::common::http_helpers::serve_capture(head, INITIAL_SYNC_BODY.as_bytes().to_vec())
            .await;
    let client = make_client(&base);

    let (headers, items, token) = client
        .sync_collection_with_level(
            "cal/",
            None,
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
            SyncLevel::Infinite,
        )
        .await
        .unwrap();

    assert!(headers.get("Sync-Token").is_none());
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].href, "/cal/a.ics");
    assert_eq!(items[0].etag.as_deref(), Some("etag-a"));
    assert_eq!(token.as_deref(), Some("http://example.com/sync/2"));

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("<D:sync-level>infinite</D:sync-level>"),
        "expected the configured sync-level on the wire: {req}"
    );
}

#[tokio::test]
async fn webdav_sync_collection_resilient_recovers_from_410_gone() {
    let ok_head = crate::common::http_helpers::response_head("", INITIAL_SYNC_BODY.len());
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        (GONE_410.to_string(), Vec::new()),
        (ok_head, INITIAL_SYNC_BODY.as_bytes().to_vec()),
    ])
    .await;
    let client = make_client(&base);

    let (headers, items, token, resynced) = client
        .sync_collection_resilient(
            "cal/",
            Some("http://example.com/sync/stale"),
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
        )
        .await
        .unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(token.as_deref(), Some("http://example.com/sync/2"));
    assert!(headers.get("Sync-Token").is_none());
    assert!(resynced, "a 410-triggered initial sync must be flagged");

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        2,
        "410 must trigger exactly one retry: {reqs:?}"
    );
    let first = String::from_utf8_lossy(&reqs[0]);
    let second = String::from_utf8_lossy(&reqs[1]);
    assert!(
        first.contains("<D:sync-token>http://example.com/sync/stale</D:sync-token>"),
        "first request must carry the stale token: {first}"
    );
    assert!(
        first.contains("<D:sync-level>1</D:sync-level>"),
        "resilient sync uses sync-level 1: {first}"
    );
    assert!(
        second.contains("<D:sync-token/>"),
        "retry must be an initial sync with an empty token: {second}"
    );
}

#[tokio::test]
async fn webdav_sync_collection_resilient_resyncs_on_403_valid_sync_token() {
    const VALID_SYNC_TOKEN_BODY: &str = r#"<?xml version="1.0"?>
<D:error xmlns:D="DAV:">
  <D:valid-sync-token/>
</D:error>
"#;
    let forbidden_head = format!(
        "HTTP/1.1 403 Forbidden\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        VALID_SYNC_TOKEN_BODY.len()
    );

    let ok_head = crate::common::http_helpers::response_head("", INITIAL_SYNC_BODY.len());
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        (forbidden_head, VALID_SYNC_TOKEN_BODY.as_bytes().to_vec()),
        (ok_head, INITIAL_SYNC_BODY.as_bytes().to_vec()),
    ])
    .await;
    let client = make_client(&base);

    let (_, items, token, resynced) = client
        .sync_collection_resilient(
            "cal/",
            Some("http://example.com/sync/stale"),
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
        )
        .await
        .unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(token.as_deref(), Some("http://example.com/sync/2"));
    assert!(
        resynced,
        "403 + valid-sync-token must be treated as a stale token (RFC 6578 §3.2)"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "the stale signal must trigger one retry");
    let second = String::from_utf8_lossy(&reqs[1]);
    assert!(
        second.contains("<D:sync-token/>"),
        "retry must be an initial sync with an empty token: {second}"
    );
}

#[tokio::test]
async fn webdav_sync_collection_resilient_propagates_403_without_valid_sync_token() {
    const OTHER_PRECONDITION_BODY: &str = r#"<?xml version="1.0"?>
<D:error xmlns:D="DAV:">
  <D:need-privileges/>
</D:error>
"#;
    let forbidden_head = format!(
        "HTTP/1.1 403 Forbidden\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        OTHER_PRECONDITION_BODY.len()
    );

    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![(
        forbidden_head,
        OTHER_PRECONDITION_BODY.as_bytes().to_vec(),
    )])
    .await;
    let client = make_client(&base);

    let err = client
        .sync_collection_resilient(
            "cal/",
            Some("http://example.com/sync/t"),
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::UnexpectedStatus { status, .. } if status == StatusCode::FORBIDDEN),
        "a 403 without valid-sync-token is not a stale signal: {err}"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 1, "no retry for a plain 403");
}

#[tokio::test]
async fn webdav_sync_collection_resilient_propagates_non_410_status() {
    let (base, captured) =
        crate::common::http_helpers::serve_sequence(vec![(NOT_FOUND_404.to_string(), Vec::new())])
            .await;
    let client = make_client(&base);

    let err = client
        .sync_collection_resilient(
            "cal/",
            Some("http://example.com/sync/t"),
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::UnexpectedStatus { status, .. } if status == StatusCode::NOT_FOUND),
        "non-410 statuses must propagate unchanged: {err}"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 1, "no retry for non-410 statuses");
}

#[tokio::test]
async fn webdav_sync_collection_resilient_does_not_retry_on_success() {
    let head = crate::common::http_helpers::response_head("", INITIAL_SYNC_BODY.len());
    let (base, captured) =
        crate::common::http_helpers::serve_capture(head, INITIAL_SYNC_BODY.as_bytes().to_vec())
            .await;
    let client = make_client(&base);

    let (_, items, token, resynced) = client
        .sync_collection_resilient(
            "cal/",
            Some("http://example.com/sync/fresh"),
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
        )
        .await
        .unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(token.as_deref(), Some("http://example.com/sync/2"));
    assert!(!resynced, "a successful incremental sync is not a resync");

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("<D:sync-token>http://example.com/sync/fresh</D:sync-token>"),
        "successful incremental sync must not be retried: {req}"
    );
}
