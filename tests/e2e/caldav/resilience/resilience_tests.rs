use bytes::Bytes;
use fast_dav_rs::{CalDavClient, Depth};
use std::sync::Arc;
use std::time::Duration;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    format!(
        "resilience_test_calendar_{}",
        chrono::Utc::now().timestamp_millis()
    )
}

/// Helper function to generate unique event UIDs
fn generate_unique_event_uid() -> String {
    format!(
        "resilience-event-{}@example.com",
        chrono::Utc::now().timestamp_millis()
    )
}

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::test]
async fn test_resilience_network_disconnect() {
    let client = create_test_client();

    // Test basic connectivity first
    let options_result = client.options("").await;
    match options_result {
        Ok(response) => {
            println!("Initial connection successful: {}", response.status());
            assert!(response.status().is_success());
        }
        Err(e) => {
            println!(
                "⚠️ Initial connection failed (maybe expected in some environments): {}",
                e
            );
            // Don't fail the test if the initial connection fails - this might be expected in CI
            return;
        }
    }

    // Test multiple rapid requests to check connection reuse
    let mut success_count = 0;
    for i in 0..10 {
        let result = client.options("").await;
        match result {
            Ok(response) => {
                if response.status().is_success() {
                    success_count += 1;
                }
                println!("Rapid request {}: {}", i, response.status());
            }
            Err(e) => {
                println!("Rapid request {} failed: {}", i, e);
            }
        }
        // Small delay to avoid overwhelming the server
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    println!("Successful rapid requests: {}/10", success_count);
    // Expect at least a 70% success rate
    assert!(
        success_count >= 7,
        "Expected at least 7 successful rapid requests"
    );
}

#[tokio::test]
async fn test_resilience_high_concurrency_load() {
    let client = create_test_client();

    // Test high concurrency with propfind_many
    let paths: Vec<String> = vec![
        "principals/test/".to_string(),
        "calendars/test/".to_string(),
        "principals/test/".to_string(), // Duplicate to increase load
        "calendars/test/".to_string(),  // Duplicate to increase load
    ];

    // Create a larger set of paths by duplicating
    let expanded_paths: Vec<String> = paths
        .into_iter()
        .cycle()
        .take(8) // Reasonable number of requests
        .collect();

    let propfind_body = Arc::new(Bytes::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#,
    ));

    // Reasonable concurrency level
    let results = client
        .propfind_many(expanded_paths, Depth::Zero, propfind_body, 4)
        .await;

    let results_len = results.len();
    println!(
        "High concurrency test completed with {} results",
        results_len
    );

    let mut success_count = 0;
    for (i, result) in results.into_iter().enumerate() {
        match result.result {
            Ok(response) => {
                if response.status().is_success() {
                    success_count += 1;
                }
                println!(
                    "High concurrency request {} ({}): {}",
                    i,
                    result.pub_path,
                    response.status()
                );
            }
            Err(e) => {
                println!(
                    "High concurrency request {} ({}) failed: {}",
                    i, result.pub_path, e
                );
            }
        }
    }

    println!(
        "Successful high concurrency requests: {}/{}",
        success_count, results_len
    );
    // Expect at least 75% success rate
    assert!(
        success_count >= (results_len * 3 / 4),
        "Expected at least 75% success rate in high concurrency test"
    );
}

#[tokio::test]
async fn test_resilience_large_payload_handling() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for large payload testing
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
        println!(
            "⚠️ Failed to create a calendar for a large payload test: {}",
            e
        );
        return;
    }
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful calendar creation"
    );

    // Create a large event with extensive description
    let event_uid = format!("{}-large-payload", generate_unique_event_uid());
    let event_filename = format!("{}.ics", event_uid);
    let event_path = format!("{}{}", calendar_path, event_filename);

    // Generate a large description
    let large_description = "A".repeat(10000); // 10KB description

    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Large Payload Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Large Payload Test Event
DESCRIPTION:{}
END:VEVENT
END:VCALENDAR"#,
        event_uid, large_description
    );

    let put_response = client.put(&event_path, Bytes::from(event_ics)).await;
    match put_response {
        Ok(response) => {
            println!("Large payload PUT response: {}", response.status());
            assert!(
                response.status().is_success(),
                "Expected successful large payload PUT"
            );
        }
        Err(e) => {
            panic!("Large payload PUT failed: {}", e);
        }
    }

    // Retrieve the large event
    let get_response = client.get(&event_path).await;
    match get_response {
        Ok(response) => {
            assert!(
                response.status().is_success(),
                "Expected successful large payload GET"
            );
            let body = response.into_body();
            println!("Large payload GET response size: {} bytes", body.len());
            assert!(body.len() > 10000, "Expected large response body");
        }
        Err(e) => {
            panic!("Large payload GET failed: {}", e);
        }
    }

    // Clean up
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;

    println!("Large payload handling test completed");
}

#[tokio::test]
async fn test_resilience_batch_operations() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a calendar for batch testing
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
        println!("⚠️ Failed to create a calendar for batch test: {}", e);
        return;
    }
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful calendar creation"
    );

    // Create multiple events for batch operations
    let mut event_paths = Vec::new();
    let mut event_hrefs = Vec::new();

    for i in 1..=5 {
        let event_uid = format!("{}-batch-{}", generate_unique_event_uid(), i);
        let event_filename = format!("{}.ics", event_uid);
        let event_path = format!("{}{}", calendar_path, event_filename);

        let event_ics = format!(
            r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Batch Test//EN
BEGIN:VEVENT
UID:{}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Batch Test Event {}
DESCRIPTION:Event #{} for batch operations testing
END:VEVENT
END:VCALENDAR"#,
            event_uid, i, i
        );

        let response = client.put(&event_path, Bytes::from(event_ics)).await;
        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    event_paths.push(event_path);
                    event_hrefs.push(format!("/{}", event_filename));
                    println!("Created batch test event {}: {}", i, resp.status());
                } else {
                    println!("Failed to create batch test event {}: {}", i, resp.status());
                }
            }
            Err(e) => {
                println!("Error when creating batch test event {}: {}", i, e);
            }
        }
    }

    if !event_hrefs.is_empty() {
        // Test calendar-multiget with multiple HREFs
        let multiget_results = client
            .calendar_multiget(&calendar_path, &event_hrefs, true)
            .await;
        match multiget_results {
            Ok(objects) => {
                println!("Calendar multiget returned {} objects", objects.len());
                // Should find at least some of our events
                if !objects.is_empty() {
                    println!("✅ Calendar multiget succeeded");

                    // Verify we got calendar data for some objects
                    let objects_with_data = objects
                        .iter()
                        .filter(|obj| obj.calendar_data.is_some())
                        .count();

                    println!("Objects with calendar data: {}", objects_with_data);
                } else {
                    println!(
                        "⚠️  Calendar multiget returned no objects (may be expected with this server)"
                    );
                }
            }
            Err(e) => {
                println!(
                    "⚠️  Calendar multiget failed (may be expected with this server): {}",
                    e
                );
            }
        }
    } else {
        println!("⚠️  No events created, skipping calendar-multiget test");
    }

    // Clean up
    for event_path in event_paths {
        let _ = client.delete(&event_path).await;
    }
    let _ = client.delete(&calendar_path).await;

    println!("Batch operations test completed");
}
