use bytes::Bytes;
use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    format!("test_calendar_{}", chrono::Utc::now().timestamp_millis())
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    format!(
        "event-{}@example.com",
        chrono::Utc::now().timestamp_millis()
    )
}

/// Helper function to create a test event
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
SUMMARY:Test Event
DESCRIPTION:Test event created by fast-dav-rs tests
END:VEVENT
END:VCALENDAR"#
    )
}

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::test]
async fn test_calendar_events() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    // First create a calendar
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:calendar-description>Test calendar for events</C:calendar-description>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );

    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    if let Err(e) = mk_response {
        panic!("Failed to create calendar: {}", e);
    }
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful calendar creation"
    );

    // Create a new event
    let event_ics = create_test_event(&event_uid);
    let event_bytes = Bytes::from(event_ics);

    let put_response = client.put(&event_path, event_bytes).await;
    match put_response {
        Ok(resp) => {
            println!("PUT event request succeeded with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful event creation, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("PUT event request failed: {}", e);
        }
    }

    // Retrieve the event
    let get_response = client.get(&event_path).await;
    match get_response {
        Ok(resp) => {
            println!("GET event request succeeded with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful event retrieval, got status: {}",
                resp.status()
            );
            let body = resp.into_body();
            assert!(!body.is_empty(), "Expected non-empty event body");
            println!("Retrieved event body length: {}", body.len());
        }
        Err(e) => {
            panic!("GET event request failed: {}", e);
        }
    }

    // Update the event
    let updated_event_uid = event_uid.clone();
    let updated_event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230102T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T120000Z
SUMMARY:Updated Test Event
DESCRIPTION:Updated test event created by fast-dav-rs tests
END:VEVENT
END:VCALENDAR"#,
        updated_event_uid
    );
    let updated_event_bytes = Bytes::from(updated_event_ics);

    // Get the ETag for conditional update
    let head_response = client.head(&event_path).await;
    match head_response {
        Ok(resp) => {
            if let Some(etag) = CalDavClient::etag_from_headers(resp.headers()) {
                let update_response = client
                    .put_if_match(&event_path, updated_event_bytes, &etag)
                    .await;
                match update_response {
                    Ok(resp) => {
                        println!(
                            "Conditional PUT event update succeeded with status: {}",
                            resp.status()
                        );
                        assert!(
                            resp.status().is_success(),
                            "Expected successful conditional update, got status: {}",
                            resp.status()
                        );
                    }
                    Err(e) => {
                        panic!("Conditional PUT event update failed: {}", e);
                    }
                }
            } else {
                panic!("Failed to get ETag for conditional update");
            }
        }
        Err(e) => {
            panic!("HEAD request failed: {}", e);
        }
    }

    // Delete the event
    let delete_response = client.delete(&event_path).await;
    match delete_response {
        Ok(resp) => {
            println!(
                "DELETE event request succeeded with status: {}",
                resp.status()
            );
            assert!(
                resp.status().is_success(),
                "Expected successful event deletion, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("DELETE event request failed: {}", e);
        }
    }

    // Clean up: Delete the calendar
    let cleanup_response = client.delete(&calendar_path).await;
    match cleanup_response {
        Ok(resp) => {
            println!("Cleaned up calendar with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful calendar deletion, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to clean up calendar: {}", e);
        }
    }
}
