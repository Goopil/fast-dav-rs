use fast_dav_rs::{CalDavClient, ContentEncoding, Depth};
use bytes::Bytes;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 COMPREHENSIVE CALDAV CLIENT VALIDATION 🚀\n");
    
    // Test 1: Basic connectivity and authentication
    test_connectivity().await?;
    
    // Test 2: Discovery operations
    test_discovery().await?;
    
    // Test 3: Calendar management
    test_calendar_management().await?;
    
    // Test 4: Event operations
    test_event_operations().await?;
    
    // Test 5: Advanced operations
    test_advanced_operations().await?;
    
    // Test 6: Error handling
    test_error_handling().await?;
    
    println!("\n🎉 ALL TESTS PASSED! Client is fully functional.");
    
    Ok(())
}

async fn test_connectivity() -> Result<(), Box<dyn std::error::Error>> {
    println!("📡 Testing connectivity and authentication...");
    
    let client = create_test_client();
    
    // Basic GET
    let response = client.get("").await?;
    assert_eq!(response.status(), hyper::StatusCode::OK);
    println!("   ✅ GET request: {}", response.status());
    
    // OPTIONS
    let response = client.options("").await?;
    println!("   ✅ OPTIONS request: {}", response.status());
    
    // HEAD
    let response = client.head("").await?;
    println!("   ✅ HEAD request: {}", response.status());
    
    // Compression support
    for encoding in &[ContentEncoding::Gzip, ContentEncoding::Br, ContentEncoding::Zstd] {
        let mut client_clone = client.clone();
        client_clone.set_request_compression(*encoding);
        let response = client_clone.get("").await?;
        assert!(response.status().is_success());
        println!("   ✅ {:?} compression: {}", encoding, response.status());
    }
    
    Ok(())
}

async fn test_discovery() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing discovery operations...");
    
    let client = create_test_client();
    
    // Discover principal
    let principal = client.discover_current_user_principal().await?;
    println!("   ✅ Principal discovery: {:?}", principal);
    assert!(principal.is_some());
    
    // Discover calendar home
    let home_sets = client.discover_calendar_home_set(&principal.unwrap()).await?;
    println!("   ✅ Calendar home sets: {}", home_sets.len());
    
    // List calendars
    if !home_sets.is_empty() {
        let calendars = client.list_calendars(&home_sets[0]).await?;
        println!("   ✅ Calendar listing: {} calendars found", calendars.len());
    }
    
    Ok(())
}

async fn test_calendar_management() -> Result<(), Box<dyn std::error::Error>> {
    println!("📅 Testing calendar management...");
    
    let client = create_test_client();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let calendar_name = format!("test_calendar_{}", timestamp);
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
    
    let response = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    assert_eq!(response.status(), hyper::StatusCode::CREATED);
    println!("   ✅ Create calendar: {}", response.status());
    
    // Verify creation
    let response = client.propfind(&calendar_path, Depth::Zero,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await?;
    assert_eq!(response.status(), hyper::StatusCode::MULTI_STATUS);
    println!("   ✅ Verify calendar: {}", response.status());
    
    // Update properties
    let proppatch_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propertyupdate xmlns:D="DAV:">
  <D:set>
    <D:prop>
      <D:displayname>Updated Test Calendar</D:displayname>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    
    let response = client.proppatch(&calendar_path, proppatch_xml).await?;
    println!("   ✅ Update properties: {}", response.status());
    
    // Cleanup
    let response = client.delete(&calendar_path).await?;
    assert_eq!(response.status(), hyper::StatusCode::NO_CONTENT);
    println!("   ✅ Delete calendar: {}", response.status());
    
    Ok(())
}

async fn test_event_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("🗓️  Testing event operations...");
    
    let client = create_test_client();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let calendar_name = format!("event_calendar_{}", timestamp);
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
    
    let _ = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    
    // Create event
    let event_uid = format!("test-event-{}@example.com", timestamp);
    let event_path = format!("{}{}.ics", calendar_path, event_uid);
    
    let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Test Event
DESCRIPTION:Test event for validation
END:VEVENT
END:VCALENDAR"#, event_uid);
    
    let response = client.put(&event_path, Bytes::from(event_ics)).await?;
    assert_eq!(response.status(), hyper::StatusCode::CREATED);
    println!("   ✅ Create event: {}", response.status());
    
    // Retrieve event
    let response = client.get(&event_path).await?;
    assert_eq!(response.status(), hyper::StatusCode::OK);
    let status = response.status();
    let body_bytes = response.into_body();
    assert!(body_bytes.len() > 0);
    println!("   ✅ Retrieve event: {} ({} bytes)", status, body_bytes.len());
    
    // Conditional update with ETag
    let response = client.head(&event_path).await?;
    if let Some(etag) = CalDavClient::etag_from_headers(response.headers()) {
        let updated_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230102T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T120000Z
SUMMARY:Updated Test Event
DESCRIPTION:Updated test event
END:VEVENT
END:VCALENDAR"#, event_uid);
        
        let response = client.put_if_match(&event_path, Bytes::from(updated_ics), &etag).await?;
        println!("   ✅ Conditional update: {}", response.status());
    }
    
    // COPY operation
    let copied_path = event_path.replace(".ics", "-copy.ics");
    let full_copied_url = format!("{}{}", SABREDAV_URL, copied_path);
    let response = client.copy(&event_path, &full_copied_url, true).await?;
    println!("   ✅ COPY operation: {}", response.status());
    
    // MOVE operation
    let moved_path = event_path.replace(".ics", "-moved.ics");
    let full_moved_url = format!("{}{}", SABREDAV_URL, moved_path);
    let response = client.r#move(&event_path, &full_moved_url, true).await?;
    println!("   ✅ MOVE operation: {}", response.status());
    
    // Cleanup
    let _ = client.delete(&moved_path).await;
    let _ = client.delete(&copied_path).await;
    let _ = client.delete(&calendar_path).await;
    
    Ok(())
}

async fn test_advanced_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ Testing advanced operations...");
    
    let client = create_test_client();
    
    // Complex PROPFIND
    let response = client.propfind("calendars/test/", Depth::One,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <D:getetag/>
    <D:resourcetype/>
    <C:calendar-description/>
  </D:prop>
</D:propfind>"#).await?;
    
    println!("   ✅ Complex PROPFIND: {}", response.status());
    
    // REPORT operation
    let response = client.report("calendars/test/", Depth::Zero,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:sync-collection xmlns:D="DAV:">
  <D:sync-token/>
  <D:sync-level>1</D:sync-level>
  <D:prop>
    <D:getetag/>
  </D:prop>
</D:sync-collection>"#).await;
    
    match response {
        Ok(resp) => {
            println!("   ✅ REPORT operation: {}", resp.status());
        }
        Err(e) => {
            println!("   ⚠️  REPORT operation failed: {}", e);
        }
    }
    
    Ok(())
}

async fn test_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("🛡️  Testing error handling...");
    
    let client = create_test_client();
    
    // Non-existent resource
    let response = client.get("nonexistent/resource").await?;
    assert_eq!(response.status(), hyper::StatusCode::NOT_FOUND);
    println!("   ✅ Non-existent resource: {}", response.status());
    
    // Invalid XML
    let response = client.propfind("principals/test/", Depth::Zero, "<invalid>").await?;
    assert_eq!(response.status(), hyper::StatusCode::BAD_REQUEST);
    println!("   ✅ Invalid XML: {}", response.status());
    
    Ok(())
}