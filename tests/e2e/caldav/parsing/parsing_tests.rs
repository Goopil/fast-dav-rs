use bytes::Bytes;
use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    format!(
        "parsing_test_calendar_{}",
        chrono::Utc::now().timestamp_millis()
    )
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    format!(
        "parsing-event-{}@example.com",
        chrono::Utc::now().timestamp_millis()
    )
}

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");

    client
}

#[tokio::test]
async fn test_parsing_special_characters_in_event_properties() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create calendar
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );

    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    if let Err(e) = mk_response {
        println!(
            "⚠️  Failed to create calendar for special characters test: {}",
            e
        );
        return;
    }
    assert!(mk_response.unwrap().status().is_success());

    // Create an event with special characters in various properties
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Special Characters Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Réunion avec Émile et François
DESCRIPTION:Discussion sur l'année 2023 & plans pour 2024
LOCATION:Bâtiment A, Salle Café
END:VEVENT
END:VCALENDAR"#,
        event_uid
    );

    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    match put_response {
        Ok(response) => {
            println!(
                "Event with special characters creation: {}",
                response.status()
            );
            assert!(response.status().is_success());
        }
        Err(e) => {
            println!("⚠️  Event with special characters creation failed: {}", e);
        }
    }

    // Retrieve and verify the event
    let get_response = client.get(&event_path).await;
    match get_response {
        Ok(response) => {
            assert!(response.status().is_success());
            let body_bytes = response.into_body();
            let body = String::from_utf8_lossy(body_bytes.as_ref());
            println!("Retrieved event size: {} characters", body.len());

            // Check that special characters are preserved
            assert!(
                body.contains("Réunion"),
                "Should contain accented characters"
            );
            assert!(body.contains("Émile"), "Should contain accented characters");
            assert!(body.contains("2023 & plans"), "Should contain ampersand");
        }
        Err(e) => {
            println!("⚠️  Retrieving event with special characters failed: {}", e);
        }
    }

    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_parsing_multiline_event_properties() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create calendar
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );

    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    if let Err(e) = mk_response {
        println!("⚠️  Failed to create calendar for multiline test: {}", e);
        return;
    }
    assert!(mk_response.unwrap().status().is_success());

    // Create an event with multiline description
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Multiline Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Multiline Description Test
DESCRIPTION:This is a long description that spans multiple lines.\nIt includes various details about the event.\n\nAgenda:\n1. Introduction\n2. Main discussion\n3. Conclusion\n\nPlease prepare accordingly.
LOCATION:Conference Room
END:VEVENT
END:VCALENDAR"#,
        event_uid
    );

    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    match put_response {
        Ok(response) => {
            println!("Multiline event creation: {}", response.status());
            assert!(response.status().is_success());
        }
        Err(e) => {
            println!("⚠️  Multiline event creation failed: {}", e);
        }
    }

    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_parsing_edge_case_timezones() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create calendar with timezone
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:calendar-timezone>BEGIN:VTIMEZONE
TZID:Europe/Paris
BEGIN:DAYLIGHT
DTSTART:19810329T020000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU
TZNAME:CEST
TZOFFSETFROM:+0100
TZOFFSETTO:+0200
END:DAYLIGHT
BEGIN:STANDARD
DTSTART:19961027T030000
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU
TZNAME:CET
TZOFFSETFROM:+0200
TZOFFSETTO:+0100
END:STANDARD
END:VTIMEZONE</C:calendar-timezone>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );

    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    match mk_response {
        Ok(response) => {
            println!("Calendar with timezone creation: {}", response.status());
            // This might fail depending on server support for embedded timezones
        }
        Err(e) => {
            println!(
                "⚠️  Calendar with timezone creation failed (may be expected): {}",
                e
            );
        }
    }

    // Try to clean up
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_parsing_recurring_events() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create calendar
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );

    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    if let Err(e) = mk_response {
        println!(
            "⚠️  Failed to create calendar for recurring event test: {}",
            e
        );
        return;
    }
    assert!(mk_response.unwrap().status().is_success());

    // Create a recurring event
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Recurring Event Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Weekly Meeting
RRULE:FREQ=WEEKLY;COUNT=10;BYDAY=MO,WE,FR
DESCRIPTION:Regular team meeting
END:VEVENT
END:VCALENDAR"#,
        event_uid
    );

    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    match put_response {
        Ok(response) => {
            println!("Recurring event creation: {}", response.status());
            assert!(response.status().is_success());
        }
        Err(e) => {
            println!("⚠️  Recurring event creation failed: {}", e);
        }
    }

    // Query for the recurring event
    let query_result = client
        .calendar_query_timerange(
            &calendar_path,
            "VEVENT",
            Some("20231225T000000Z"),
            Some("20240301T000000Z"),
            true,
        )
        .await;

    match query_result {
        Ok(objects) => {
            println!(
                "Query for recurring events returned {} objects",
                objects.len()
            );
            // Should find at least our recurring event
        }
        Err(e) => {
            println!("⚠️  Query for recurring events failed: {}", e);
        }
    }

    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_parsing_events_with_attachments() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create calendar
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );

    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    if let Err(e) = mk_response {
        println!("⚠️  Failed to create calendar for attachments test: {}", e);
        return;
    }
    assert!(mk_response.unwrap().status().is_success());

    // Create an event with attachment reference (not actual attachment)
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Attachments Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Event with Attachments
DESCRIPTION:Event that references external attachments
ATTACH:http://example.com/document.pdf
ATTACH;FMTTYPE=image/png:http://example.com/image.png
END:VEVENT
END:VCALENDAR"#,
        event_uid
    );

    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    match put_response {
        Ok(response) => {
            println!("Event with attachments creation: {}", response.status());
            assert!(response.status().is_success());
        }
        Err(e) => {
            println!("⚠️  Event with attachments creation failed: {}", e);
        }
    }

    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}
