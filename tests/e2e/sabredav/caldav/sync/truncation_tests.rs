use crate::util::{unique_calendar_name, unique_uid};
use bytes::Bytes;
use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_event(uid: &str) -> String {
    format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//EN
BEGIN:VEVENT
UID:{uid}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Truncation regression event
END:VEVENT
END:VCALENDAR"#
    )
}

/// AUDIT-015 regression (e2e): against the real server, `sync_collection`
/// must report `truncated == false` — the first-class truncation signal must
/// not fire on a healthy, untruncated sync (a false positive would make
/// callers loop on pagination forever).
#[tokio::test]
async fn test_sync_collection_not_truncated() {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");

    let calendar_name = unique_calendar_name("e2e_sync_truncation");
    let calendar_path = format!("calendars/test/{calendar_name}/");
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{calendar_name}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#
    );
    let mk = client
        .mkcalendar(&calendar_path, &calendar_xml)
        .await
        .expect("MKCALENDAR request");
    assert!(
        mk.status().is_success(),
        "Expected successful calendar creation, got {}",
        mk.status()
    );

    let mut event_paths = Vec::new();
    for i in 1..=2 {
        let uid = unique_uid(&format!("truncation-{i}"));
        let event_path = format!("{calendar_path}{uid}.ics");
        let put = client
            .put(&event_path, Bytes::from(create_test_event(&uid)))
            .await
            .expect("PUT request");
        assert!(
            put.status().is_success(),
            "Expected successful event creation {i}, got {}",
            put.status()
        );
        event_paths.push(event_path);
    }

    let delta = client
        .sync_collection(&calendar_path, None, Some(100), true, None)
        .await
        .expect("sync_collection must succeed against the live server");
    assert!(
        !delta.truncated,
        "A healthy sync against the fixture must not be reported as truncated"
    );
    assert!(
        delta.items.len() >= event_paths.len(),
        "Expected at least {} items in the delta, got {}",
        event_paths.len(),
        delta.items.len()
    );
    assert!(
        delta.sync_token.is_some(),
        "Sabre/DAV must hand out a sync token"
    );

    // Teardown.
    for event_path in event_paths {
        let _ = client.delete(&event_path).await;
    }
    let del = client
        .delete(&calendar_path)
        .await
        .expect("Teardown DELETE request");
    assert!(
        del.status().is_success(),
        "Expected successful calendar deletion, got {}",
        del.status()
    );
}
