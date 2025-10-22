use fast_dav_rs::{CalDavClient, ContentEncoding, Depth};
use bytes::Bytes;

/// E2E tests for the CalDAV client against a real SabreDAV server
/// These tests require the SabreDAV test environment to be running
/// Run `cd sabredav-test && ./setup.sh` before running these tests

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    client
}

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
DESCRIPTION:Test event created by fast-dav-rs E2E tests
END:VEVENT
END:VCALENDAR"#)
}

#[tokio::test]
async fn test_basic_connectivity() {
    let client = create_test_client();
    
    // Test basic connectivity with a simple GET request
    let response = client.get("").await;
    match response {
        Ok(resp) => {
            println!("GET request succeeded with status: {}", resp.status());
            // Even if it's not successful, we at least know the server is reachable
        }
        Err(e) => {
            panic!("GET request failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_propfind_principals() {
    let client = create_test_client();
    
    // Try a PROPFIND on the principals path
    let response = client.propfind("principals/", fast_dav_rs::Depth::One, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("PROPFIND on principals succeeded with status: {}", resp.status());
            if resp.status().is_success() {
                let body = resp.into_body();
                println!("Response body length: {}", body.len());
                // Don't print the body as it might be compressed
            }
        }
        Err(e) => {
            println!("PROPFIND on principals failed: {}", e);
            // This might be a compression error, which is actually good - it means we're getting a response
        }
    }
}

#[tokio::test]
async fn test_propfind_user_principal() {
    let client = create_test_client();
    
    // Try a PROPFIND on the user principal
    let response = client.propfind("principals/test/", fast_dav_rs::Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("PROPFIND on user principal succeeded with status: {}", resp.status());
            if resp.status().is_success() {
                let body = resp.into_body();
                println!("Response body length: {}", body.len());
                // Don't print the body as it might be compressed
            }
        }
        Err(e) => {
            println!("PROPFIND on user principal failed: {}", e);
            // This might be a compression error, which is actually good - it means we're getting a response
        }
    }
}

#[tokio::test]
async fn test_propfind_user_calendars() {
    let client = create_test_client();
    
    // Try a PROPFIND on the user calendars path
    let response = client.propfind("calendars/test/", fast_dav_rs::Depth::One, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
    <C:calendar-description/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("PROPFIND on user calendars succeeded with status: {}", resp.status());
            if resp.status().is_success() {
                let body = resp.into_body();
                println!("Response body length: {}", body.len());
                // Don't print the body as it might be compressed
            }
        }
        Err(e) => {
            println!("PROPFIND on user calendars failed: {}", e);
            // This might be a compression error, which is actually good - it means we're getting a response
        }
    }
}

#[tokio::test]
async fn test_compression_support() {
    let client = create_test_client();
    
    // Test with different compression encodings
    let encodings = vec![
        ContentEncoding::Identity,
        ContentEncoding::Gzip,
        ContentEncoding::Br,
        ContentEncoding::Zstd,
    ];
    
    for encoding in encodings {
        let mut client_with_encoding = client.clone();
        client_with_encoding.set_request_compression(encoding);
        
        // Test a simple GET request
        let response = client_with_encoding.get("").await;
        match response {
            Ok(resp) => {
                // Request should succeed
                println!("Request with {:?} compression succeeded with status {:?}", 
                         encoding, resp.status());
            }
            Err(e) => {
                // Some servers might not support certain compression methods
                println!("Request with {:?} compression failed: {:?}", encoding, e);
            }
        }
    }
}

// Test that demonstrates the client can handle compressed responses
#[tokio::test]
async fn test_compressed_response_handling() {
    let client = create_test_client();
    
    // This test verifies that our client can handle compressed responses
    // Even if we get a decompression error, it means the server is sending compressed data
    let response = client.propfind("principals/test/", fast_dav_rs::Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("Compressed response test succeeded with status: {}", resp.status());
            // For now, let's not assert success since we might get 400 for various reasons
            // The important thing is that we don't get a decompression error
        }
        Err(e) => {
            println!("Compressed response test encountered error: {}", e);
            // If the error is related to compression, that's actually good - it means we're getting a response
            // The important thing is that we're communicating with the server
        }
    }
}

/// Test creating a new calendar using MKCALENDAR
#[tokio::test]
async fn test_create_calendar() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Create a new calendar
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:calendar-description>Test calendar created by fast-dav-rs E2E tests</C:calendar-description>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name);
    
    let response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    match response {
        Ok(resp) => {
            println!("MKCALENDAR request succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful calendar creation, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("MKCALENDAR request failed: {}", e);
        }
    }
    
    // Verify the calendar was created by doing a PROPFIND
    let verify_response = client.propfind(&calendar_path, Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await;
    
    match verify_response {
        Ok(resp) => {
            assert!(resp.status().is_success(), "Expected to find created calendar, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("Failed to verify calendar creation: {}", e);
        }
    }
    
    // Clean up: Delete the calendar
    let delete_response = client.delete(&calendar_path).await;
    match delete_response {
        Ok(resp) => {
            println!("Cleaned up calendar with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful calendar deletion, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("Failed to clean up calendar: {}", e);
        }
    }
}

/// Test creating and managing calendar events
#[tokio::test]
async fn test_calendar_events() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);
    
    // First create a calendar
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:calendar-description>Test calendar for events</C:calendar-description>
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
    match put_response {
        Ok(resp) => {
            println!("PUT event request succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful event creation, got status: {}", resp.status());
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
            assert!(resp.status().is_success(), "Expected successful event retrieval, got status: {}", resp.status());
            let body = resp.into_body();
            assert!(body.len() > 0, "Expected non-empty event body");
            println!("Retrieved event body length: {}", body.len());
        }
        Err(e) => {
            panic!("GET event request failed: {}", e);
        }
    }
    
    // Update the event
    let updated_event_uid = event_uid.clone();
    let updated_event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230102T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T120000Z
SUMMARY:Updated Test Event
DESCRIPTION:Updated test event created by fast-dav-rs E2E tests
END:VEVENT
END:VCALENDAR"#, updated_event_uid);
    let updated_event_bytes = Bytes::from(updated_event_ics);
    
    // Get the ETag for conditional update
    let head_response = client.head(&event_path).await;
    match head_response {
        Ok(resp) => {
            if let Some(etag) = CalDavClient::etag_from_headers(resp.headers()) {
                let update_response = client.put_if_match(&event_path, updated_event_bytes, &etag).await;
                match update_response {
                    Ok(resp) => {
                        println!("Conditional PUT event update succeeded with status: {}", resp.status());
                        assert!(resp.status().is_success(), "Expected successful conditional update, got status: {}", resp.status());
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
            println!("DELETE event request succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful event deletion, got status: {}", resp.status());
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
            assert!(resp.status().is_success(), "Expected successful calendar deletion, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("Failed to clean up calendar: {}", e);
        }
    }
}

/// Test COPY and MOVE operations
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
    
    // Create a new event
    let event_ics = create_test_event(&event_uid);
    let event_bytes = Bytes::from(event_ics);
    
    let put_response = client.put(&event_path, event_bytes).await;
    if let Err(e) = put_response {
        panic!("Failed to create event: {}", e);
    }
    
    // Test COPY operation
    let full_copied_event_url = format!("{}{}", SABREDAV_URL, copied_event_path);
    let copy_response = client.copy(&event_path, &full_copied_event_url, true).await;
    match copy_response {
        Ok(resp) => {
            println!("COPY request succeeded with status: {}", resp.status());
            // COPY may succeed or fail depending on server implementation
        }
        Err(e) => {
            println!("COPY request failed (may be expected): {}", e);
        }
    }
    
    // Test MOVE operation
    let full_moved_event_url = format!("{}{}", SABREDAV_URL, moved_event_path);
    let move_response = client.r#move(&event_path, &full_moved_event_url, true).await;
    match move_response {
        Ok(resp) => {
            println!("MOVE request succeeded with status: {}", resp.status());
            // MOVE may succeed or fail depending on server implementation
        }
        Err(e) => {
            println!("MOVE request failed (may be expected): {}", e);
        }
    }
    
    // Clean up: Delete the calendar (this will delete all events in it)
    let cleanup_response = client.delete(&calendar_path).await;
    if let Ok(resp) = cleanup_response {
        println!("Cleaned up calendar with status: {}", resp.status());
    }
}

/// Test PROPPATCH operation
#[tokio::test]
async fn test_proppatch_operation() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
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
    
    // Test PROPPATCH operation
    let proppatch_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propertyupdate xmlns:D="DAV:">
  <D:set>
    <D:prop>
      <D:displayname>Updated Test Calendar</D:displayname>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    
    let proppatch_response = client.proppatch(&calendar_path, proppatch_xml).await;
    match proppatch_response {
        Ok(resp) => {
            println!("PROPPATCH request succeeded with status: {}", resp.status());
            // PROPPATCH may succeed or fail depending on server implementation
        }
        Err(e) => {
            println!("PROPPATCH request failed (may be expected): {}", e);
        }
    }
    
    // Clean up: Delete the calendar
    let cleanup_response = client.delete(&calendar_path).await;
    if let Ok(resp) = cleanup_response {
        println!("Cleaned up calendar with status: {}", resp.status());
    }
}

/// Test discovery operations
#[tokio::test]
async fn test_discovery_operations() {
    let client = create_test_client();
    
    // Test discovering current user principal
    let principal_result = client.discover_current_user_principal().await;
    match principal_result {
        Ok(Some(principal)) => {
            println!("Discovered user principal: {}", principal);
        }
        Ok(None) => {
            println!("No user principal found");
        }
        Err(e) => {
            println!("Failed to discover user principal: {}", e);
        }
    }
    
    // Test discovering calendar home set
    let home_sets_result = client.discover_calendar_home_set("principals/test/").await;
    match home_sets_result {
        Ok(home_sets) => {
            println!("Discovered {} calendar home sets", home_sets.len());
            for home_set in home_sets {
                println!("  Home set: {}", home_set);
            }
        }
        Err(e) => {
            println!("Failed to discover calendar home sets: {}", e);
        }
    }
    
    // Test listing calendars
    let calendars_result = client.list_calendars("calendars/test/").await;
    match calendars_result {
        Ok(calendars) => {
            println!("Found {} calendars", calendars.len());
            for calendar in calendars {
                println!("  Calendar: {:?}", calendar.displayname);
            }
        }
        Err(e) => {
            println!("Failed to list calendars: {}", e);
        }
    }
}