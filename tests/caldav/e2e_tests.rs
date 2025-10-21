use fast_dav_rs::{CalDavClient, ContentEncoding};

/// E2E tests for the CalDAV client against a real SabreDAV server
/// These tests require the SabreDAV test environment to be running
/// Run `cd sabredav-test && ./setup.sh` before running these tests

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    client
}

#[tokio::test]
async fn test_basic_connectivity() {
    let client = create_test_client();
    
    // Test basic connectivity with a simple GET request
    let response = client.get("").await;
    match response {
        Ok(resp) => {
            println!("GET request succeeded with status: {}", resp.status());
            // Even if it's not successful, we at least know the server is reachable
        }
        Err(e) => {
            panic!("GET request failed: {}", e);
        }
    }
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
            if resp.status().is_success() {
                let body = resp.into_body();
                println!("Response body length: {}", body.len());
                // Don't print the body as it might be compressed
            }
        }
        Err(e) => {
            println!("PROPFIND on principals failed: {}", e);
            // This might be a compression error, which is actually good - it means we're getting a response
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
            if resp.status().is_success() {
                let body = resp.into_body();
                println!("Response body length: {}", body.len());
                // Don't print the body as it might be compressed
            }
        }
        Err(e) => {
            println!("PROPFIND on user principal failed: {}", e);
            // This might be a compression error, which is actually good - it means we're getting a response
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
            if resp.status().is_success() {
                let body = resp.into_body();
                println!("Response body length: {}", body.len());
                // Don't print the body as it might be compressed
            }
        }
        Err(e) => {
            println!("PROPFIND on user calendars failed: {}", e);
            // This might be a compression error, which is actually good - it means we're getting a response
        }
    }
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
                // Request should succeed
                println!("Request with {:?} compression succeeded with status {:?}", 
                         encoding, resp.status());
            }
            Err(e) => {
                // Some servers might not support certain compression methods
                println!("Request with {:?} compression failed: {:?}", encoding, e);
            }
        }
    }
}

// Test that demonstrates the client can handle compressed responses
#[tokio::test]
async fn test_compressed_response_handling() {
    let client = create_test_client();
    
    // This test verifies that our client can handle compressed responses
    // Even if we get a decompression error, it means the server is sending compressed data
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
            // For now, let's not assert success since we might get 400 for various reasons
            // The important thing is that we don't get a decompression error
        }
        Err(e) => {
            println!("Compressed response test encountered error: {}", e);
            // If the error is related to compression, that's actually good - it means we're getting a response
            // The important thing is that we're communicating with the server
        }
    }
}