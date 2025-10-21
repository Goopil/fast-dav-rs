use fast_dav_rs::CalDavClient;

/// Debug tests to understand the correct paths and authentication

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    let client = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");

    client
}

#[tokio::test]
async fn debug_paths() {
    let client = create_test_client();

    // Test different paths to see what works
    let paths = vec![
        "",
        "/",
        "principals/",
        "principals/test/",
        "calendars/",
        "calendars/test/",
        "calendars/test/default/",
    ];

    for path in paths {
        println!("\n--- Testing path: '{}' ---", path);

        // Try a simple GET first
        match client.get(path).await {
            Ok(resp) => {
                println!("GET {} -> Status: {}", path, resp.status());
            }
            Err(e) => {
                println!("GET {} -> Error: {}", path, e);
            }
        }

        // Try OPTIONS to see supported methods
        match client.options(path).await {
            Ok(resp) => {
                println!("OPTIONS {} -> Status: {}", path, resp.status());
                if let Some(allow_header) = resp.headers().get("allow") {
                    if let Ok(allow_value) = allow_header.to_str() {
                        println!("  Supported methods: {}", allow_value);
                    }
                }
            }
            Err(e) => {
                println!("OPTIONS {} -> Error: {}", path, e);
            }
        }
    }
}

#[tokio::test]
async fn debug_auth_directly() {
    // Test if we can authenticate at all
    let client = create_test_client();

    // Try a simple authenticated request to see if auth works
    match client.head("").await {
        Ok(resp) => {
            println!(
                "HEAD / -> Status: {}, Headers: {:?}",
                resp.status(),
                resp.headers().len()
            );
        }
        Err(e) => {
            println!("HEAD / -> Error: {}", e);
        }
    }
}
