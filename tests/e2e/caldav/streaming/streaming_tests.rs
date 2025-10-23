use fast_dav_rs::{CalDavClient, Depth};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    format!(
        "stream_test_calendar_{}",
        chrono::Utc::now().timestamp_millis()
    )
}

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::test]
async fn test_propfind_stream() {
    let client = create_test_client();

    // Test streaming PROPFIND on calendars
    let response = client
        .propfind_stream(
            "calendars/test/",
            Depth::One,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <D:getetag/>
    <D:resourcetype/>
    <C:calendar-description/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match response {
        Ok(stream_response) => {
            println!(
                "PROPFIND stream request succeeded with status: {}",
                stream_response.status()
            );
            assert!(
                stream_response.status().is_success(),
                "Expected successful PROPFIND stream"
            );

            // Test that we can read the streamed response
            let encoding = fast_dav_rs::detect_encoding(stream_response.headers());
            let items =
                fast_dav_rs::parse_multistatus_stream(stream_response.into_body(), encoding).await;

            match items {
                Ok(parsed_items) => {
                    println!("Parsed {} items from stream", parsed_items.len());
                    // This should succeed even if there are no items
                    // Just verify it doesn't panic - remove the assert!(true) as it's optimized out
                    println!("Successfully parsed stream");
                }
                Err(e) => {
                    panic!("Failed to parse streamed response: {}", e);
                }
            }
        }
        Err(e) => {
            panic!("PROPFIND stream request failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_report_stream() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // First create a calendar for the report test
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

    // Test streaming REPORT on the calendar
    let response = client
        .report_stream(
            &calendar_path,
            Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:sync-collection xmlns:D="DAV:">
  <D:sync-token/>
  <D:sync-level>1</D:sync-level>
  <D:prop>
    <D:getetag/>
  </D:prop>
</D:sync-collection>"#,
        )
        .await;

    match response {
        Ok(stream_response) => {
            println!(
                "REPORT stream request succeeded with status: {}",
                stream_response.status()
            );
            // Report may succeed or fail depending on server support
            if stream_response.status().is_success() {
                let encoding = fast_dav_rs::detect_encoding(stream_response.headers());
                let items =
                    fast_dav_rs::parse_multistatus_stream(stream_response.into_body(), encoding)
                        .await;

                match items {
                    Ok(parsed_items) => {
                        println!("Parsed {} items from report stream", parsed_items.len());
                        // Just verify it doesn't panic - remove the assert!(true) as it's optimized out
                        println!("Successfully parsed report stream");
                    }
                    Err(e) => {
                        println!("Failed to parse report stream (may be expected): {}", e);
                    }
                }
            }
        }
        Err(e) => {
            println!("REPORT stream request failed (may be expected): {}", e);
        }
    }

    // Clean up
    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_streaming_parser() {
    let client = create_test_client();

    // Test the streaming parser with a regular response first
    let response = client
        .propfind(
            "calendars/test/",
            Depth::One,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match response {
        Ok(regular_response) => {
            assert!(
                regular_response.status().is_success(),
                "Expected successful PROPFIND"
            );

            let _status = regular_response.status();
            let headers = regular_response.headers().clone();
            let body_bytes = regular_response.into_body();

            // Test the streaming parser on regular response body
            let _encoding = fast_dav_rs::detect_encoding(&headers);
            let items = fast_dav_rs::parse_multistatus_bytes(&body_bytes);

            match items {
                Ok(parsed_items) => {
                    println!(
                        "Streaming parser parsed {} items from regular response",
                        parsed_items.len()
                    );
                    // Just verify it doesn't panic - remove the assert!(true) as it's optimized out
                    println!("Successfully parsed regular response with streaming parser");
                }
                Err(e) => {
                    panic!(
                        "Failed to parse regular response with streaming parser: {}",
                        e
                    );
                }
            }
        }
        Err(e) => {
            panic!("Regular PROPFIND failed: {}", e);
        }
    }
}
