use bytes::Bytes;
use fast_dav_rs::CalDavClient;
use std::collections::HashMap;
use std::slice;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    format!(
        "comparison_test_calendar_{}",
        chrono::Utc::now().timestamp_millis()
    )
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    format!(
        "comparison-event-{}@example.com",
        chrono::Utc::now().timestamp_millis()
    )
}

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

/// Traditional sync method: fetch ctag of all calendars (in parallel) then fetch data for changed calendars
async fn traditional_sync_method(
    client: &CalDavClient,
    calendar_paths: &[String],
) -> anyhow::Result<HashMap<String, String>> {
    // Step 1: Fetch ctags for all calendars in parallel
    let mut ctag_futures = Vec::new();
    for calendar_path in calendar_paths {
        let client_clone = client.clone();
        let path_clone = calendar_path.clone();
        ctag_futures.push(tokio::spawn(async move {
            let propfind_body = r#"
            <D:propfind xmlns:D="DAV:">
              <D:prop>
                <D:getctag/>
                <D:displayname/>
              </D:prop>
            </D:propfind>"#;

            let response = client_clone
                .propfind(&path_clone, fast_dav_rs::Depth::Zero, propfind_body)
                .await;
            (path_clone, response)
        }));
    }

    // Collect ctag results
    let ctag_results = futures::future::join_all(ctag_futures).await;

    // Step 2: Identify changed calendars and fetch their data
    let mut calendar_data = HashMap::new();
    let mut data_futures = Vec::new();

    for result in ctag_results {
        if let Ok((calendar_path, Ok(response))) = result {
            // Parse ctag from response (simplified for this example)
            let body = response.into_body();
            let ctag = String::from_utf8_lossy(&body);
            calendar_data.insert(calendar_path.clone(), ctag.to_string());

            // For demonstration, we'll fetch calendar data for all calendars
            // In a real implementation, we'd only fetch data for changed calendars
            let client_clone = client.clone();
            let path_clone = calendar_path.clone();
            data_futures.push(tokio::spawn(async move {
                let query_body = r#"
                <C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
                  <D:prop>
                    <D:getetag/>
                    <C:calendar-data/>
                  </D:prop>
                  <C:filter>
                    <C:comp-filter name="VCALENDAR">
                      <C:comp-filter name="VEVENT"/>
                    </C:comp-filter>
                  </C:filter>
                </C:calendar-query>"#;

                client_clone
                    .report(&path_clone, fast_dav_rs::Depth::One, query_body)
                    .await
                    .map(|r| (path_clone, r))
            }));
        }
    }

    // Collect calendar data
    let data_results = futures::future::join_all(data_futures).await;
    for result in data_results {
        if let Ok(Ok((calendar_path, response))) = result {
            let body = response.into_body();
            let data = String::from_utf8_lossy(&body);
            calendar_data.insert(format!("{}_data", calendar_path), data.to_string());
        }
    }

    Ok(calendar_data)
}

/// WebDAV Sync method: use sync-collection REPORT to get changes per calendar
async fn webdav_sync_method(
    client: &CalDavClient,
    calendar_path: &str,
    sync_token: Option<&str>,
) -> anyhow::Result<(Option<String>, Vec<String>)> {
    // Use the sync_collection method implemented in the client
    let sync_response = client
        .sync_collection(calendar_path, sync_token, Some(100), true)
        .await?;

    // Extract relevant data from sync response
    let mut changes = Vec::new();
    for item in sync_response.items {
        if let Some(href) = item.etag {
            changes.push(href);
        }
    }

    Ok((sync_response.sync_token, changes))
}

#[tokio::test]
async fn test_traditional_vs_webdav_sync() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for testing
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
        panic!("Failed to create calendar: {}", e);
    }
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful calendar creation"
    );

    // Add some test events to the calendar
    let mut event_paths = Vec::new();
    let mut event_uids = Vec::new();
    for i in 1..=3 {
        let event_uid = format!("{}-traditional-{}", generate_unique_event_uid(), i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);

        let event_ics = format!(
            r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Comparison Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Traditional Sync Test Event {}
DESCRIPTION:Event #{} for traditional vs WebDAV sync comparison
END:VEVENT
END:VCALENDAR"#,
            event_uid, i, i
        );

        let response = client.put(&event_path, Bytes::from(event_ics)).await;
        match response {
            Ok(resp) => {
                assert!(
                    resp.status().is_success(),
                    "Expected successful event creation for event {}",
                    i
                );
                event_paths.push(event_path);
                event_uids.push(event_uid);
                println!("Created event {}: {}", i, resp.status());
            }
            Err(e) => {
                panic!("Error creating event {}: {}", i, e);
            }
        }
    }

    // Test traditional sync method
    println!("Testing traditional sync method...");
    let traditional_result =
        traditional_sync_method(&client, slice::from_ref(&calendar_path)).await;
    match traditional_result {
        Ok(data) => {
            println!(
                "Traditional sync completed. Found {} calendar entries",
                data.len()
            );
            // Verify we got some data
            assert!(!data.is_empty(), "Expected non-empty traditional sync data");
        }
        Err(e) => {
            panic!("Traditional sync failed: {}", e);
        }
    }

    // Test WebDAV sync method
    println!("Testing WebDAV sync method...");
    let webdav_result = webdav_sync_method(&client, &calendar_path, None).await;
    let sync_token = match webdav_result {
        Ok((sync_token, changes)) => {
            println!(
                "WebDAV sync completed. Sync token: {:?}, Changes: {}",
                sync_token,
                changes.len()
            );
            // With initial sync, we should get at least the events we created
            assert!(
                changes.len() >= event_uids.len(),
                "Expected at least {} changes, got {}",
                event_uids.len(),
                changes.len()
            );
            sync_token
        }
        Err(e) => {
            panic!("WebDAV sync failed: {}", e);
        }
    };

    // Validate that all our events are present in the sync response
    // Note: This is a simplified validation - in practice we'd need to parse the actual hrefs

    // Add more events for incremental sync test
    let mut new_event_paths = Vec::new();
    let mut new_event_uids = Vec::new();
    for i in 1..=2 {
        let event_uid = format!("{}-incremental-{}", generate_unique_event_uid(), i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);

        let event_ics = format!(
            r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Incremental Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231226T100000Z
DTEND:20231226T110000Z
SUMMARY:Incremental Sync Test Event {}
DESCRIPTION:Event #{} for incremental sync testing
END:VEVENT
END:VCALENDAR"#,
            event_uid, i, i
        );

        let response = client.put(&event_path, Bytes::from(event_ics)).await;
        match response {
            Ok(resp) => {
                assert!(
                    resp.status().is_success(),
                    "Expected successful incremental event creation for event {}",
                    i
                );
                new_event_paths.push(event_path);
                new_event_uids.push(event_uid);
                println!("Created incremental event {}: {}", i, resp.status());
            }
            Err(e) => {
                panic!("Error creating incremental event {}: {}", i, e);
            }
        }
    }

    // Test incremental sync with both methods
    println!("Testing incremental sync...");

    // Traditional method would need to check ctag again
    let traditional_incremental =
        traditional_sync_method(&client, slice::from_ref(&calendar_path)).await;
    match traditional_incremental {
        Ok(data) => {
            println!(
                "Traditional incremental sync completed. Found {} calendar entries",
                data.len()
            );
        }
        Err(e) => {
            panic!("Traditional incremental sync failed: {}", e);
        }
    }

    // WebDAV incremental sync
    if let Some(token) = sync_token.as_deref() {
        let webdav_incremental = webdav_sync_method(&client, &calendar_path, Some(token)).await;
        match webdav_incremental {
            Ok((new_token, changes)) => {
                println!(
                    "WebDAV incremental sync completed. New sync token: {:?}, Changes: {}",
                    new_token,
                    changes.len()
                );
                // Should have changes for the new events
                assert!(
                    changes.len() >= new_event_uids.len(),
                    "Expected at least {} incremental changes, got {}",
                    new_event_uids.len(),
                    changes.len()
                );

                // Token should be different
                assert_ne!(
                    new_token.as_deref(),
                    Some(token),
                    "Expected different sync token after changes"
                );
            }
            Err(e) => {
                panic!("WebDAV incremental sync failed: {}", e);
            }
        }
    } else {
        println!("Skipping WebDAV incremental sync - no sync token available");
    }

    // Test deletion tracking
    if !new_event_paths.is_empty() {
        let delete_path = &new_event_paths[0];
        let delete_response = client.delete(delete_path).await;
        match delete_response {
            Ok(resp) => {
                assert!(resp.status().is_success(), "Expected successful deletion");
                println!("Deleted event: {}", resp.status());
            }
            Err(e) => {
                panic!("Failed to delete event: {}", e);
            }
        }

        // Test sync after deletion
        if let Some(token) = sync_token.as_deref() {
            let webdav_deletion_sync =
                webdav_sync_method(&client, &calendar_path, Some(token)).await;
            match webdav_deletion_sync {
                Ok((_new_token, changes)) => {
                    println!("WebDAV deletion sync completed. Changes: {}", changes.len());
                    // Should have changes (including the deletion)
                    assert!(!changes.is_empty(), "Expected changes after deletion");
                }
                Err(e) => {
                    panic!("WebDAV deletion sync failed: {}", e);
                }
            }
        }
    }

    // Clean up
    for event_path in event_paths.into_iter().chain(new_event_paths.into_iter()) {
        let _ = client.delete(&event_path).await;
    }
    let _ = client.delete(&calendar_path).await;

    println!("Sync comparison test completed");
}

#[tokio::test]
async fn test_sync_data_integrity() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for data integrity testing
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
        panic!("Failed to create calendar: {}", e);
    }
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful calendar creation"
    );

    // Create an event with specific data
    let event_uid = format!("{}-integrity", generate_unique_event_uid());
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    let event_summary = "Data Integrity Test Event";
    let event_description = "This event is used to test data integrity in sync operations";

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Integrity Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231227T100000Z
DTEND:20231227T110000Z
SUMMARY:{}
DESCRIPTION:{}
END:VEVENT
END:VCALENDAR"#,
        event_uid, event_summary, event_description
    );

    let put_response = client
        .put(&event_path, Bytes::from(event_ics.clone()))
        .await;
    match put_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected successful event creation"
            );
            println!("Created integrity test event: {}", resp.status());
        }
        Err(e) => {
            panic!("Error creating integrity test event: {}", e);
        }
    }

    // Test WebDAV sync method and validate data integrity
    let sync_response = client
        .sync_collection(&calendar_path, None, Some(100), true)
        .await;
    match sync_response {
        Ok(response) => {
            println!("Sync completed with {} items", response.items.len());

            // Find our specific event
            let event_item = response
                .items
                .iter()
                .find(|item| item.href.contains(&event_uid));

            match event_item {
                Some(item) => {
                    println!("Found event in sync response");

                    // Validate that we have calendar data
                    assert!(
                        item.calendar_data.is_some(),
                        "Expected calendar data for event"
                    );

                    let calendar_data = item.calendar_data.as_ref().unwrap();
                    println!("Calendar data length: {} characters", calendar_data.len());

                    // Validate that the calendar data contains our expected content
                    assert!(
                        calendar_data.contains(&event_uid),
                        "Calendar data should contain event UID"
                    );
                    assert!(
                        calendar_data.contains(event_summary),
                        "Calendar data should contain event summary"
                    );
                    assert!(
                        calendar_data.contains(event_description),
                        "Calendar data should contain event description"
                    );

                    // Validate ETag presence
                    assert!(item.etag.is_some(), "Expected ETag for event");
                    println!("Event ETag: {:?}", item.etag);

                    // Validate that event is not marked as deleted
                    assert!(!item.is_deleted, "Event should not be marked as deleted");
                }
                None => {
                    panic!("Expected to find our test event in sync response");
                }
            }
        }
        Err(e) => {
            panic!("Sync failed: {}", e);
        }
    }

    // Test lightweight sync (without data) and validate ETag integrity
    let lightweight_sync = client
        .sync_collection(&calendar_path, None, Some(100), false)
        .await;
    match lightweight_sync {
        Ok(response) => {
            println!(
                "Lightweight sync completed with {} items",
                response.items.len()
            );

            // Find our specific event
            let event_item = response
                .items
                .iter()
                .find(|item| item.href.contains(&event_uid));

            match event_item {
                Some(item) => {
                    println!("Found event in lightweight sync response");

                    // Validate that we have ETag but no calendar data
                    assert!(
                        item.etag.is_some(),
                        "Expected ETag for event in lightweight sync"
                    );
                    assert!(
                        item.calendar_data.is_none(),
                        "Expected no calendar data in lightweight sync"
                    );
                    println!("Lightweight sync ETag: {:?}", item.etag);

                    // Validate that event is not marked as deleted
                    assert!(
                        !item.is_deleted,
                        "Event should not be marked as deleted in lightweight sync"
                    );
                }
                None => {
                    panic!("Expected to find our test event in lightweight sync response");
                }
            }
        }
        Err(e) => {
            panic!("Lightweight sync failed: {}", e);
        }
    }

    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;

    println!("Sync data integrity test completed");
}

#[tokio::test]
async fn test_sync_performance_comparison() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for performance testing
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
        panic!("Failed to create calendar: {}", e);
    }
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful calendar creation"
    );

    // Add multiple test events to the calendar
    let mut event_paths = Vec::new();
    let mut event_uids = Vec::new();
    for i in 1..=10 {
        let event_uid = format!("{}-perf-{}", generate_unique_event_uid(), i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);

        let event_ics = format!(
            r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Performance Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T{}0000Z
DTEND:20231225T{}0000Z
SUMMARY:Performance Test Event {}
DESCRIPTION:Event #{} for performance testing
END:VEVENT
END:VCALENDAR"#,
            event_uid,
            10 + i,
            11 + i,
            i,
            i
        );

        let response = client.put(&event_path, Bytes::from(event_ics)).await;
        match response {
            Ok(resp) => {
                assert!(
                    resp.status().is_success(),
                    "Expected successful event creation for event {}",
                    i
                );
                event_paths.push(event_path);
                event_uids.push(event_uid);
            }
            Err(e) => {
                panic!("Error creating event {}: {}", i, e);
            }
        }
    }

    println!(
        "Created {} events for performance testing",
        event_paths.len()
    );

    // Measure traditional sync method
    let traditional_start = std::time::Instant::now();
    let traditional_result =
        traditional_sync_method(&client, slice::from_ref(&calendar_path)).await;
    let traditional_duration = traditional_start.elapsed();

    // Validate traditional sync result
    match traditional_result {
        Ok(data) => {
            assert!(!data.is_empty(), "Expected non-empty traditional sync data");
        }
        Err(e) => {
            panic!("Traditional sync failed: {}", e);
        }
    }

    // Measure WebDAV sync method
    let webdav_start = std::time::Instant::now();
    let webdav_result = webdav_sync_method(&client, &calendar_path, None).await;
    let webdav_duration = webdav_start.elapsed();

    // Validate WebDAV sync result
    match webdav_result {
        Ok((_, changes)) => {
            // Should have changes for all our events
            assert!(
                changes.len() >= event_uids.len(),
                "Expected at least {} changes, got {}",
                event_uids.len(),
                changes.len()
            );
        }
        Err(e) => {
            panic!("WebDAV sync failed: {}", e);
        }
    }

    println!("Performance comparison:");
    println!("  Traditional sync: {:?}", traditional_duration);
    println!("  WebDAV sync: {:?}", webdav_duration);

    // Validate performance improvement (WebDAV should be faster)
    if webdav_duration < traditional_duration {
        let improvement = (traditional_duration.as_micros() as f64
            - webdav_duration.as_micros() as f64)
            / traditional_duration.as_micros() as f64
            * 100.0;
        println!("✅ WebDAV sync is {:.2}% faster", improvement);
    } else {
        println!("⚠️  WebDAV sync is not faster than traditional sync");
    }

    // Clean up
    for event_path in event_paths {
        let _ = client.delete(&event_path).await;
    }
    let _ = client.delete(&calendar_path).await;

    println!("Performance test completed");
}

#[tokio::test]
async fn test_parallel_sync_consistency() {
    let client = create_test_client();
    let mut calendar_paths = Vec::new();
    let mut event_paths_collection = Vec::new();

    // Create multiple calendars for parallel sync testing
    for i in 1..=3 {
        let calendar_name = format!(
            "parallel_test_calendar_{}_{}",
            i,
            chrono::Utc::now().timestamp_millis()
        );
        let calendar_path = format!("calendars/test/{}/", calendar_name);

        // Create a calendar
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
            panic!("Failed to create calendar {}: {}", i, e);
        }
        assert!(
            mk_response.unwrap().status().is_success(),
            "Expected successful calendar creation for calendar {}",
            i
        );

        // Add events to this calendar
        let mut event_paths = Vec::new();
        for j in 1..=2 {
            let event_uid = format!("{}-parallel-{}-{}", generate_unique_event_uid(), i, j);
            let event_filename = format!("{}.ics", event_uid);
            let event_path = format!("{}{}", calendar_path, event_filename);

            let event_ics = format!(
                r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Parallel Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T{}0000Z
DTEND:20231225T{}0000Z
SUMMARY:Parallel Test Event {}-{}
DESCRIPTION:Event {}-{} for parallel sync consistency testing
END:VEVENT
END:VCALENDAR"#,
                event_uid,
                10 + j,
                11 + j,
                i,
                j,
                i,
                j
            );

            let response = client.put(&event_path, Bytes::from(event_ics)).await;
            match response {
                Ok(resp) => {
                    assert!(
                        resp.status().is_success(),
                        "Expected successful event creation for event {}-{}",
                        i,
                        j
                    );
                    event_paths.push(event_path);
                }
                Err(e) => {
                    panic!("Error creating event {}-{}: {}", i, j, e);
                }
            }
        }

        calendar_paths.push(calendar_path);
        event_paths_collection.push(event_paths);
    }

    println!(
        "Created {} calendars with events for parallel sync testing",
        calendar_paths.len()
    );

    // Perform parallel sync operations
    let mut sync_tasks = Vec::new();
    let mut expected_event_counts = Vec::new();

    for (i, calendar_path) in calendar_paths.iter().enumerate() {
        let client_clone = client.clone();
        let path_clone = calendar_path.clone();
        let event_count = event_paths_collection[i].len();
        expected_event_counts.push(event_count);

        let task = tokio::spawn(async move {
            // Lightweight sync first to get baseline
            let lightweight_sync = client_clone
                .sync_collection(&path_clone, None, Some(100), false)
                .await;
            match lightweight_sync {
                Ok(response) => {
                    println!(
                        "Lightweight sync for calendar {}: {} items",
                        i + 1,
                        response.items.len()
                    );
                    // Validate ETags only
                    let etag_count = response
                        .items
                        .iter()
                        .filter(|item| item.etag.is_some() && item.calendar_data.is_none())
                        .count();
                    println!("  ETag-only items: {}", etag_count);
                }
                Err(e) => {
                    println!("Lightweight sync failed for calendar {}: {}", i + 1, e);
                }
            }

            // Full data sync
            let full_sync = client_clone
                .sync_collection(&path_clone, None, Some(100), true)
                .await;
            match full_sync {
                Ok(response) => {
                    println!(
                        "Full sync for calendar {}: {} items",
                        i + 1,
                        response.items.len()
                    );
                    (i, Ok(response.items.len()))
                }
                Err(e) => {
                    println!("Full sync failed for calendar {}: {}", i + 1, e);
                    (i, Err(e))
                }
            }
        });

        sync_tasks.push(task);
    }

    // Wait for all parallel syncs to complete
    let sync_results = futures::future::join_all(sync_tasks).await;

    // Validate results
    for result in sync_results.into_iter() {
        match result {
            Ok((calendar_index, Ok(item_count))) => {
                let expected_count = expected_event_counts[calendar_index];
                assert!(
                    item_count >= expected_count,
                    "Calendar {} expected at least {} items, got {}",
                    calendar_index + 1,
                    expected_count,
                    item_count
                );
                println!(
                    "✅ Calendar {} sync validated: {} items (expected at least {})",
                    calendar_index + 1,
                    item_count,
                    expected_count
                );
            }
            Ok((calendar_index, Err(e))) => {
                panic!("Sync failed for calendar {}: {}", calendar_index + 1, e);
            }
            Err(e) => {
                panic!("Task panicked for calendar: {}", e);
            }
        }
    }

    // Clean up
    for (calendar_path, event_paths) in calendar_paths
        .into_iter()
        .zip(event_paths_collection.into_iter())
    {
        for event_path in event_paths {
            let _ = client.delete(&event_path).await;
        }
        let _ = client.delete(&calendar_path).await;
    }

    println!("Parallel sync consistency test completed");
}
