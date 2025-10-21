/// Test to understand what happens when we get a successful response

use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

#[tokio::test]
async fn test_successful_auth_flow() {
    println!("=== Testing successful authentication flow ===");
    
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDav client");
    
    // First, let's try to get the principal - this should work with our credentials
    println!("Testing OPTIONS request (should challenge for auth)...");
    
    match client.options("").await {
        Ok(resp) => {
            println!("✅ OPTIONS request succeeded!");
            println!("Status: {}", resp.status());
            
            // Print authentication-related headers
            println!("Auth-related headers:");
            for (name, value) in resp.headers() {
                let name_str = name.to_string().to_lowercase();
                if name_str.contains("auth") || name_str.contains("www") {
                    if let Ok(value_str) = value.to_str() {
                        println!("  {}: {}", name, value_str);
                    }
                }
            }
            
            let body = resp.into_body();
            println!("Body length: {}", body.len());
            
            // Check content encoding
            // The error happens during decompression, so let's see what encoding was detected
        }
        Err(e) => {
            println!("❌ OPTIONS request failed: {}", e);
            println!("This might be expected if server requires authentication");
        }
    }
}

#[tokio::test]
async fn test_principal_discovery() {
    println!("\n=== Testing principal discovery ===");
    
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDav client");
    
    // Try to discover the current user principal
    match client.discover_current_user_principal().await {
        Ok(result) => {
            println!("✅ Principal discovery succeeded!");
            println!("Result: {:?}", result);
        }
        Err(e) => {
            println!("❌ Principal discovery failed: {}", e);
            // This is where we might see the compression error
        }
    }
}