/// Test to understand the compression issue by examining what happens in our client

use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

#[tokio::test]
async fn debug_compression_issue() {
    println!("=== Debugging compression issue ===");
    
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDav client");
    
    // Test a simple request
    let xml_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#;
    
    println!("Sending PROPFIND request...");
    
    match client.propfind("principals/test/", fast_dav_rs::Depth::Zero, xml_body).await {
        Ok(resp) => {
            println!("✅ Request succeeded!");
            println!("Status: {}", resp.status());
            
            // Print all headers
            println!("Headers:");
            for (name, value) in resp.headers() {
                if let Ok(value_str) = value.to_str() {
                    println!("  {}: {}", name, value_str);
                } else {
                    println!("  {}: (binary value)", name);
                }
            }
            
            let body = resp.into_body();
            println!("Body length: {}", body.len());
            
            // Check if we have content-encoding
            // The error happens during decompression, so let's see what encoding was detected
            
        }
        Err(e) => {
            println!("❌ Request failed with error: {}", e);
            println!("This is the 'Unknown frame descriptor' error we're trying to fix");
        }
    }
}

#[tokio::test]
async fn test_manual_decompression() {
    println!("\n=== Testing manual decompression ===");
    
    // Let's see what the server actually sends by bypassing our auto-decompression
    // We'll use the streaming version that doesn't auto-decompress
    
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDav client");
    
    let xml_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#;
    
    // Use the streaming version to get raw response
    let mut headers = hyper::HeaderMap::new();
    headers.insert("Depth", "0".parse().unwrap());
    headers.insert("Content-Type", "application/xml; charset=utf-8".parse().unwrap());
    
    match client.send_stream(
        hyper::Method::from_bytes(b"PROPFIND").unwrap(),
        "principals/test/",
        headers,
        Some(bytes::Bytes::from(xml_body)),
        None
    ).await {
        Ok(resp) => {
            println!("✅ Streaming request succeeded!");
            println!("Status: {}", resp.status());
            
            // Check headers
            println!("Headers:");
            for (name, value) in resp.headers() {
                if let Ok(value_str) = value.to_str() {
                    println!("  {}: {}", name, value_str);
                } else {
                    println!("  {}: (binary value)", name);
                }
            }
            
            // Check content encoding
            let content_encoding = resp.headers().get("content-encoding")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("none");
            println!("Content-Encoding detected: {}", content_encoding);
            
            // Now try to manually decompress what we get
            println!("Attempting manual inspection of body...");
            
        }
        Err(e) => {
            println!("❌ Streaming request failed: {}", e);
        }
    }
}