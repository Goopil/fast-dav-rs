use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    client
}

#[tokio::test]
async fn test_propfind_principals() {
    let client = create_test_client();
    
    // Try a PROPFIND on the principals path
    let response = client.propfind("principals/", fast_dav_rs::Depth::One, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("PROPFIND on principals succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful PROPFIND on principals");
            let body = resp.into_body();
            println!("Response body length: {}", body.len());
            assert!(body.len() > 0, "Expected non-empty response body");
        }
        Err(e) => {
            panic!("PROPFIND on principals failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_propfind_user_principal() {
    let client = create_test_client();
    
    // Try a PROPFIND on the user principal
    let response = client.propfind("principals/test/", fast_dav_rs::Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("PROPFIND on user principal succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful PROPFIND on user principal");
            let body = resp.into_body();
            println!("Response body length: {}", body.len());
            assert!(body.len() > 0, "Expected non-empty response body");
        }
        Err(e) => {
            panic!("PROPFIND on user principal failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_propfind_user_calendars() {
    let client = create_test_client();
    
    // Try a PROPFIND on the user calendars path
    let response = client.propfind("calendars/test/", fast_dav_rs::Depth::One, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
    <C:calendar-description/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("PROPFIND on user calendars succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful PROPFIND on user calendars");
            let body = resp.into_body();
            println!("Response body length: {}", body.len());
            assert!(body.len() > 0, "Expected non-empty response body");
        }
        Err(e) => {
            panic!("PROPFIND on user calendars failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_discovery_operations() {
    let client = create_test_client();
    
    // Test discovering current user principal
    let principal_result = client.discover_current_user_principal().await;
    match principal_result {
        Ok(Some(principal)) => {
            println!("Discovered user principal: {}", principal);
            assert!(!principal.is_empty(), "Expected non-empty principal URL");
        }
        Ok(None) => {
            panic!("No user principal found");
        }
        Err(e) => {
            panic!("Failed to discover user principal: {}", e);
        }
    }
    
    // Test discovering calendar home set
    let home_sets_result = client.discover_calendar_home_set("principals/test/").await;
    match home_sets_result {
        Ok(home_sets) => {
            println!("Discovered {} calendar home sets", home_sets.len());
            assert!(!home_sets.is_empty(), "Expected at least one calendar home set");
            for home_set in home_sets {
                println!("  Home set: {}", home_set);
                assert!(!home_set.is_empty(), "Expected non-empty home set URL");
            }
        }
        Err(e) => {
            panic!("Failed to discover calendar home sets: {}", e);
        }
    }
    
    // Test listing calendars
    let calendars_result = client.list_calendars("calendars/test/").await;
    match calendars_result {
        Ok(calendars) => {
            println!("Found {} calendars", calendars.len());
            assert!(!calendars.is_empty(), "Expected at least one calendar");
            for calendar in calendars {
                println!("  Calendar: {:?}", calendar.displayname);
                assert!(calendar.displayname.is_some(), "Expected calendar to have a display name");
            }
        }
        Err(e) => {
            panic!("Failed to list calendars: {}", e);
        }
    }
}