use crate::util::{unique_calendar_name, unique_uid};
use bytes::Bytes;
use fast_dav_rs::{CalDavClient, Depth};
use std::sync::Arc;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    unique_calendar_name("parallel_test_calendar")
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    unique_uid("parallel-event")
}

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::test]
async fn test_propfind_many() {
    let client = create_test_client();

    // Test parallel PROPFIND operations
    let paths: Vec<String> = vec![
        "principals/test/".to_string(),
        "calendars/test/".to_string(),
    ];

    let propfind_body = Arc::new(Bytes::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#,
    ));

    let results = client
        .propfind_many(paths, Depth::Zero, propfind_body, 2)
        .await;

    println!("PROPFIND many completed with {} results", results.len());
    assert_eq!(results.len(), 2, "Expected 2 results from propfind_many");

    for (i, result) in results.into_iter().enumerate() {
        match result.result {
            Ok(response) => {
                println!(
                    "PROPFIND {} ({}): {}",
                    i,
                    result.pub_path,
                    response.status()
                );
                assert!(
                    response.status().is_success(),
                    "Expected successful PROPFIND for {}",
                    result.pub_path
                );
            }
            Err(e) => {
                panic!("PROPFIND {} ({}) failed: {}", i, result.pub_path, e);
            }
        }
    }
}

#[tokio::test]
async fn test_report_many() {
    let client = create_test_client();

    // Create test calendars first
    let mut calendar_paths = Vec::new();
    let mut cleanup_paths = Vec::new();

    for i in 1..=3 {
        let calendar_name = format!("{}_{}", generate_unique_calendar_name(), i);
        let calendar_path = format!("calendars/test/{}/", calendar_name);

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

        let response = client.mkcalendar(&calendar_path, &calendar_xml).await;
        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    calendar_paths.push(calendar_path.clone());
                    cleanup_paths.push(calendar_path);
                    println!("Created calendar {}: {}", i, resp.status());
                } else {
                    println!("Failed to create calendar {}: {}", i, resp.status());
                }
            }
            Err(e) => {
                println!("Error creating calendar {}: {}", i, e);
            }
        }
    }

    if !calendar_paths.is_empty() {
        // Test parallel REPORT operations
        let report_body = Arc::new(Bytes::from(
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:sync-collection xmlns:D="DAV:">
  <D:sync-token/>
  <D:sync-level>1</D:sync-level>
  <D:prop>
    <D:getetag/>
  </D:prop>
</D:sync-collection>"#,
        ));

        let results = client
            .report_many(calendar_paths, Depth::Zero, report_body, 3)
            .await;

        println!("REPORT many completed with {} results", results.len());

        for (i, result) in results.into_iter().enumerate() {
            match result.result {
                Ok(response) => {
                    println!("REPORT {} ({}): {}", i, result.pub_path, response.status());
                    // Reports may succeed or fail depending on server support
                    assert!(
                        response.status().is_success() || response.status().is_client_error(),
                        "Expected successful or client error REPORT for {}",
                        result.pub_path
                    );
                }
                Err(e) => {
                    println!(
                        "REPORT {} ({}) failed (may be expected): {}",
                        i, result.pub_path, e
                    );
                }
            }
        }
    }

    // Clean up test calendars
    for calendar_path in cleanup_paths {
        let _ = client.delete(&calendar_path).await;
    }
}

#[tokio::test]
async fn test_parallel_event_creation() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create calendar for parallel events
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

    // Create multiple events in parallel using manual spawning
    let mut tasks = Vec::new();
    let mut event_paths = Vec::new();

    for i in 1..=5 {
        let client_clone = client.clone();
        let calendar_path_clone = calendar_path.clone();
        let event_uid = format!("{}-parallel-{}", generate_unique_event_uid(), i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path_clone, event_filename);

        event_paths.push(event_path.clone());

        let event_ics = format!(
            r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Parallel Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Parallel Test Event {}
DESCRIPTION:Parallel event #{} created during testing
END:VEVENT
END:VCALENDAR"#,
            event_uid, i, i
        );

        let task =
            tokio::spawn(
                async move { client_clone.put(&event_path, Bytes::from(event_ics)).await },
            );

        tasks.push(task);
    }

    // Wait for all parallel creations
    let results = futures::future::join_all(tasks).await;

    let mut success_count = 0;
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    success_count += 1;
                    println!("Parallel event creation #{}: {}", i + 1, response.status());
                } else {
                    println!(
                        "Parallel event creation #{} failed: {}",
                        i + 1,
                        response.status()
                    );
                }
            }
            Ok(Err(e)) => {
                println!("Parallel event creation #{} error: {}", i + 1, e);
            }
            Err(e) => {
                println!("Parallel event creation #{} panic: {}", i + 1, e);
            }
        }
    }

    println!(
        "Successfully created {}/5 events in parallel",
        success_count
    );
    assert!(
        success_count >= 3,
        "Expected at least 3 successful parallel creations"
    );

    // Clean up events and calendar
    for event_path in event_paths {
        let _ = client.delete(&event_path).await;
    }
    let _ = client.delete(&calendar_path).await;
}
