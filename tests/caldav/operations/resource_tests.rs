use fast_dav_rs::CalDavClient;
use bytes::Bytes;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    format!("test_calendar_{}", chrono::Utc::now().timestamp_millis())
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    format!("event-{}@example.com", chrono::Utc::now().timestamp_millis())
}

/// Helper function to create a test event
fn create_test_event(uid: &str) -> String {
    format!(r#"BEGIN:VCALENDAR
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
END:VCALENDAR"#)
}

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    client
}

#[tokio::test]
async fn test_copy_move_operations() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);
    let copied_event_path = format!("{}copy-{}", calendar_path, event_filename);
    let moved_event_path = format!("{}moved-{}", calendar_path, event_filename);
    
    // First create a calendar
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name);
    
    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    if let Err(e) = mk_response {
        panic!("Failed to create calendar: {}", e);
    }
    assert!(mk_response.unwrap().status().is_success(), "Expected successful calendar creation");
    
    // Create a new event
    let event_ics = create_test_event(&event_uid);
    let event_bytes = Bytes::from(event_ics);
    
    let put_response = client.put(&event_path, event_bytes).await;
    if let Err(e) = put_response {
        panic!("Failed to create event: {}", e);
    }
    assert!(put_response.unwrap().status().is_success(), "Expected successful event creation");
    
    // Test COPY operation
    let full_copied_event_url = format!("{}{}", SABREDAV_URL, copied_event_path);
    let copy_response = client.copy(&event_path, &full_copied_event_url, true).await;
    match copy_response {
        Ok(resp) => {
            println!("COPY request succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful COPY operation, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("COPY request failed: {}", e);
        }
    }
    
    // Test MOVE operation
    let full_moved_event_url = format!("{}{}", SABREDAV_URL, moved_event_path);
    let move_response = client.r#move(&event_path, &full_moved_event_url, true).await;
    match move_response {
        Ok(resp) => {
            println!("MOVE request succeeded with status: {}", resp.status());
            // MOVE may be forbidden by some servers
            assert!(resp.status().is_success() || resp.status() == hyper::StatusCode::FORBIDDEN, 
                    "Expected successful MOVE operation or FORBIDDEN, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("MOVE request failed: {}", e);
        }
    }
    
    // Clean up: Delete the calendar (this will delete all events in it)
    let cleanup_response = client.delete(&calendar_path).await;
    match cleanup_response {
        Ok(resp) => {
            println!("Cleaned up calendar with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful calendar deletion, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("Failed to clean up calendar: {}", e);
        }
    }
}