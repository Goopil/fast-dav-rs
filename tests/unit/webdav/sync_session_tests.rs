//! Wire tests for the [`SyncSession`] engine (issue #160): a mock HTTP
//! server drives the supported / unsupported / stale-token / truncation
//! paths of the DAVx⁵ algorithm.

use fast_dav_rs::{CalDavClient, RequestCompressionMode, WebDavClient};

use crate::common::http_helpers::{response_head, serve_sequence};

const SYNC_SUPPORTED_PROPFIND: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:supported-report-set>
          <D:supported-report>
            <D:report><D:sync-collection/></D:report>
          </D:supported-report>
        </D:supported-report-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

const PLAIN_PROPFIND: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
  </D:response>
</D:multistatus>"#;

/// Two members (a, b) with etags and a fresh sync token.
const INITIAL_BODY: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/b.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-b1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>token-2</D:sync-token>
</D:multistatus>"#;

/// Incremental delta: a deleted (404), b modified, c added.
const DELTA_BODY: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:status>HTTP/1.1 404 Not Found</D:status>
  </D:response>
  <D:response>
    <D:href>/cal/b.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-b2"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/c.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-c1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>token-3</D:sync-token>
</D:multistatus>"#;

/// Full listing used by the fallback path (PROPFIND Depth: 1); the first
/// response element is the collection itself.
const LIST_AB: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/b.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-b1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

/// Second listing: a modified, b gone, c added.
const LIST_AC: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a2"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/c.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-c1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

/// Empty incremental page (fresh token, no changes).
const EMPTY_DELTA_BODY: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:sync-token>token-3</D:sync-token>
</D:multistatus>"#;

const GONE_410: &str = "HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// `403` + `<D:valid-sync-token/>`: Radicale's observed stale-token signal.
fn valid_sync_token_403() -> (String, Vec<u8>) {
    const BODY: &str = r#"<?xml version="1.0"?>
<D:error xmlns:D="DAV:">
  <D:valid-sync-token/>
</D:error>"#;
    (
        format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            BODY.len()
        ),
        BODY.as_bytes().to_vec(),
    )
}

/// `403` + a non-stale precondition (report not supported / permissions).
fn report_not_supported_403() -> (String, Vec<u8>) {
    const BODY: &str = r#"<?xml version="1.0"?>
<D:error xmlns:D="DAV:">
  <D:report-not-supported/>
</D:error>"#;
    (
        format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            BODY.len()
        ),
        BODY.as_bytes().to_vec(),
    )
}

fn multistatus_response(body: &str) -> (String, Vec<u8>) {
    (response_head("", body.len()), body.as_bytes().to_vec())
}

fn make_client(base: &str) -> WebDavClient {
    let client = WebDavClient::new(base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

fn make_caldav_client(base: &str) -> CalDavClient {
    let client = CalDavClient::new(base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

#[tokio::test]
async fn sync_session_supported_initial_and_incremental_produce_typed_deltas() {
    let (base, captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        multistatus_response(INITIAL_BODY),
        multistatus_response(DELTA_BODY),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");

    let snapshot = session.initial().await.unwrap();
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(snapshot.items[0].href, "/cal/a.ics");
    assert_eq!(snapshot.items[0].etag.as_deref(), Some("etag-a1"));
    assert_eq!(snapshot.sync_token.as_deref(), Some("token-2"));
    assert_eq!(session.sync_token().as_deref(), Some("token-2"));

    let delta = session.incremental().await.unwrap();
    assert!(!delta.resynced);
    assert_eq!(delta.added.len(), 1);
    assert_eq!(delta.added[0].href, "/cal/c.ics");
    assert_eq!(delta.added[0].etag.as_deref(), Some("etag-c1"));
    assert_eq!(delta.modified.len(), 1);
    assert_eq!(delta.modified[0].href, "/cal/b.ics");
    assert_eq!(delta.modified[0].etag.as_deref(), Some("etag-b2"));
    assert_eq!(delta.deleted, vec!["/cal/a.ics".to_string()]);
    assert_eq!(delta.sync_token.as_deref(), Some("token-3"));

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "probe + initial + incremental");
    let probe = String::from_utf8_lossy(&reqs[0]);
    assert!(
        probe.contains("supported-report-set"),
        "first request must be the capability probe: {probe}"
    );
    let initial = String::from_utf8_lossy(&reqs[1]);
    assert!(
        initial.contains("<D:sync-token/>"),
        "initial sync must use an empty token: {initial}"
    );
    let incremental = String::from_utf8_lossy(&reqs[2]);
    assert!(
        incremental.contains("<D:sync-token>token-2</D:sync-token>"),
        "incremental must carry the stored token: {incremental}"
    );
}

#[tokio::test]
async fn sync_session_incremental_without_prior_state_reports_full_state_as_added() {
    let (base, _captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        multistatus_response(INITIAL_BODY),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");

    let delta = session.incremental().await.unwrap();
    assert!(!delta.resynced, "the first sync is not a resync");
    assert_eq!(delta.added.len(), 2, "no prior state: everything is added");
    assert!(delta.deleted.is_empty());
    assert_eq!(delta.sync_token.as_deref(), Some("token-2"));
}

#[tokio::test]
async fn sync_session_unsupported_server_falls_back_to_propfind_diff() {
    // Probe: the PROPFIND does not advertise sync-collection and the minimal
    // REPORT is rejected with 403 -> Unsupported; the session then serves
    // initial() and incremental() from PROPFIND Depth: 1 + etag diff.
    let (base, captured) = serve_sequence(vec![
        multistatus_response(PLAIN_PROPFIND),
        report_not_supported_403(),
        multistatus_response(LIST_AB),
        multistatus_response(LIST_AC),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");

    let snapshot = session.initial().await.unwrap();
    assert_eq!(snapshot.items.len(), 2, "collection entry must be skipped");
    assert_eq!(snapshot.sync_token, None, "fallback has no server token");

    let delta = session.incremental().await.unwrap();
    assert!(!delta.resynced);
    assert_eq!(
        delta
            .added
            .iter()
            .map(|e| e.href.as_str())
            .collect::<Vec<_>>(),
        vec!["/cal/c.ics"]
    );
    assert_eq!(
        delta
            .modified
            .iter()
            .map(|e| e.href.as_str())
            .collect::<Vec<_>>(),
        vec!["/cal/a.ics"]
    );
    assert_eq!(delta.modified[0].etag.as_deref(), Some("etag-a2"));
    assert_eq!(delta.deleted, vec!["/cal/b.ics".to_string()]);

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 4, "probe x2 + two PROPFINDs");
    let last = String::from_utf8_lossy(&reqs[3]);
    assert!(
        last.starts_with("PROPFIND") && last.contains("<D:getetag/>"),
        "the fallback must be a PROPFIND etag list: {last}"
    );
}

#[tokio::test]
async fn sync_session_403_during_sync_downgrades_to_fallback() {
    // The probe confirms support, but the incremental report is rejected
    // with a plain 403 (e.g. report-not-supported): the session must
    // permanently downgrade to the full-list path.
    let (base, captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        multistatus_response(INITIAL_BODY),
        report_not_supported_403(),
        multistatus_response(LIST_AC),
        multistatus_response(LIST_AC),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");

    let snapshot = session.initial().await.unwrap();
    assert_eq!(snapshot.sync_token.as_deref(), Some("token-2"));

    let delta = session.incremental().await.unwrap();
    assert!(!delta.resynced);
    assert_eq!(
        delta
            .added
            .iter()
            .map(|e| e.href.as_str())
            .collect::<Vec<_>>(),
        vec!["/cal/c.ics"]
    );
    assert_eq!(delta.deleted, vec!["/cal/b.ics".to_string()]);

    // The downgrade sticks: the next sync goes straight to PROPFIND.
    let next = session.incremental().await.unwrap();
    assert!(next.added.is_empty() && next.modified.is_empty() && next.deleted.is_empty());

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 5, "probe + initial + 403 + two PROPFINDs");
    let second_propfind = String::from_utf8_lossy(&reqs[4]);
    assert!(
        second_propfind.starts_with("PROPFIND"),
        "after the downgrade no further REPORT is sent: {second_propfind}"
    );
}

#[tokio::test]
async fn sync_session_caldav_fallback_fetches_content_via_multiget() {
    const MULTIGET_AB: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-a1"</D:getetag>
        <C:calendar-data>BEGIN:VCALENDAR...a</C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/b.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-b1"</D:getetag>
        <C:calendar-data>BEGIN:VCALENDAR...b</C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    // The second multiget omits c's data: the entry must survive with
    // data = None (the server did not return the payload).
    const MULTIGET_AC: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-a2"</D:getetag>
        <C:calendar-data>BEGIN:VCALENDAR...a2</C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let (base, captured) = serve_sequence(vec![
        multistatus_response(PLAIN_PROPFIND),
        report_not_supported_403(),
        multistatus_response(LIST_AB),
        multistatus_response(MULTIGET_AB),
        multistatus_response(LIST_AC),
        multistatus_response(MULTIGET_AC),
    ])
    .await;
    let session = make_caldav_client(&base).sync_session("cal/");

    let snapshot = session.initial().await.unwrap();
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(
        snapshot.items[0].data.as_deref(),
        Some("BEGIN:VCALENDAR...a"),
        "initial fallback must fetch content via calendar-multiget"
    );

    let delta = session.incremental().await.unwrap();
    assert_eq!(delta.modified.len(), 1);
    assert_eq!(
        delta.modified[0].data.as_deref(),
        Some("BEGIN:VCALENDAR...a2")
    );
    assert_eq!(delta.added.len(), 1);
    assert_eq!(delta.added[0].href, "/cal/c.ics");
    assert!(delta.added[0].data.is_none(), "server sent no data for c");

    let reqs = captured.lock().unwrap();
    let multiget1 = String::from_utf8_lossy(&reqs[3]);
    assert!(
        multiget1.starts_with("REPORT") && multiget1.contains("calendar-multiget"),
        "content must be fetched via calendar-multiget REPORTs: {multiget1}"
    );
    assert!(
        multiget1.contains("/cal/a.ics") && multiget1.contains("/cal/b.ics"),
        "the multiget must request the snapshot hrefs: {multiget1}"
    );
}

#[tokio::test]
async fn sync_session_caldav_fallback_propagates_multiget_failure() {
    let (base, _captured) = serve_sequence(vec![
        multistatus_response(PLAIN_PROPFIND),
        report_not_supported_403(),
        multistatus_response(LIST_AB),
        (
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_string(),
            Vec::new(),
        ),
    ])
    .await;
    let session = make_caldav_client(&base).sync_session("cal/");

    let err = session
        .initial()
        .await
        .expect_err("a failed multiget chunk must fail the snapshot");
    assert!(
        matches!(err, fast_dav_rs::Error::UnexpectedStatus { .. }),
        "the multiget failure must surface: {err}"
    );
}

#[tokio::test]
async fn sync_session_resyncs_transparently_on_403_valid_sync_token() {
    let (base, captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        valid_sync_token_403(),
        multistatus_response(INITIAL_BODY),
    ])
    .await;
    let session = make_client(&base)
        .sync_session("cal/")
        .with_sync_token(Some("garbage-token"));

    let delta = session.incremental().await.unwrap();
    assert!(delta.resynced, "403 valid-sync-token must force a resync");
    assert_eq!(delta.added.len(), 2, "the resync is a full snapshot");
    assert!(
        delta.deleted.is_empty(),
        "RFC 6578 §3.4: a resync must not report deletions"
    );
    assert_eq!(delta.sync_token.as_deref(), Some("token-2"));
    assert_eq!(session.sync_token().as_deref(), Some("token-2"));

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "probe + stale attempt + resync");
    let stale = String::from_utf8_lossy(&reqs[1]);
    assert!(
        stale.contains("<D:sync-token>garbage-token</D:sync-token>"),
        "the persisted token must be sent first: {stale}"
    );
    let retry = String::from_utf8_lossy(&reqs[2]);
    assert!(
        retry.contains("<D:sync-token/>"),
        "the resync must be an initial sync with an empty token: {retry}"
    );
}

#[tokio::test]
async fn sync_session_resyncs_transparently_on_410_gone() {
    let (base, captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        (GONE_410.to_string(), Vec::new()),
        multistatus_response(INITIAL_BODY),
        multistatus_response(EMPTY_DELTA_BODY),
    ])
    .await;
    let session = make_client(&base)
        .sync_session("cal/")
        .with_sync_token(Some("stale-token"));

    let delta = session.incremental().await.unwrap();
    assert!(delta.resynced, "410 Gone must force a resync");
    assert_eq!(delta.added.len(), 2);
    assert!(delta.deleted.is_empty());
    assert_eq!(delta.sync_token.as_deref(), Some("token-2"));

    // Follow-up incremental with the fresh token is a clean delta.
    let next = session.incremental().await.unwrap();
    assert!(!next.resynced);
    assert!(next.added.is_empty() && next.modified.is_empty() && next.deleted.is_empty());

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 4);
    let follow_up = String::from_utf8_lossy(&reqs[3]);
    assert!(
        follow_up.contains("<D:sync-token>token-2</D:sync-token>"),
        "the follow-up must carry the fresh token: {follow_up}"
    );
}

#[tokio::test]
async fn sync_session_continues_past_507_truncation() {
    const TRUNCATED_PAGE: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/c.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-c1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/</D:href>
    <D:status>HTTP/1.1 507 Insufficient Storage</D:status>
  </D:response>
  <D:sync-token>token-partial</D:sync-token>
</D:multistatus>"#;

    const FINAL_PAGE: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/d.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-d1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>token-final</D:sync-token>
</D:multistatus>"#;

    let (base, captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        multistatus_response(INITIAL_BODY),
        multistatus_response(TRUNCATED_PAGE),
        multistatus_response(FINAL_PAGE),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");
    session.initial().await.unwrap();

    let delta = session.incremental().await.unwrap();
    let added: Vec<&str> = delta.added.iter().map(|e| e.href.as_str()).collect();
    assert_eq!(added, vec!["/cal/c.ics", "/cal/d.ics"], "pages merged");
    assert_eq!(delta.sync_token.as_deref(), Some("token-final"));

    let reqs = captured.lock().unwrap();
    let continuation = String::from_utf8_lossy(&reqs[3]);
    assert!(
        continuation.contains("<D:sync-token>token-partial</D:sync-token>"),
        "the truncated page must be continued with its token: {continuation}"
    );
}

#[tokio::test]
async fn sync_session_stops_when_truncation_repeats_the_same_token() {
    const TRUNCATED_PAGE: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/c.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-c1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/</D:href>
    <D:status>HTTP/1.1 507 Insufficient Storage</D:status>
  </D:response>
  <D:sync-token>token-same</D:sync-token>
</D:multistatus>"#;

    let (base, captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        multistatus_response(TRUNCATED_PAGE),
        multistatus_response(TRUNCATED_PAGE),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");

    let delta = session.incremental().await.unwrap();
    assert_eq!(
        delta
            .added
            .iter()
            .map(|e| e.href.as_str())
            .collect::<Vec<_>>(),
        vec!["/cal/c.ics"]
    );
    assert_eq!(
        captured.lock().unwrap().len(),
        3,
        "a repeated page token must stop the continuation loop"
    );
}

#[tokio::test]
async fn sync_session_unknown_capability_still_attempts_sync_collection() {
    // The PROPFIND probe gets a multistatus without supported-report-set and
    // the REPORT probe gets 207 -> Supported after two requests. The initial
    // sync then uses sync-collection (request 3), not the fallback.
    let (base, captured) = serve_sequence(vec![
        multistatus_response(PLAIN_PROPFIND),
        multistatus_response(INITIAL_BODY),
        multistatus_response(INITIAL_BODY),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");

    let snapshot = session.initial().await.unwrap();
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(snapshot.sync_token.as_deref(), Some("token-2"));
    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        3,
        "probe PROPFIND + probe REPORT + initial sync-collection"
    );
    assert!(
        String::from_utf8_lossy(&reqs[2]).contains("<D:sync-collection"),
        "the initial sync must be a sync-collection REPORT after the probe confirmed support"
    );
}

#[tokio::test]
async fn sync_session_clones_share_token_and_probe_state() {
    let (base, _captured) = serve_sequence(vec![
        multistatus_response(SYNC_SUPPORTED_PROPFIND),
        multistatus_response(INITIAL_BODY),
    ])
    .await;
    let session = make_client(&base).sync_session("cal/");
    let clone = session.clone();
    assert_eq!(clone.collection(), "cal/");

    session.initial().await.unwrap();
    assert_eq!(
        clone.sync_token().as_deref(),
        Some("token-2"),
        "clones must share the session state"
    );
}
