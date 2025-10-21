/// Test to debug Basic auth headers

use fast_dav_rs::CalDavClient;
use hyper::{Method, HeaderMap};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

#[tokio::test]
async fn debug_basic_auth_headers() {
    println!("=== Debugging Basic Auth Headers ===");
    
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDav client");
    
    // Let's manually inspect what headers our client would send
    let mut test_headers = HeaderMap::new();
    test_headers.insert("Depth", "0".parse().unwrap());
    
    println!("Testing manual GET request with our client's auth...");
    
    match client.get("").await {
        Ok(resp) => {
            println!("✅ GET request succeeded!");
            println!("Status: {}", resp.status());
            
            // Check if we get a proper response now
            let body = resp.into_body();
            println!("Body length: {}", body.len());
            
            if body.len() < 500 {
                println!("Body content: {}", String::from_utf8_lossy(&body));
            }
        }
        Err(e) => {
            println!("❌ GET request failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_with_explicit_auth() {
    println!("\n=== Testing with explicit Basic Auth ===");
    
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDav client");
    
    // Test a simple OPTIONS request which should trigger auth
    match client.options("").await {
        Ok(resp) => {
            println!("✅ OPTIONS with auth succeeded!");
            println!("Status: {}", resp.status());
            
            // Check if we're now authenticated
            if resp.status().is_success() {
                println!("🎉 Successfully authenticated!");
            }
            
            let body = resp.into_body();
            println!("Body length: {}", body.len());
        }
        Err(e) => {
            println!("❌ OPTIONS with auth failed: {}", e);
        }
    }
}