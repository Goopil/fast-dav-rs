use fast_dav_rs::{CalDavClient, ContentEncoding};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    client
}

#[tokio::test]
async fn test_compression_support() {
    let client = create_test_client();
    
    // Test with different compression encodings
    let encodings = vec![
        ContentEncoding::Identity,
        ContentEncoding::Gzip,
        ContentEncoding::Br,
        ContentEncoding::Zstd,
    ];
    
    for encoding in encodings {
        let mut client_with_encoding = client.clone();
        client_with_encoding.set_request_compression(encoding);
        
        // Test a simple GET request
        let response = client_with_encoding.get("").await;
        match response {
            Ok(resp) => {
                println!("Request with {:?} compression succeeded with status {:?}", 
                         encoding, resp.status());
                assert!(resp.status().is_success(), "Expected successful request with {:?} compression, got status: {}", encoding, resp.status());
            }
            Err(e) => {
                panic!("Request with {:?} compression failed: {:?}", encoding, e);
            }
        }
    }
}

// Test that demonstrates the client can handle compressed responses
#[tokio::test]
async fn test_compressed_response_handling() {
    let client = create_test_client();
    
    // This test verifies that our client can handle compressed responses
    let response = client.propfind("principals/test/", fast_dav_rs::Depth::Zero, 
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await;
    
    match response {
        Ok(resp) => {
            println!("Compressed response test succeeded with status: {}", resp.status());
            assert!(resp.status().is_success(), "Expected successful compressed response handling, got status: {}", resp.status());
        }
        Err(e) => {
            panic!("Compressed response test encountered error: {}", e);
        }
    }
}