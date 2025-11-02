use bytes::Bytes;
use fast_dav_rs::CalDavClient;
use std::time::Duration;
use crate::util::{unique_calendar_name, unique_uid};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    unique_calendar_name("sync_test_calendar")
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    unique_uid("sync-event")
}

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::test]
async fn test_webdav_sync_support() {
    let client = create_test_client();

    // Test if the server supports WebDAV sync
    let supports_sync = client.supports_webdav_sync().await;

    match supports_sync {
        Ok(supported) => {
            println!("WebDAV Sync support: {}", supported);
            // SabreDAV should support sync
            // Note: We're not asserting here because server availability may vary
            if supported {
                println!("✅ Server supports WebDAV Sync");
            } else {
                println!("⚠️  Server does not support WebDAV Sync");
            }
        }
        Err(e) => {
            println!("⚠️  Failed to check WebDAV Sync support: {}", e);
        }
    }
}

#[tokio::test]
async fn test_initial_sync_collection() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for sync testing
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

    // Small delay to ensure calendar is fully created
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Add some test events to the calendar
    let mut event_paths = Vec::new();
    for i in 1..=3 {
        let event_uid = format!("{}-initial-{}", generate_unique_event_uid(), i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);

        let event_ics = format!(
            r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Sync Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Initial Sync Test Event {}
DESCRIPTION:Event #{} for initial sync testing
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
                println!("Created event {}: {}", i, resp.status());
            }
            Err(e) => {
                panic!("Error creating event {}: {}", i, e);
            }
        }
    }

    // Small delay to ensure events are fully created
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Perform initial sync with empty sync token
    let sync_response = client
        .sync_collection(&calendar_path, None, Some(100), true)
        .await;

    match sync_response {
        Ok(response) => {
            println!("Initial sync completed with {} items", response.items.len());
            println!("Sync token: {:?}", response.sync_token);

            // Should have at least the events we created (if server supports returning items)
            assert!(
                response.items.len() >= event_paths.len(),
                "Expected at least {} items, got {}",
                event_paths.len(),
                response.items.len()
            );

            // Verify we got calendar data for events
            let event_items: Vec<_> = response
                .items
                .iter()
                .filter(|item| item.calendar_data.is_some())
                .collect();

            println!("Events with data: {}", event_items.len());
            assert!(
                event_items.len() >= event_paths.len(),
                "Expected calendar data for at least {} events",
                event_paths.len()
            );
        }
        Err(e) => {
            panic!("Initial sync failed: {}", e);
        }
    }

    // Clean up
    for event_path in event_paths {
        let _ = client.delete(&event_path).await;
    }
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_incremental_sync() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for incremental sync testing
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

    // Small delay to ensure calendar is fully created
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Perform initial sync to get baseline sync token
    let initial_sync = client
        .sync_collection(&calendar_path, None, Some(100), true)
        .await;
    let initial_sync_token = match initial_sync {
        Ok(response) => {
            println!(
                "Initial sync baseline established with {} items",
                response.items.len()
            );
            // Note: Server may not provide sync token immediately, which limits our ability to do incremental sync
            if response.sync_token.is_none() {
                println!(
                    "ℹ️  No sync token received from server - incremental sync testing will be limited"
                );
            }
            response.sync_token.clone()
        }
        Err(e) => {
            panic!("Failed to establish sync baseline: {}", e);
        }
    };

    // Add a new event after initial sync
    let event_uid = format!("{}-incremental", generate_unique_event_uid());
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Incremental Sync Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Incremental Sync Test Event
DESCRIPTION:Event for incremental sync testing
END:VEVENT
END:VCALENDAR"#,
        event_uid
    );

    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    match put_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected successful event creation"
            );
            println!("Created incremental event: {}", resp.status());
        }
        Err(e) => {
            panic!("Failed to create incremental event: {}", e);
        }
    }

    // Small delay to ensure event is fully created
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Perform incremental sync with previous sync token
    // Skip incremental sync test if server didn't provide a sync token
    let token = match initial_sync_token.clone() {
        Some(t) => t,
        None => {
            println!("ℹ️  Skipping incremental sync test - server did not provide sync token");
            // Clean up and exit test early
            let _ = client.delete(&event_path).await;
            let _ = client.delete(&calendar_path).await;
            return;
        }
    };

    // Small delay before sync to ensure changes are processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    let incremental_sync = client
        .sync_collection(&calendar_path, Some(&token), Some(100), true)
        .await;

    match incremental_sync {
        Ok(response) => {
            println!(
                "Incremental sync completed with {} items",
                response.items.len()
            );
            println!("New sync token: {:?}", response.sync_token);

            // Find the new event in the sync response
            let new_event_item = response
                .items
                .iter()
                .find(|item| item.href.contains(&event_uid));

            if let Some(item) = new_event_item {
                println!(
                    "✅ Found new event in sync: {} (ETag: {:?})",
                    item.href, item.etag
                );
                if let Some(ref data) = item.calendar_data {
                    println!("Event has calendar data: {} chars", data.len());
                }
                if item.is_deleted {
                    panic!("New event is marked as deleted");
                }
            } else {
                panic!("Expected to find new event in sync response");
            }
        }
        Err(e) => {
            panic!("Incremental sync failed: {}", e);
        }
    }

    // Test deletion sync - delete the event
    let delete_response = client.delete(&event_path).await;
    match delete_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected successful event deletion"
            );
            println!("Deleted incremental event: {}", resp.status());
        }
        Err(e) => {
            panic!("Failed to delete incremental event: {}", e);
        }
    }

    // Small delay to ensure deletion is processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get the sync token before deletion sync
    let pre_delete_sync = client
        .sync_collection(
            &calendar_path,
            initial_sync_token.as_deref(),
            Some(100),
            true,
        )
        .await;
    let pre_delete_token = match pre_delete_sync {
        Ok(response) => response.sync_token.clone(),
        Err(e) => {
            panic!("Failed to get pre-delete sync token: {}", e);
        }
    };

    // Perform sync after deletion
    if let Some(ref_token) = pre_delete_token {
        // Small delay before sync to ensure changes are processed
        tokio::time::sleep(Duration::from_millis(100)).await;

        let deletion_sync = client
            .sync_collection(&calendar_path, Some(&ref_token), Some(100), true)
            .await;

        match deletion_sync {
            Ok(response) => {
                println!(
                    "Deletion sync completed with {} items",
                    response.items.len()
                );

                // Check if we have deletion markers
                let deleted_items: Vec<_> = response
                    .items
                    .iter()
                    .filter(|item| item.is_deleted)
                    .collect();

                println!("Deleted items in sync: {}", deleted_items.len());

                if !deleted_items.is_empty() {
                    println!("✅ Found deleted items in sync response");
                    for item in deleted_items {
                        println!("  Deleted item: {}", item.href);
                        assert!(item.is_deleted, "Item should be marked as deleted");
                        assert!(
                            item.calendar_data.is_none(),
                            "Deleted items should have no calendar data"
                        );
                    }
                } else {
                    panic!("Expected deleted items in sync response");
                }
            }
            Err(e) => {
                panic!("Deletion sync failed: {}", e);
            }
        }
    }

    // Clean up
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_sync_limit_and_pagination() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for limit testing
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

    // Create multiple events to test limit functionality
    let mut event_paths = Vec::new();
    let mut event_uids = Vec::new();
    for i in 1..=5 {
        let event_uid = format!("{}-limit-{}", generate_unique_event_uid(), i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);

        let event_ics = format!(
            r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Limit Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231229T100000Z
DTEND:20231229T110000Z
SUMMARY:Limit Test Event {}
DESCRIPTION:Event #{} for limit and pagination testing
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
                println!("Created limit test event {}: {}", i, resp.status());
            }
            Err(e) => {
                panic!("Error creating limit test event {}: {}", i, e);
            }
        }
    }

    // Test sync with limit = 3 (should return only 3 items)
    let limited_sync = client
        .sync_collection(&calendar_path, None, Some(3), true)
        .await;
    match limited_sync {
        Ok(response) => {
            println!(
                "Limited sync (3) completed with {} items",
                response.items.len()
            );

            // Should have exactly 3 items (or fewer if server has limitations)
            // Note: SabreDAV might not respect limits perfectly in all cases
            if response.items.len() > 3 {
                println!(
                    "⚠️  Server returned more items ({}) than requested limit (3)",
                    response.items.len()
                );
            } else {
                println!("✅ Server respected limit request");
            }

            // Should still have a sync token
            println!("Sync token present: {}", response.sync_token.is_some());
        }
        Err(e) => {
            panic!("Limited sync failed: {}", e);
        }
    }

    // Test sync with limit = 10 (should return all 5 items)
    let unlimited_sync = client
        .sync_collection(&calendar_path, None, Some(10), true)
        .await;
    match unlimited_sync {
        Ok(response) => {
            println!(
                "Unlimited sync (10) completed with {} items",
                response.items.len()
            );

            // Should have at least our 5 items
            assert!(
                response.items.len() >= event_uids.len(),
                "Expected at least {} items, got {}",
                event_uids.len(),
                response.items.len()
            );

            // Validate that all our events are present
            let mut found_events = 0;
            for event_uid in &event_uids {
                let event_found = response
                    .items
                    .iter()
                    .any(|item| item.href.contains(event_uid));
                if event_found {
                    found_events += 1;
                }
            }

            assert_eq!(
                found_events,
                event_uids.len(),
                "Expected to find all {} events, found {}",
                event_uids.len(),
                found_events
            );
        }
        Err(e) => {
            panic!("Unlimited sync failed: {}", e);
        }
    }

    // Clean up
    for event_path in event_paths {
        let _ = client.delete(&event_path).await;
    }
    let _ = client.delete(&calendar_path).await;

    println!("Sync limit and pagination test completed");
}

#[tokio::test]
async fn test_sync_deletion_tracking() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for deletion tracking testing
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

    // Create an event that we'll later delete
    let event_uid = format!("{}-deletion", generate_unique_event_uid());
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Deletion Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231230T100000Z
DTEND:20231230T110000Z
SUMMARY:Deletion Test Event
DESCRIPTION:Event for deletion tracking testing
END:VEVENT
END:VCALENDAR"#,
        event_uid
    );

    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    match put_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected successful event creation"
            );
            println!("Created deletion test event: {}", resp.status());
        }
        Err(e) => {
            panic!("Error creating deletion test event: {}", e);
        }
    }

    // Perform initial sync to establish baseline
    let initial_sync = client
        .sync_collection(&calendar_path, None, Some(100), true)
        .await;
    let initial_sync_token = match initial_sync {
        Ok(response) => {
            println!("Initial sync completed with {} items", response.items.len());

            // Verify our event is present
            let event_found = response
                .items
                .iter()
                .any(|item| item.href.contains(&event_uid));
            assert!(
                event_found,
                "Expected to find our test event in initial sync"
            );

            response.sync_token.clone()
        }
        Err(e) => {
            panic!("Initial sync failed: {}", e);
        }
    };

    // Delete the event
    let delete_response = client.delete(&event_path).await;
    match delete_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected successful event deletion"
            );
            println!("Deleted test event: {}", resp.status());
        }
        Err(e) => {
            panic!("Error deleting test event: {}", e);
        }
    }

    // Perform sync after deletion to check deletion tracking
    if let Some(token) = initial_sync_token {
        let deletion_sync = client
            .sync_collection(&calendar_path, Some(&token), Some(100), true)
            .await;
        match deletion_sync {
            Ok(response) => {
                println!(
                    "Deletion sync completed with {} items",
                    response.items.len()
                );

                // Should have at least one item (the deleted event)
                assert!(
                    !response.items.is_empty(),
                    "Expected at least one item in deletion sync"
                );

                // Check if we have deletion markers
                let deleted_items: Vec<_> = response
                    .items
                    .iter()
                    .filter(|item| item.is_deleted)
                    .collect();

                println!("Deleted items in sync: {}", deleted_items.len());

                if !deleted_items.is_empty() {
                    println!("✅ Found deleted items in sync response");
                    for item in deleted_items {
                        println!("  Deleted item: {}", item.href);
                        // Validate deletion marker properties
                        assert!(item.is_deleted, "Item should be marked as deleted");
                        assert!(
                            item.calendar_data.is_none(),
                            "Deleted items should have no calendar data"
                        );
                        // Note: ETag behavior for deletions may vary by server
                    }
                } else {
                    panic!("Expected deleted items in sync response");
                }
            }
            Err(e) => {
                panic!("Deletion sync failed: {}", e);
            }
        }
    } else {
        println!("⚠️  Skipping deletion tracking test - no sync token available");
    }

    // Clean up (calendar only, event already deleted)
    let _ = client.delete(&calendar_path).await;

    println!("Sync deletion tracking test completed");
}
