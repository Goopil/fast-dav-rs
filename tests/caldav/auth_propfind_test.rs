use fast_dav_rs::CalDavClient;

/// Test authentication with PROPFIND method

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

#[tokio::test]
async fn test_authentication_with_propfind() {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    // Test PROPFIND request to root
    let xml_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;
    
    match client.propfind("", fast_dav_rs::Depth::Zero, xml_body).await {
        Ok(resp) => {
            println!("PROPFIND Authentication SUCCESS!");
            println!("Status: {}", resp.status());
            let body = resp.into_body();
            println!("Body length: {}", body.len());
            if body.len() < 500 {
                println!("Body: {}", String::from_utf8_lossy(&body));
            }
        }
        Err(e) => {
            println!("PROPFIND Authentication FAILED: {}", e);
        }
    }
}