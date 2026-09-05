//! Nextcloud `SyncSession`: initial snapshot + empty incremental delta.

use super::util;
use super::util::{NEXTCLOUD_USER, nextcloud_caldav_client};
use bytes::Bytes;

/// `SyncSession` happy path (issue #160) against Nextcloud: the initial
/// snapshot lists the event with its calendar data and a sync token, and the
/// follow-up incremental sync returns an empty delta (the fixture is static
/// after setup).
#[tokio::test]
async fn test_sync_session_initial_and_empty_incremental() {
    let client = nextcloud_caldav_client();
    let calendar_path = format!(
        "calendars/{NEXTCLOUD_USER}/{}/",
        util::unique_calendar_name("nc_session")
    );
    let mkcalendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_path
    );
    let created = client
        .mkcalendar(&calendar_path, &mkcalendar_xml)
        .await
        .expect("MKCALENDAR request");
    assert!(
        created.status().is_success(),
        "MKCALENDAR must succeed, got {}",
        created.status()
    );

    let uid = util::unique_uid("nc-session");
    let event_path = format!("{calendar_path}{uid}.ics");
    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//Nextcloud E2E//EN
BEGIN:VEVENT
UID:{uid}
DTSTAMP:20260101T000000Z
DTSTART:20260701T100000Z
DTEND:20260701T110000Z
SUMMARY:Nextcloud sync session event
END:VEVENT
END:VCALENDAR"#
    );
    let put = client
        .put(&event_path, Bytes::from(event_ics))
        .await
        .expect("event PUT");
    assert!(put.status().is_success(), "event PUT must succeed");

    let session = client.sync_session(&calendar_path);
    let snapshot = session.initial().await.expect("initial sync session");
    assert!(
        snapshot.items.iter().any(|entry| entry.href.contains(&uid)),
        "initial snapshot must list the event ({} items)",
        snapshot.items.len()
    );
    assert!(
        snapshot.sync_token.is_some(),
        "Nextcloud must issue a sync token for the session"
    );
    assert!(
        snapshot
            .items
            .iter()
            .all(|entry| entry.data.is_some() && entry.etag.is_some()),
        "the CalDAV session must fetch calendar data alongside the etags ({} items)",
        snapshot.items.len()
    );

    // The fixture is static after setup: the incremental sync is empty.
    let delta = session.incremental().await.expect("incremental sync");
    assert!(!delta.resynced);
    assert!(
        delta.added.is_empty() && delta.modified.is_empty() && delta.deleted.is_empty(),
        "no changes since the initial sync ({} added, {} modified, {} deleted)",
        delta.added.len(),
        delta.modified.len(),
        delta.deleted.len()
    );
    assert!(
        delta.sync_token.is_some(),
        "the incremental sync must hand out a token to persist"
    );

    // Cleanup.
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}
