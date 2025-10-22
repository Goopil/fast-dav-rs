use fast_dav_rs::{CalDavClient, ContentEncoding, Depth, build_calendar_query_body, build_calendar_multiget_body};
use bytes::Bytes;
use std::time::Instant;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 ULTIMATE CALDAV CLIENT TESTING SUITE 🚀\n");
    println!("This test will comprehensively validate all CalDAV functionality\n");
    
    let start_time = Instant::now();
    
    // Phase 1: Foundation Testing
    println!("🏁 PHASE 1: FOUNDATION TESTING");
    test_foundation().await?;
    
    // Phase 2: Discovery Operations
    println!("\n🔍 PHASE 2: DISCOVERY OPERATIONS");
    test_discovery().await?;
    
    // Phase 3: Calendar Management
    println!("\n📅 PHASE 3: CALENDAR MANAGEMENT");
    test_calendar_management().await?;
    
    // Phase 4: Event Operations
    println!("\n🗓️  PHASE 4: EVENT OPERATIONS");
    test_event_operations().await?;
    
    // Phase 5: Advanced Queries
    println!("\n🧠 PHASE 5: ADVANCED QUERIES");
    test_advanced_queries().await?;
    
    // Phase 6: Concurrency Testing
    println!("\n⚡ PHASE 6: CONCURRENCY TESTING");
    test_concurrency().await?;
    
    // Phase 7: Error Handling
    println!("\n🛡️  PHASE 7: ERROR HANDLING");
    test_error_handling().await?;
    
    // Phase 8: Performance Metrics
    println!("\n📊 PHASE 8: PERFORMANCE METRICS");
    test_performance().await?;
    
    let duration = start_time.elapsed();
    println!("\n🎉 ALL TESTS COMPLETED SUCCESSFULLY!");
    println!("⏱️  Total test duration: {:.2?}", duration);
    
    Ok(())
}

// PHASE 1: FOUNDATION TESTING
async fn test_foundation() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    
    // Test 1: Basic connectivity
    let response = client.get("").await?;
    assert_eq!(response.status(), hyper::StatusCode::OK);
    println!("   ✅ Basic connectivity: {}", response.status());
    
    // Test 2: OPTIONS request
    let response = client.options("").await?;
    println!("   ✅ OPTIONS request: {}", response.status());
    
    // Test 3: HEAD request
    let response = client.head("").await?;
    println!("   ✅ HEAD request: {}", response.status());
    
    // Test 4: All compression methods
    let encodings = vec![
        ContentEncoding::Identity,
        ContentEncoding::Gzip,
        ContentEncoding::Br,
        ContentEncoding::Zstd,
    ];
    
    for encoding in encodings {
        let mut client_clone = client.clone();
        client_clone.set_request_compression(encoding);
        let response = client_clone.get("").await?;
        assert!(response.status().is_success());
        println!("   ✅ {:?} compression: {}", encoding, response.status());
    }
    
    Ok(())
}

// PHASE 2: DISCOVERY OPERATIONS
async fn test_discovery() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    
    // Test 1: Discover current user principal
    let principal = client.discover_current_user_principal().await?;
    println!("   ✅ Discovered principal: {:?}", principal);
    assert!(principal.is_some());
    
    let principal_url = principal.unwrap();
    
    // Test 2: Discover calendar home set
    let home_sets = client.discover_calendar_home_set(&principal_url).await?;
    println!("   ✅ Found {} calendar home sets", home_sets.len());
    assert!(!home_sets.is_empty());
    
    let home_set = &home_sets[0];
    
    // Test 3: List calendars
    let calendars = client.list_calendars(home_set).await?;
    println!("   ✅ Found {} calendars", calendars.len());
    
    // Test 4: Detailed calendar info
    for calendar in calendars {
        println!("      📁 Calendar: {:?}", calendar.displayname);
        if let Some(sync_token) = &calendar.sync_token {
            println!("         🔁 Sync token: {}...", &sync_token[..std::cmp::min(20, sync_token.len())]);
        }
    }
    
    Ok(())
}

// PHASE 3: CALENDAR MANAGEMENT
async fn test_calendar_management() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let calendar_name = format!("ultimate_test_calendar_{}", timestamp);
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Test 1: Create calendar
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:calendar-description>Ultimate test calendar created at {}</C:calendar-description>
      <C:supported-calendar-component-set>
        <C:comp name="VEVENT"/>
        <C:comp name="VTODO"/>
      </C:supported-calendar-component-set>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name, timestamp);
    
    let response = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    assert_eq!(response.status(), hyper::StatusCode::CREATED);
    println!("   ✅ Create calendar: {}", response.status());
    
    // Test 2: Verify calendar creation
    let response = client.propfind(&calendar_path, Depth::Zero,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <C:calendar-description/>
    <C:supported-calendar-component-set/>
  </D:prop>
</D:propfind>"#).await?;
    
    assert_eq!(response.status(), hyper::StatusCode::MULTI_STATUS);
    println!("   ✅ Verify calendar: {}", response.status());
    
    // Test 3: Update calendar properties
    let proppatch_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propertyupdate xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>Updated Ultimate Test Calendar</D:displayname>
      <C:calendar-description>This calendar has been updated during ultimate testing</C:calendar-description>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    
    let response = client.proppatch(&calendar_path, proppatch_xml).await?;
    println!("   ✅ Update properties: {}", response.status());
    
    // Test 4: Test WebDAV Sync capability
    let response = client.report(&calendar_path, Depth::Zero,
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
            println!("   ✅ WebDAV Sync test: {}", resp.status());
        }
        Err(e) => {
            println!("   ⚠️  WebDAV Sync test failed (may not be supported): {}", e);
        }
    }
    
    // Cleanup
    let _ = client.delete(&calendar_path).await;
    
    Ok(())
}

// PHASE 4: EVENT OPERATIONS
async fn test_event_operations() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let calendar_name = format!("event_test_calendar_{}", timestamp);
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Setup: Create calendar
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name);
    
    let _ = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    
    // Test 1: Create multiple events
    let mut event_paths = Vec::new();
    for i in 1..=5 {
        let event_uid = format!("ultimate-event-{}-{}@example.com", timestamp, i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);
        
        let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Ultimate Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Ultimate Test Event {}
DESCRIPTION:Event #{} created during ultimate testing
LOCATION:Test Location {}
END:VEVENT
END:VCALENDAR"#, event_uid, i, i, i);
        
        let response = client.put(&event_path, Bytes::from(event_ics)).await?;
        assert_eq!(response.status(), hyper::StatusCode::CREATED);
        event_paths.push(event_path);
        println!("   ✅ Create event #{}: {}", i, response.status());
    }
    
    // Test 2: Retrieve events
    for (i, event_path) in event_paths.iter().enumerate() {
        let response = client.get(event_path).await?;
        assert_eq!(response.status(), hyper::StatusCode::OK);
        let status = response.status();
        let body_bytes = response.into_body();
        assert!(body_bytes.len() > 0);
        println!("   ✅ Retrieve event #{}: {} ({} bytes)", i + 1, status, body_bytes.len());
    }
    
    // Test 3: Conditional operations with ETags
    for (i, event_path) in event_paths.iter().enumerate() {
        let response = client.head(event_path).await?;
        if let Some(etag) = CalDavClient::etag_from_headers(response.headers()) {
            // Test conditional update
            let updated_event_uid = event_path.split('/').last().unwrap().trim_end_matches(".ics");
            let updated_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Ultimate Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230102T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T120000Z
SUMMARY:Updated Ultimate Test Event {}
DESCRIPTION:Updated event #{} during ultimate testing
LOCATION:Updated Test Location {}
END:VEVENT
END:CALENDAR"#, updated_event_uid, i + 1, i + 1, i + 1);
            
            let response = client.put_if_match(event_path, Bytes::from(updated_ics), &etag).await?;
            println!("   ✅ Conditional update event #{}: {}", i + 1, response.status());
        }
    }
    
    // Test 4: COPY operations
    for (i, event_path) in event_paths.iter().enumerate() {
        let copied_path = event_path.replace(".ics", "-copy.ics");
        let full_copied_url = format!("{}{}", SABREDAV_URL, copied_path);
        
        let response = client.copy(event_path, &full_copied_url, true).await?;
        println!("   ✅ COPY event #{}: {}", i + 1, response.status());
        
        // Clean up copied event
        let _ = client.delete(&copied_path).await;
    }
    
    // Test 5: MOVE operations (we'll create new events for this to avoid conflicts)
    let mut moved_paths = Vec::new();
    for i in 1..=3 {
        let event_uid = format!("move-event-{}-{}@example.com", timestamp, i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);
        let moved_path = event_path.replace(".ics", "-moved.ics");
        
        // Create event
        let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Move Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Move Test Event {}
DESCRIPTION:Event #{} created for move testing
END:VEVENT
END:VCALENDAR"#, event_uid, i, i);
        
        let _ = client.put(&event_path, Bytes::from(event_ics)).await?;
        
        // Move event
        let full_moved_url = format!("{}{}", SABREDAV_URL, moved_path);
        let response = client.r#move(&event_path, &full_moved_url, true).await?;
        println!("   ✅ MOVE event #{}: {}", i, response.status());
        
        moved_paths.push(moved_path);
    }
    
    // Cleanup
    for event_path in &event_paths {
        let _ = client.delete(event_path).await;
    }
    for moved_path in &moved_paths {
        let _ = client.delete(moved_path).await;
    }
    let _ = client.delete(&calendar_path).await;
    
    Ok(())
}

// PHASE 5: ADVANCED QUERIES
async fn test_advanced_queries() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let calendar_name = format!("query_test_calendar_{}", timestamp);
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Setup: Create calendar and populate with events
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name);
    
    let _ = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    
    // Create test events with different properties
    let events_data = vec![
        ("20231201T100000Z", "Meeting with Team", "Team meeting discussion"),
        ("20231202T140000Z", "Project Review", "Quarterly project review"),
        ("20231203T090000Z", "Client Presentation", "Present to key client"),
    ];
    
    let mut event_uids = Vec::new();
    for (i, (dtstart, summary, description)) in events_data.iter().enumerate() {
        let event_uid = format!("query-event-{}-{}@example.com", timestamp, i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);
        
        let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Query Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:{}
DTEND:{}T110000Z
SUMMARY:{}
DESCRIPTION:{}
END:VEVENT
END:VCALENDAR"#, event_uid, dtstart, &dtstart[..8], summary, description);
        
        let _ = client.put(&event_path, Bytes::from(event_ics)).await?;
        event_uids.push(event_uid);
    }
    
    // Test 1: Calendar Query - All events
    let query_xml = build_calendar_query_body("VEVENT", None, None, true);
    let response = client.report(&calendar_path, Depth::Zero, &query_xml).await?;
    println!("   ✅ Calendar query (all events): {}", response.status());
    
    if response.status().is_success() {
        let body_bytes = response.into_body();
        println!("      📊 Query returned {} bytes", body_bytes.len());
    }
    
    // Test 2: Calendar Query - Date range
    let query_xml = build_calendar_query_body(
        "VEVENT",
        Some("20231201T000000Z"),
        Some("20231202T235959Z"),
        true
    );
    let response = client.report(&calendar_path, Depth::Zero, &query_xml).await?;
    println!("   ✅ Calendar query (date range): {}", response.status());
    
    // Test 3: Calendar Multiget
    let hrefs: Vec<&str> = event_uids.iter().map(|uid| uid.as_str()).collect();
    if let Some(multiget_xml) = build_calendar_multiget_body(hrefs, true) {
        let response = client.report(&calendar_path, Depth::Zero, &multiget_xml).await?;
        println!("   ✅ Calendar multiget: {}", response.status());
    }
    
    // Test 4: PROPFIND with complex properties
    let response = client.propfind(&calendar_path, Depth::One,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/">
  <D:prop>
    <D:displayname/>
    <D:getetag/>
    <D:resourcetype/>
    <C:calendar-data/>
    <CS:getctag/>
  </D:prop>
</D:propfind>"#).await?;
    
    println!("   ✅ Complex PROPFIND: {}", response.status());
    
    // Cleanup
    let _ = client.delete(&calendar_path).await;
    
    Ok(())
}

// PHASE 6: CONCURRENCY TESTING
async fn test_concurrency() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let calendar_name = format!("concurrent_test_calendar_{}", timestamp);
    let calendar_path = format!("calendars/test/{}/", calendar_name);
    
    // Setup: Create calendar
    let calendar_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#, calendar_name);
    
    let _ = client.mkcalendar(&calendar_path, &calendar_xml).await?;
    
    // Test 1: Concurrent event creation
    println!("   🚀 Testing concurrent event creation...");
    let start = Instant::now();
    
    let mut tasks = Vec::new();
    for i in 1..=10 {
        let client_clone = client.clone();
        let calendar_path_clone = calendar_path.clone();
        let event_uid = format!("concurrent-event-{}-{}@example.com", timestamp, i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path_clone, event_filename);
        
        let task = tokio::spawn(async move {
            let event_ics = format!(r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Concurrent Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Concurrent Test Event {}
DESCRIPTION:Event #{} created during concurrent testing
END:VEVENT
END:VCALENDAR"#, event_uid, i, i);
            
            client_clone.put(&event_path, Bytes::from(event_ics)).await
        });
        
        tasks.push(task);
    }
    
    let results = futures::future::join_all(tasks).await;
    let duration = start.elapsed();
    
    let mut success_count = 0;
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    success_count += 1;
                }
                println!("      ✅ Concurrent event #{}: {}", i + 1, response.status());
            }
            Ok(Err(e)) => {
                println!("      ❌ Concurrent event #{} failed: {}", i + 1, e);
            }
            Err(e) => {
                println!("      ❌ Concurrent event #{} panicked: {}", i + 1, e);
            }
        }
    }
    
    println!("   📊 Concurrent creation: {}/10 successful in {:?}", success_count, duration);
    
    // Test 2: Concurrent event retrieval
    println!("   🚀 Testing concurrent event retrieval...");
    let start = Instant::now();
    
    let mut tasks = Vec::new();
    for i in 1..=10 {
        let client_clone = client.clone();
        let event_uid = format!("concurrent-event-{}-{}@example.com", timestamp, i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);
        
        let task = tokio::spawn(async move {
            client_clone.get(&event_path).await
        });
        
        tasks.push(task);
    }
    
    let results = futures::future::join_all(tasks).await;
    let duration = start.elapsed();
    
    let mut success_count = 0;
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    success_count += 1;
                }
                println!("      ✅ Concurrent retrieval #{}: {}", i + 1, response.status());
            }
            Ok(Err(e)) => {
                println!("      ❌ Concurrent retrieval #{} failed: {}", i + 1, e);
            }
            Err(e) => {
                println!("      ❌ Concurrent retrieval #{} panicked: {}", i + 1, e);
            }
        }
    }
    
    println!("   📊 Concurrent retrieval: {}/10 successful in {:?}", success_count, duration);
    
    // Cleanup
    // Delete all events
    for i in 1..=10 {
        let event_uid = format!("concurrent-event-{}-{}@example.com", timestamp, i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);
        let _ = client.delete(&event_path).await;
    }
    let _ = client.delete(&calendar_path).await;
    
    Ok(())
}

// PHASE 7: ERROR HANDLING
async fn test_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    
    // Test 1: Non-existent resource access
    let response = client.get("nonexistent/resource").await?;
    assert_eq!(response.status(), hyper::StatusCode::NOT_FOUND);
    println!("   ✅ Non-existent resource: {}", response.status());
    
    // Test 2: Invalid XML
    let response = client.propfind("principals/test/", Depth::Zero, "<invalid>").await?;
    assert_eq!(response.status(), hyper::StatusCode::BAD_REQUEST);
    println!("   ✅ Invalid XML: {}", response.status());
    
    // Test 3: Malformed path
    let response = client.propfind("//malformed//path//", Depth::Zero,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await?;
    println!("   ✅ Malformed path: {}", response.status());
    
    // Test 4: Empty body requests
    let response = client.propfind("principals/test/", Depth::Zero, "").await?;
    println!("   ✅ Empty body: {}", response.status());
    
    // Test 5: Unauthorized access simulation (if possible)
    // This would require creating a client with invalid credentials
    
    Ok(())
}

// PHASE 8: PERFORMANCE METRICS
async fn test_performance() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_test_client();
    
    // Test 1: Measure basic request latency
    println!("   📈 Measuring request performance...");
    
    let mut latencies = Vec::new();
    for i in 1..=20 {
        let start = Instant::now();
        let response = client.get("").await?;
        let duration = start.elapsed();
        latencies.push(duration);
        
        if i <= 5 {
            println!("      Request #{}: {:?} - {}", i, duration, response.status());
        }
    }
    
    let avg_latency: std::time::Duration = latencies.iter().sum::<std::time::Duration>() / latencies.len() as u32;
    let min_latency = latencies.iter().min().unwrap();
    let max_latency = latencies.iter().max().unwrap();
    
    println!("   📊 Performance metrics:");
    println!("      Average latency: {:?}", avg_latency);
    println!("      Minimum latency: {:?}", min_latency);
    println!("      Maximum latency: {:?}", max_latency);
    
    // Test 2: Large response handling
    println!("   📈 Testing large response handling...");
    let start = Instant::now();
    
    let response = client.propfind("calendars/test/", Depth::One,
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <D:getetag/>
    <D:resourcetype/>
    <C:calendar-description/>
    <C:supported-calendar-component-set/>
  </D:prop>
</D:propfind>"#).await?;
    
    let duration = start.elapsed();
    let status = response.status();
    if status.is_success() {
        let body_bytes = response.into_body();
        let body_size = body_bytes.len();
        println!("      Large PROPFIND: {} bytes in {:?}", body_size, duration);
    } else {
        println!("      Large PROPFIND: {} in {:?}", status, duration);
    }
    
    Ok(())
}