//! Radicale sync behaviors: unknown-token REPORT (observed) and the
//! `SyncSession` stale-token transparent resync.

use super::util;
use super::util::{RADICALE_USER, radicale_caldav_client, radicale_webdav_client};
use bytes::Bytes;
use fast_dav_rs::Depth;

fn principal_path() -> String {
    format!("{RADICALE_USER}/")
}

/// Records Radicale's observed behavior for a `sync-collection` REPORT with
/// an unknown token. Observed on Radicale 3.7.6: `403 Forbidden` with
/// `<D:error><valid-sync-token/></D:error>` (RFC 6578 §3.2 stale-token
/// signal). The assertion is deliberately loose (any non-success status);
/// the raw status and body are printed for the record.
#[tokio::test]
async fn test_sync_collection_unknown_token_records_observed_behavior() {
    let client = radicale_caldav_client();
    let raw = radicale_webdav_client();

    let calendar_path = format!(
        "{}{}/",
        principal_path(),
        util::unique_calendar_name("radicale_sync")
    );
    let mkcalendar_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"/>"#;
    let created = client
        .mkcalendar(&calendar_path, mkcalendar_xml)
        .await
        .expect("MKCALENDAR");
    assert!(created.status().is_success(), "MKCALENDAR must succeed");

    let uid = util::unique_uid("radicale-sync");
    let event_path = format!("{calendar_path}{uid}.ics");
    let put = client
        .put(&event_path, Bytes::from(format!("BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//fast-dav-rs//Radicale E2E//EN\nBEGIN:VEVENT\nUID:{uid}\nDTSTAMP:20260101T000000Z\nDTSTART:20260601T100000Z\nEND:VEVENT\nEND:VCALENDAR")))
        .await
        .expect("event PUT");
    assert!(put.status().is_success(), "PUT must succeed");

    // The initial sync hands out a real token; keep it out of the request to
    // prove the *server* rejects the fabricated one.
    let initial = client
        .sync_collection(&calendar_path, None, Some(100), true, None)
        .await
        .expect("initial sync_collection must succeed");
    assert!(
        initial.sync_token.is_some(),
        "Radicale must issue a sync token"
    );
    assert_eq!(initial.items.len(), 1, "initial sync must list the event");

    let bogus_report = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:sync-collection xmlns:D="DAV:">
  <D:sync-token>http://radicale.example/ns/sync/DOES-NOT-EXIST</D:sync-token>
  <D:prop><D:getetag/></D:prop>
</D:sync-collection>"#;
    let resp = raw
        .report(&calendar_path, Depth::Zero, bogus_report)
        .await
        .expect("REPORT with unknown token must complete (no transport error)");
    println!(
        "Radicale sync-collection REPORT with unknown token -> status {} body: {}",
        resp.status(),
        String::from_utf8_lossy(resp.body())
    );
    assert!(
        !resp.status().is_success(),
        "Radicale must reject an unknown sync token (observed 403 + valid-sync-token), got {}",
        resp.status()
    );

    // Client-level contract: `sync_collection_resilient` treats the observed
    // stale-token signal (403 + valid-sync-token, or 410) as "resync" and
    // transparently re-issues an initial sync (the DAVx⁵ rule).
    let (_, items, token, resynced) = raw
        .sync_collection_resilient(
            &calendar_path,
            Some("http://radicale.example/ns/sync/DOES-NOT-EXIST"),
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "getetag",
        )
        .await
        .expect("resilient sync must transparently fall back to an initial sync");
    assert!(resynced, "unknown token must surface as resynced=true");
    assert!(!items.is_empty(), "the resync must return the live items");
    assert!(token.is_some(), "the resync must hand out a fresh token");
    println!(
        "resilient resync returned {} item(s) with token {token:?}",
        items.len()
    );

    // The server must still be healthy after the rejected REPORT.
    let alive = client
        .propfind(
            &calendar_path,
            Depth::Zero,
            r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#,
        )
        .await
        .expect("PROPFIND after rejected REPORT");
    assert!(
        alive.status().is_success(),
        "server must stay healthy, got {}",
        alive.status()
    );

    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}

/// Documents the `SyncSession` contract (issue #160) against Radicale:
/// a restored session whose persisted (garbage) token the server rejects
/// with `403` + `valid-sync-token` must transparently resync (DAVx⁵ rule)
/// and flag `resynced = true`; the follow-up incremental sync with the
/// fresh token is a clean delta.
#[tokio::test]
async fn test_sync_session_invalid_token_transparent_resync() {
    let client = radicale_caldav_client();
    let calendar_path = format!(
        "{}{}/",
        principal_path(),
        util::unique_calendar_name("radicale_session")
    );
    let created = client
        .mkcalendar(
            &calendar_path,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"/>"#,
        )
        .await
        .expect("MKCALENDAR");
    assert!(created.status().is_success(), "MKCALENDAR must succeed");

    let uid = util::unique_uid("radicale-session");
    let event_path = format!("{calendar_path}{uid}.ics");
    let put = client
        .put(&event_path, Bytes::from(format!("BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//fast-dav-rs//Radicale E2E//EN\nBEGIN:VEVENT\nUID:{uid}\nDTSTAMP:20260101T000000Z\nDTSTART:20260601T100000Z\nEND:VEVENT\nEND:VCALENDAR")))
        .await
        .expect("event PUT");
    assert!(put.status().is_success(), "PUT must succeed");

    let session = client.sync_session(&calendar_path);
    let snapshot = session.initial().await.expect("initial sync session");
    assert_eq!(snapshot.items.len(), 1, "initial sync must list the event");
    assert!(
        snapshot.sync_token.is_some(),
        "Radicale must issue a sync token for the session"
    );

    // A new session restored from a garbage (persisted) token: Radicale
    // answers 403 + valid-sync-token; the session must reset transparently.
    let restored = client
        .sync_session(&calendar_path)
        .with_sync_token(Some("http://radicale.example/ns/sync/DOES-NOT-EXIST"));
    let delta = restored
        .incremental()
        .await
        .expect("a stale token must trigger a transparent resync, not an error");
    assert!(
        delta.resynced,
        "Radicale's 403 valid-sync-token must surface as resynced=true"
    );
    assert!(
        delta.added.iter().any(|entry| entry
            .href
            // Radicale percent-encodes `@` in stored hrefs (`%40`).
            .contains(uid.split('@').next().expect("uid has a local part"))),
        "the resync must list the live event ({} added, {} modified)",
        delta.added.len(),
        delta.modified.len()
    );
    assert!(
        delta.deleted.is_empty(),
        "RFC 6578 §3.4: a resync must not report deletions"
    );
    assert!(
        delta.sync_token.is_some(),
        "the resync must hand out a fresh token to persist"
    );

    // The follow-up incremental with the fresh token is a clean delta.
    let next = restored.incremental().await.expect("follow-up incremental");
    assert!(!next.resynced);
    assert!(
        next.added.is_empty() && next.modified.is_empty() && next.deleted.is_empty(),
        "no changes since the resync: {next:?}"
    );

    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}
