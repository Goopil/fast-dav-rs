use fast_dav_rs::{CalDavClient, ContentEncoding};

/// Tests with disabled compression to isolate the issue

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client_no_compression() -> CalDavClient {
    let mut client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    // Disable request compression
    client.set_request_compression(ContentEncoding::Identity);
    
    client
}

#[tokio::test]
async fn test_with_no_compression() {
    let client = create_test_client_no_compression();
    
    // Test a path that was working but giving compression errors
    println!("Testing principals/test/ with no compression...");
    match client.propfind("principals/test/", fast_dav_rs::Depth::Zero, 
        r#"&lt;?xml version="1.0" encoding="utf-8"?&gt;
&lt;D:propfind xmlns:D="DAV:"&gt;
  &lt;D:prop&gt;
    &lt;D:displayname/&gt;
  &lt;/D:prop&gt;
&lt;/D:propfind&gt;"#).await {
        Ok(resp) => {
            println!("Success! Status: {}, Body length: {}", resp.status(), resp.into_body().len());
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
    
    // Test a GET request to a specific path
    println!("Testing GET on calendars/test/ with no compression...");
    match client.get("calendars/test/").await {
        Ok(resp) => {
            println!("Success! Status: {}, Body length: {}", resp.status(), resp.into_body().len());
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}

#[tokio::test]
async fn test_check_server_compression() {
    let client = create_test_client_no_compression();
    
    // Check what compression the server offers
    match client.options("principals/test/").await {
        Ok(resp) => {
            println!("OPTIONS principals/test/ Status: {}", resp.status());
            if let Some(encoding) = resp.headers().get("content-encoding") {
                println!("Content-Encoding: {:?}", encoding);
            }
            if let Some(encoding) = resp.headers().get("content-type") {
                println!("Content-Type: {:?}", encoding);
            }
        }
        Err(e) => {
            println!("OPTIONS error: {}", e);
        }
    }
}