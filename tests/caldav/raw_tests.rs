use fast_dav_rs::{CalDavClient};
use hyper::{HeaderMap};

/// Raw tests to see what data we're actually getting

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    
    client
}

#[tokio::test]
async fn test_raw_response() {
    let client = create_test_client();
    
    // Test with streaming to see raw response
    let mut headers = HeaderMap::new();
    // Don't add Accept-Encoding to avoid compression
    headers.remove(hyper::header::ACCEPT_ENCODING);
    
    // Try a simple request to see what we get
    println!("Testing raw response...");
    
    // Let's try to access the streaming send method directly if we can
    // For now, let's just test what we know works
    match client.options("principals/test/").await {
        Ok(resp) => {
            println!("OPTIONS Success! Status: {}", resp.status());
            println!("Headers:");
            for (name, value) in resp.headers() {
                if let Ok(value_str) = value.to_str() {
                    println!("  {}: {}", name, value_str);
                } else {
                    println!("  {}: (binary)", name);
                }
            }
        }
        Err(e) => {
            println!("OPTIONS Error: {}", e);
        }
    }
}

#[tokio::test]
async fn test_simple_get() {
    let client = create_test_client();
    
    // Try a GET request that we know gets a 200 from Nginx logs
    match client.get("calendars/test/").await {
        Ok(resp) => {
            println!("GET Success! Status: {}", resp.status());
            let body = resp.into_body();
            println!("Body length: {}", body.len());
            if body.len() < 1000 {
                println!("Body (first 100 chars): {:?}", 
                         String::from_utf8_lossy(&body[..std::cmp::min(100, body.len())]));
            }
        }
        Err(e) => {
            println!("GET Error: {}", e);
        }
    }
}