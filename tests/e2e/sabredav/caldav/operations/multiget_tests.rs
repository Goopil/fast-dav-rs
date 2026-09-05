use crate::util::{unique_calendar_name, unique_uid};
use bytes::Bytes;
use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_event(uid: &str, summary: &str) -> String {
    format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//EN
BEGIN:VEVENT
UID:{uid}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:{summary}
END:VEVENT
END:VCALENDAR"#
    )
}

/// `calendar_multiget_many` (issue #105) against the live server: ≥3 events
/// created under one calendar, fetched across all hrefs with a batch size of
/// 2 (forcing multiple concurrent REPORTs) and every object retrieved in
/// full, with deterministic ordering and per-href data fidelity (each
/// returned object carries the UID of its requested href).
#[tokio::test]
async fn test_calendar_multiget_many_retrieves_all_events_in_order() {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");

    let calendar_name = unique_calendar_name("e2e_multiget_many");
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

    // Create 3 events with distinct UIDs and summaries.
    let mut uids = Vec::new();
    for i in 1..=3 {
        let uid = unique_uid(&format!("multiget-many-{i}"));
        let event_path = format!("{calendar_path}{uid}.ics");
        let put = client
            .put(
                &event_path,
                Bytes::from(create_test_event(&uid, &format!("Multiget Many Event {i}"))),
            )
            .await
            .expect("PUT request");
        assert!(
            put.status().is_success(),
            "Expected successful event creation {i}, got {}",
            put.status()
        );
        uids.push(uid);
    }

    let hrefs: Vec<String> = uids
        .iter()
        .map(|uid| format!("/{calendar_path}{uid}.ics"))
        .collect();

    // batch_size 2 → 2 chunks, max_concurrency 2 → both in flight at once.
    let results = client
        .calendar_multiget_many(&calendar_path, &hrefs, true, None, 2, 2)
        .await
        .expect("calendar_multiget_many must succeed");

    assert_eq!(
        results.len(),
        hrefs.len(),
        "Expected one result per requested href, got {}",
        results.len()
    );

    for (i, batch_item) in results.iter().enumerate() {
        let obj = batch_item
            .result
            .as_ref()
            .unwrap_or_else(|e| panic!("multiget chunk {i} must succeed, got {e}"));
        assert_eq!(
            batch_item.pub_path, calendar_path,
            "BatchItem must carry the calendar path the REPORT was sent to"
        );
        assert!(
            obj.href.contains(&uids[i]),
            "Result {i} must be the object for UID {}, got href {:?}",
            uids[i],
            obj.href
        );
        assert!(
            obj.etag.is_some(),
            "Expected an ETag for retrieved object {i}"
        );
        let data = obj.calendar_data.as_deref().unwrap_or_else(|| {
            panic!("Expected full calendar data for object {i} (include_data = true)")
        });
        assert!(
            data.contains(&uids[i]) && data.contains("BEGIN:VCALENDAR"),
            "Calendar data for object {i} must round-trip intact, got: {data:?}"
        );
    }

    // Teardown.
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
