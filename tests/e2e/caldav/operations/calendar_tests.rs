use fast_dav_rs::CalDavClient;
use crate::util::unique_calendar_name;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// Helper function to generate unique calendar names
fn generate_unique_calendar_name() -> String {
    unique_calendar_name("test_calendar")
}

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

#[tokio::test]
async fn test_create_calendar() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // Create a new calendar
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:calendar-description>Test calendar created by fast-dav-rs tests</C:calendar-description>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );

    let response = client.mkcalendar(&calendar_path, &calendar_xml).await;
    match response {
        Ok(resp) => {
            println!(
                "MKCALENDAR request succeeded with status: {}",
                resp.status()
            );
            assert!(
                resp.status().is_success(),
                "Expected successful calendar creation, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("MKCALENDAR request failed: {}", e);
        }
    }

    // Verify the calendar was created by doing a PROPFIND
    let verify_response = client
        .propfind(
            &calendar_path,
            fast_dav_rs::Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match verify_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected to find created calendar, got status: {}",
                resp.status()
            );
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
            assert!(
                resp.status().is_success(),
                "Expected successful calendar deletion, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to clean up calendar: {}", e);
        }
    }
}

#[tokio::test]
async fn test_proppatch_operation() {
    let client = create_test_client();
    let calendar_name = generate_unique_calendar_name();
    let calendar_path = format!("calendars/test/{}/", calendar_name);

    // First create a calendar
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
            assert!(
                resp.status().is_success(),
                "Expected successful PROPPATCH operation, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("PROPPATCH request failed: {}", e);
        }
    }

    // Verify the property was updated
    let verify_response = client
        .propfind(
            &calendar_path,
            fast_dav_rs::Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match verify_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected successful verification PROPFIND, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to verify PROPPATCH update: {}", e);
        }
    }

    // Clean up: Delete the calendar
    let cleanup_response = client.delete(&calendar_path).await;
    match cleanup_response {
        Ok(resp) => {
            println!("Cleaned up calendar with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful calendar deletion, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to clean up calendar: {}", e);
        }
    }
}
