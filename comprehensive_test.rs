use fast_dav_rs::{CalDavClient, ContentEncoding, Depth};
use bytes::Bytes;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";
const INVALID_USER: &str = "invalid";
const INVALID_PASS: &str = "invalid";

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

fn create_invalid_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(INVALID_USER), Some(INVALID_PASS))
        .expect("Failed to create invalid CalDAV client")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Comprehensive CalDAV Client Testing ===\n");
    
    // Test 1: Basic connectivity and authentication
    test_basic_connectivity().await?;
    
    // Test 2: Valid authentication
    test_valid_authentication().await?;
    
    // Test 3: Invalid authentication
    test_invalid_authentication().await?;
    
    // Test 4: Principal discovery
    test_principal_discovery().await?;
    
    // Test 5: Calendar operations
    test_calendar_operations().await?;
    
    // Test 6: Event operations
    test_event_operations().await?;
    
    // Test 7: Compression support
    test_compression_support().await?;
    
    // Test 8: Error handling
    test_error_handling().await?;
    
    println!("=== All tests completed successfully! ===");
    Ok(())
}

async fn test_basic_connectivity() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Testing basic connectivity...");
    
    let client = create_test_client();
    let response = client.get("").await?;
    
    println!("   ✓ GET request: {}", response.status());
    assert!(response.status().is_success());
    
    Ok(())
}

async fn test_valid_authentication() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Testing valid authentication...");
    
    let client = create_test_client();
    
    // Test PROPFIND with valid credentials
    let response = client.propfind("principals/test/", Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await?;
    
    println!("   ✓ Authenticated PROPFIND: {}", response.status());
    assert!(response.status().is_success());
    
    Ok(())
}

async fn test_invalid_authentication() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Testing invalid authentication...");
    
    let client = create_invalid_client();
    
    // Test PROPFIND with invalid credentials
    match client.propfind("principals/test/", Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await {
        Ok(response) => {
            println!("   ✓ Invalid auth PROPFIND: {}", response.status());
            // Should be 401 Unauthorized
            assert_eq!(response.status(), hyper::StatusCode::UNAUTHORIZED);
        }
        Err(e) => {
            println!("   ✓ Invalid auth PROPFIND failed as expected: {}", e);
        }
    }
    
    Ok(())
}

async fn test_principal_discovery() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. Testing principal discovery...");
    
    let client = create_test_client();
    
    // Discover current user principal
    let principal = client.discover_current_user_principal().await?;
    println!("   ✓ Discovered principal: {:?}", principal);
    assert!(principal.is_some());
    
    // Discover calendar home set
    if let Some(principal_url) = principal {
        let home_sets = client.discover_calendar_home_set(&principal_url).await?;
        println!("   ✓ Found {} calendar home sets", home_sets.len());
        
        // List calendars
        if !home_sets.is_empty() {
            let calendars = client.list_calendars(&home_sets[0]).await?;
            println!("   ✓ Found {} calendars", calendars.len());
        }
    }
    
    Ok(())
}

async fn test_calendar_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("5. Testing calendar operations...");
    
    let client = create_test_client();
    let calendar_name = format!("test_calendar_{}", chrono::Utc::now().timestamp_millis());
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Create calendar
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:calendar-description>Test calendar for comprehensive testing</C:calendar-description>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name);
    
    let response = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    println!("   ✓ MKCALENDAR: {}", response.status());
    
    if response.status().is_success() {
        // Verify calendar exists
        let verify_response = client.propfind(&calendar_path, Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await?;
        
        println!("   ✓ Calendar verification: {}", verify_response.status());
        assert!(verify_response.status().is_success());
        
        // Update calendar properties
        let proppatch_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propertyupdate xmlns:D="DAV:">
  <D:set>
    <D:prop>
      <D:displayname>Updated Test Calendar</D:displayname>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
        
        let proppatch_response = client.proppatch(&calendar_path, proppatch_xml).await?;
        println!("   ✓ PROPPATCH: {}", proppatch_response.status());
        
        // Clean up
        let delete_response = client.delete(&calendar_path).await?;
        println!("   ✓ Calendar cleanup: {}", delete_response.status());
    }
    
    Ok(())
}

async fn test_event_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("6. Testing event operations...");
    
    let client = create_test_client();
    let calendar_name = format!("test_events_{}", chrono::Utc::now().timestamp_millis());
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    let event_uid = format!("event-{}@example.com", chrono::Utc::now().timestamp_millis());
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);
    
    // Create calendar first
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name);
    
    let mk_response = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    println!("   ✓ Event calendar creation: {}", mk_response.status());
    
    if mk_response.status().is_success() {
        // Create event
        let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Comprehensive Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Comprehensive Test Event
DESCRIPTION:Event created during comprehensive testing
END:VEVENT
END:VCALENDAR"#, event_uid);
        
        let event_bytes = Bytes::from(event_ics);
        let put_response = client.put(&event_path, event_bytes).await?;
        println!("   ✓ Event creation: {}", put_response.status());
        
        if put_response.status().is_success() {
            // Retrieve event
            let get_response = client.get(&event_path).await?;
            println!("   ✓ Event retrieval: {}", get_response.status());
            assert!(get_response.status().is_success());
            
            let body = get_response.into_body();
            assert!(body.len() > 0);
            println!("   ✓ Event body length: {}", body.len());
            
            // Get ETag and test conditional operations
            let head_response = client.head(&event_path).await?;
            if let Some(etag) = CalDavClient::etag_from_headers(head_response.headers()) {
                println!("   ✓ Retrieved ETag: {}", etag);
                
                // Test conditional update
                let updated_event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Comprehensive Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230102T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T120000Z
SUMMARY:Updated Comprehensive Test Event
DESCRIPTION:Updated event created during comprehensive testing
END:VEVENT
END:VCALENDAR"#, event_uid);
                
                let updated_bytes = Bytes::from(updated_event_ics);
                let update_response = client.put_if_match(&event_path, updated_bytes, &etag).await?;
                println!("   ✓ Conditional update: {}", update_response.status());
                
                // Test COPY operation
                let copied_event_path = format!("{}copy-{}", calendar_path, event_filename);
                let full_copied_url = format!("{}{}", SABREDAV_URL, copied_event_path);
                let copy_response = client.copy(&event_path, &full_copied_url, true).await?;
                println!("   ✓ COPY operation: {}", copy_response.status());
                
                // Test MOVE operation
                let moved_event_path = format!("{}moved-{}", calendar_path, event_filename);
                let full_moved_url = format!("{}{}", SABREDAV_URL, moved_event_path);
                let move_response = client.r#move(&event_path, &full_moved_url, true).await?;
                println!("   ✓ MOVE operation: {}", move_response.status());
            }
            
            // Clean up events (try to delete both original and moved)
            let _ = client.delete(&event_path).await;
            let _ = client.delete(&format!("{}moved-{}", calendar_path, event_filename)).await;
            let _ = client.delete(&format!("{}copy-{}", calendar_path, event_filename)).await;
        }
        
        // Clean up calendar
        let _ = client.delete(&calendar_path).await;
    }
    
    Ok(())
}

async fn test_compression_support() -> Result<(), Box<dyn std::error::Error>> {
    println!("7. Testing compression support...");
    
    let encodings = vec![
        ContentEncoding::Identity,
        ContentEncoding::Gzip,
        ContentEncoding::Br,
        ContentEncoding::Zstd,
    ];
    
    for encoding in encodings {
        let mut client = create_test_client();
        client.set_request_compression(encoding);
        
        let response = client.get("").await?;
        println!("   ✓ {:?} compression: {}", encoding, response.status());
        assert!(response.status().is_success());
    }
    
    Ok(())
}

async fn test_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("8. Testing error handling...");
    
    let client = create_test_client();
    
    // Test PROPFIND on non-existent resource
    let response = client.propfind("nonexistent/", Depth::Zero,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await?;
    
    println!("   ✓ Non-existent resource PROPFIND: {}", response.status());
    // Should be 404 Not Found
    assert_eq!(response.status(), hyper::StatusCode::NOT_FOUND);
    
    // Test invalid XML
    let invalid_response = client.propfind("principals/test/", Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<invalid xml>"#).await?;
    
    println!("   ✓ Invalid XML PROPFIND: {}", invalid_response.status());
    
    // Test PUT to invalid path
    let put_response = client.put("invalid/path/event.ics", Bytes::from("INVALID")).await?;
    println!("   ✓ Invalid path PUT: {}", put_response.status());
    
    Ok(())
}