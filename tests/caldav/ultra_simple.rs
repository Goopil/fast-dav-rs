/// Ultra-simple test using raw hyper to see what we get

use hyper::{Method, Request};
use hyper_util::client::legacy::Client as LegacyClient;
use hyper_util::rt::TokioExecutor;
use http_body_util::{Empty, BodyExt};
use bytes::Bytes;

#[tokio::test]
async fn ultra_simple_test() {
    // Create a simple HTTP client
    let client = LegacyClient::builder(TokioExecutor::new()).build_http();
    
    // Test a simple request
    println!("Making ultra simple request...");
    
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("http://localhost:8080/principals/test/")
        .header("Authorization", "Basic dGVzdDp0ZXN0") // test:test in base64
        .body(Empty::<Bytes>::new())
        .unwrap();
    
    match client.request(req).await {
        Ok(resp) => {
            println!("Ultra simple request SUCCESS!");
            println!("Status: {}", resp.status());
            println!("Headers:");
            for (name, value) in resp.headers() {
                if let Ok(value_str) = value.to_str() {
                    println!("  {}: {}", name, value_str);
                } else {
                    println!("  {}: (binary)", name);
                }
            }
            
            // Try to read the body
            let body = BodyExt::collect(resp.into_body()).await;
            match body {
                Ok(collected) => {
                    let bytes = collected.to_bytes();
                    println!("Body length: {}", bytes.len());
                    if bytes.len() > 0 {
                        println!("Body (first 200 chars): {:?}", 
                                 String::from_utf8_lossy(&bytes[..std::cmp::min(200, bytes.len())]));
                    }
                }
                Err(e) => {
                    println!("Failed to read body: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Ultra simple request FAILED: {}", e);
        }
    }
}