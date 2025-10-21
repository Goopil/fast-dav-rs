use fast_dav_rs::CalDavClient;
use hyper::{Method, HeaderMap};
use bytes::Bytes;

/// Diagnostic test using our own client to understand compression issues

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

#[tokio::test]
async fn test_compression_detection() {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    println!("=== Testing compression detection ===");
    
    // Test a simple PROPFIND request
    let xml_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#;
    
    match client.propfind("principals/test/", fast_dav_rs::Depth::Zero, xml_body).await {
        Ok(resp) => {
            println!("PROPFIND SUCCESS!");
            println!("Status: {}", resp.status());
            println!("Headers:");
            for (name, value) in resp.headers() {
                if let Ok(value_str) = value.to_str() {
                    println!("  {}: {}", name, value_str);
                }
            }
            
            let body = resp.into_body();
            println!("Body length: {}", body.len());
            println!("First 50 bytes (hex): {:?}", &body[..std::cmp::min(50, body.len())]);
            
            // Try to detect compression
            if body.len() > 4 {
                let magic = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
                println!("Magic number: 0x{:08x}", magic);
                
                // zstd magic number is 0xFD2FB528
                if magic == 0xFD2FB528 {
                    println!("✅ This appears to be zstd compressed data");
                }
                // gzip magic number is 0x00088B1F  
                else if body.len() > 2 && body[0] == 0x1F && body[1] == 0x8B {
                    println!("✅ This appears to be gzip compressed data");
                }
                // Check for Brotli (no fixed magic, but often starts with small values)
                else if body.len() > 0 && body[0] < 10 {
                    println!("❓ Might be brotli compressed data");
                }
                else {
                    println!("❓ Appears to be uncompressed data");
                    if body.len() < 500 {
                        println!("Body content: {}", String::from_utf8_lossy(&body));
                    }
                }
            }
        }
        Err(e) => {
            println!("PROPFIND FAILED: {}", e);
        }
    }
}

#[tokio::test]
async fn test_disable_client_compression() {
    let mut client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    // Disable request compression
    client.set_request_compression(fast_dav_rs::ContentEncoding::Identity);
    
    println!("\n=== Testing with client compression disabled ===");
    
    // Test a simple PROPFIND request
    let xml_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#;
    
    match client.propfind("principals/test/", fast_dav_rs::Depth::Zero, xml_body).await {
        Ok(resp) => {
            println!("PROPFIND with disabled compression SUCCESS!");
            println!("Status: {}", resp.status());
            
            if let Some(encoding) = resp.headers().get("content-encoding") {
                println!("Content-Encoding: {:?}", encoding);
            } else {
                println!("No Content-Encoding header");
            }
            
            let body = resp.into_body();
            println!("Body length: {}", body.len());
            
            if body.len() < 500 && body.len() > 0 {
                println!("Body content: {}", String::from_utf8_lossy(&body));
            }
        }
        Err(e) => {
            println!("PROPFIND with disabled compression FAILED: {}", e);
        }
    }
}