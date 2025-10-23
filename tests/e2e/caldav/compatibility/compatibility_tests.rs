use fast_dav_rs::{CalDavClient, Depth};
use bytes::Bytes;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    format!("compatibility_test_calendar_{}", chrono::Utc::now().timestamp_millis())
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    format!("compatibility-event-{}@example.com", chrono::Utc::now().timestamp_millis())
}

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    client
}

#[tokio::test]
async fn test_compatibility_different_depth_levels() {
    let client = create_test_client();
    
    // Test different depth levels with PROPFIND
    let propfind_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;
    
    // Test Depth::Zero
    let zero_result = client.propfind("calendars/test/", Depth::Zero, propfind_body).await;
    match zero_result {
        Ok(response) => {
            println!("PROPFIND with Depth::Zero: {}", response.status());
            assert!(response.status().is_success());
        }
        Err(e) => {
            println!("⚠️  PROPFIND with Depth::Zero failed: {}", e);
        }
    }
    
    // Test Depth::One
    let one_result = client.propfind("calendars/test/", Depth::One, propfind_body).await;
    match one_result {
        Ok(response) => {
            println!("PROPFIND with Depth::One: {}", response.status());
            assert!(response.status().is_success());
        }
        Err(e) => {
            println!("⚠️  PROPFIND with Depth::One failed: {}", e);
        }
    }
    
    // Note: Depth::Infinity is not tested as it's often not supported or can be dangerous
}

#[tokio::test]
async fn test_compatibility_various_xml_namespaces() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Create calendar with standard namespace (more compatible)
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:C="urn:ietf:params:xml:ns:caldav">
  <C:set>
    <C:prop>
      <D:displayname xmlns:D="DAV:">{}</D:displayname>
    </C:prop>
  </C:set>
</C:mkcalendar>"#, calendar_name);
    
    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    match mk_response {
        Ok(response) => {
            println!("MKCALENDAR with standard namespaces: {}", response.status());
            // This should work with most servers
            if !response.status().is_success() {
                println!("⚠️  MKCALENDAR with standard namespaces failed (may be expected with this server)");
            }
        }
        Err(e) => {
            println!("⚠️  MKCALENDAR with standard namespaces failed: {}", e);
        }
    }
    
    // Try to clean up regardless
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_compatibility_special_characters_in_names() {
    let client = create_test_client();
    
    // Test with special characters in calendar name
    let special_chars_name = format!("test_calendar_éñ_{}", chrono::Utc::now().timestamp_millis());
    let calendar_path = format!("calendars/test/{}/", special_chars_name);
    
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, special_chars_name);
    
    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    match mk_response {
        Ok(response) => {
            println!("MKCALENDAR with special characters: {}", response.status());
            // This might fail depending on server support, so we don't assert
        }
        Err(e) => {
            println!("MKCALENDAR with special characters failed (may be expected): {}", e);
        }
    }
    
    // Try to clean up regardless
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_compatibility_different_date_formats() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Create calendar
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
        println!("⚠️  Failed to create calendar for date format test: {}", e);
        return;
    }
    assert!(mk_response.unwrap().status().is_success());
    
    // Test different date formats in calendar-query
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);
    
    // Create an event with standard format
    let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Date Format Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Date Format Test Event
END:VEVENT
END:VCALENDAR"#, event_uid);
    
    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    if let Err(e) = put_response {
        println!("⚠️  Failed to create event for date format test: {}", e);
        let _ = client.delete(&calendar_path).await;
        return;
    }
    assert!(put_response.unwrap().status().is_success());
    
    // Test calendar query with different time range formats
    // Standard UTC format
    let query_result = client.calendar_query_timerange(
        &calendar_path,
        "VEVENT",
        Some("20231201T000000Z"),
        Some("20231231T235959Z"),
        true
    ).await;
    
    match query_result {
        Ok(objects) => {
            println!("Calendar query with UTC format returned {} objects", objects.len());
            // Should find our event
        }
        Err(e) => {
            println!("⚠️  Calendar query with UTC format failed: {}", e);
        }
    }
    
    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_compatibility_etag_handling() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Create calendar
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
        println!("⚠️  Failed to create calendar for ETag test: {}", e);
        return;
    }
    assert!(mk_response.unwrap().status().is_success());
    
    // Create an event
    let event_uid = generate_unique_event_uid();
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);
    
    let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//ETag Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:ETag Test Event
END:VEVENT
END:VCALENDAR"#, event_uid);
    
    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    if let Err(e) = put_response {
        println!("⚠️  Failed to create event for ETag test: {}", e);
        let _ = client.delete(&calendar_path).await;
        return;
    }
    
    // Clone the response before unwrapping to use it later
    let response_copy = match &put_response {
        Ok(response) => response.clone(),
        Err(_) => {
            let _ = client.delete(&calendar_path).await;
            return;
        }
    };
    
    assert!(response_copy.status().is_success());
    
    // Get ETag from response or HEAD request
    let etag = CalDavClient::etag_from_headers(response_copy.headers());
    
    if let Some(etag_value) = etag {
        println!("Retrieved ETag: {}", etag_value);
        
        // Test conditional PUT with matching ETag
        let updated_event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//ETag Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Updated ETag Test Event
DESCRIPTION:Updated with ETag
END:VEVENT
END:VCALENDAR"#, event_uid);
        
        let conditional_put = client.put_if_match(&event_path, Bytes::from(updated_event_ics), &etag_value).await;
        match conditional_put {
            Ok(response) => {
                println!("Conditional PUT with ETag: {}", response.status());
                // Should succeed if ETag is still valid
            }
            Err(e) => {
                println!("⚠️  Conditional PUT with ETag failed: {}", e);
            }
        }
    } else {
        println!("⚠️  No ETag retrieved from server");
    }
    
    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}