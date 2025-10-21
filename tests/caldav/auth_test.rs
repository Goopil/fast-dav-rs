use fast_dav_rs::CalDavClient;

/// Test authentication with correct credentials

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

#[tokio::test]
async fn test_authentication_with_correct_credentials() {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    // Test a simple authenticated request
    match client.options("").await {
        Ok(resp) => {
            println!("Authentication SUCCESS!");
            println!("Status: {}", resp.status());
            println!("Headers:");
            for (name, value) in resp.headers() {
                if let Ok(value_str) = value.to_str() {
                    println!("  {}: {}", name, value_str);
                }
            }
        }
        Err(e) => {
            println!("Authentication FAILED: {}", e);
        }
    }
}

#[tokio::test]
async fn test_authentication_with_wrong_credentials() {
    let client = CalDavClient::new(SABREDAV_URL, Some("test"), Some("wrongpassword"))
        .expect("Failed to create CalDAV client");
    
    // Test a simple authenticated request
    match client.options("").await {
        Ok(resp) => {
            println!("Wrong password test - Unexpected SUCCESS!");
            println!("Status: {}", resp.status());
        }
        Err(e) => {
            println!("Wrong password test - Expected failure: {}", e);
        }
    }
}