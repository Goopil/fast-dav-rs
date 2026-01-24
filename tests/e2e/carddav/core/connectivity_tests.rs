use fast_dav_rs::CardDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CardDavClient {
    CardDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CardDAV client")
}

#[tokio::test]
async fn test_basic_connectivity() {
    let client = create_test_client();

    let response = client.get("").await;
    match response {
        Ok(resp) => {
            println!("GET request succeeded with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful GET request"
            );
        }
        Err(e) => {
            panic!("GET request failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_http_methods() {
    let client = create_test_client();

    let response = client.options("").await;
    match response {
        Ok(resp) => {
            println!("OPTIONS request succeeded with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful OPTIONS request"
            );
        }
        Err(e) => {
            panic!("OPTIONS request failed: {}", e);
        }
    }

    let response = client.head("").await;
    match response {
        Ok(resp) => {
            println!("HEAD request succeeded with status: {}", resp.status());
            assert!(
                resp.status().is_success() || resp.status().is_client_error(),
                "Expected successful HEAD request or client error, got: {}",
                resp.status()
            );
        }
        Err(e) => {
            println!("HEAD request failed (may be expected): {}", e);
        }
    }
}
