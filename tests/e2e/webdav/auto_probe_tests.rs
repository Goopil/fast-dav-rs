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
SUMMARY:Auto-probe smoke event
END:VEVENT
END:VCALENDAR"#
    )
}

/// AUDIT-012 fix smoke (e2e): a default client (`RequestCompressionMode::Auto`)
/// must complete a real `PUT` against the live server. The hidden compression
/// probe may fail (SabreDAV does not decode request bodies, so the gzip'd
/// probe is answered with 4xx) — in that case the request proceeds
/// uncompressed and nothing may poison the write path. Asserts the object
/// round-trips intact via `GET`.
#[tokio::test]
async fn test_auto_request_compression_put_round_trip() {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client (default Auto request compression)");

    let calendar_name = unique_calendar_name("e2e_autoprobe_calendar");
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

    let uid = unique_uid("autoprobe-event");
    let event_path = format!("{calendar_path}{uid}.ics");

    // The PUT in default Auto mode exercises the probe → (maybe) fallback →
    // write pipeline end-to-end against the real server.
    let put = client
        .put(&event_path, Bytes::from(create_test_event(&uid)))
        .await
        .expect("PUT must succeed in Auto mode even if the probe fails");
    assert!(
        put.status().is_success(),
        "Expected successful PUT in Auto request-compression mode, got {}",
        put.status()
    );

    // Round-trip: the stored object must be the original iCalendar, not the
    // compressed wire form (a server that accepted a compressed request
    // without decoding it would surface here).
    let get = client
        .get(&event_path)
        .await
        .expect("GET request for the round-trip");
    assert!(
        get.status().is_success(),
        "Expected successful GET, got {}",
        get.status()
    );
    let body = get.into_body();
    let body = std::str::from_utf8(&body).expect("Stored object must be UTF-8 iCalendar");
    assert!(
        body.contains("BEGIN:VCALENDAR") && body.contains(&uid),
        "Round-tripped object must contain the original iCalendar data, got: {body:?}"
    );

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
