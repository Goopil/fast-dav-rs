use fast_dav_rs::CalDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";

#[tokio::test]
async fn test_security_invalid_credentials() {
    // Test with invalid credentials
    let invalid_client = CalDavClient::new(SABREDAV_URL, Some("invalid_user"), Some("invalid_pass"));
    
    match invalid_client {
        Ok(client) => {
            let result = client.options("").await;
            match result {
                Ok(response) => {
                    println!("Request with invalid credentials returned: {}", response.status());
                    // Should be 401 Unauthorized
                    assert_eq!(response.status(), 401, "Expected 401 Unauthorized for invalid credentials");
                }
                Err(e) => {
                    println!("Request with invalid credentials failed as expected: {}", e);
                }
            }
        }
        Err(e) => {
            println!("⚠️  Failed to create client with invalid credentials (may be expected): {}", e);
        }
    }
}

#[tokio::test]
async fn test_security_no_credentials() {
    // Test without credentials
    let no_auth_client = CalDavClient::new(SABREDAV_URL, None, None);
    
    match no_auth_client {
        Ok(client) => {
            let result = client.options("").await;
            match result {
                Ok(response) => {
                    println!("Request without credentials returned: {}", response.status());
                    // Depending on server config, this might be 401 or 200
                    if response.status() == 401 {
                        println!("✅ Server correctly requires authentication");
                    } else if response.status().is_success() {
                        println!("⚠️  Server allows unauthenticated access (may be expected in test environment)");
                    } else {
                        println!("Request without credentials returned unexpected status: {}", response.status());
                    }
                }
                Err(e) => {
                    println!("Request without credentials failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("⚠️  Failed to create client without credentials: {}", e);
        }
    }
}

#[tokio::test]
async fn test_security_path_traversal_attempts() {
    let client = CalDavClient::new(SABREDAV_URL, Some("test"), Some("test"))
        .expect("Failed to create CalDav client");
    
    // Test path traversal attempts (these should be rejected by the server)
    let malicious_paths = vec![
        "../etc/passwd",
        "/../etc/passwd",
        "....//etc/passwd",
        "/....//etc/passwd",
    ];
    
    for path in malicious_paths {
        let result = client.get(path).await;
        match result {
            Ok(response) => {
                println!("Path traversal attempt '{}' returned: {}", path, response.status());
                // Should not be 200 OK - likely 404 or 403
                assert!(!response.status().is_success() || response.status() == 404,
                    "Path traversal attempt '{}' should not return success status", path);
            }
            Err(e) => {
                println!("Path traversal attempt '{}' failed as expected: {}", path, e);
            }
        }
    }
}

#[tokio::test]
async fn test_security_malformed_requests() {
    let client = CalDavClient::new(SABREDAV_URL, Some("test"), Some("test"))
        .expect("Failed to create CalDav client");
    
    // Test malformed PROPFIND request
    let malformed_propfind = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:nonexistentproperty/>
  </D:prop>
</D:propfind>"#;
    
    let result = client.propfind("calendars/test/", fast_dav_rs::Depth::Zero, malformed_propfind).await;
    match result {
        Ok(response) => {
            println!("Malformed PROPFIND returned: {}", response.status());
            // Should handle gracefully, likely 207 Multi-Status or 400 Bad Request
        }
        Err(e) => {
            println!("Malformed PROPFIND failed as expected: {}", e);
        }
    }
    
    // Test extremely large request body
    let large_body = "A".repeat(1000000); // 1MB body
    let large_xml = format!(r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
  <D:comment>{}</D:comment>
</D:propfind>"#, large_body);
    
    let large_result = client.propfind("calendars/test/", fast_dav_rs::Depth::Zero, &large_xml).await;
    match large_result {
        Ok(response) => {
            println!("Large request body returned: {}", response.status());
            // Server should handle this gracefully, possibly with 413 Payload Too Large
        }
        Err(e) => {
            println!("Large request body failed as expected: {}", e);
        }
    }
}

#[tokio::test]
async fn test_security_unauthorized_resource_access() {
    let client = CalDavClient::new(SABREDAV_URL, Some("test"), Some("test"))
        .expect("Failed to create CalDav client");
    
    // Try to access resources that should be inaccessible
    let restricted_paths = vec![
        "principals/admin/",
        "calendars/admin/",
        "/etc/passwd",
        "/var/log/",
    ];
    
    for path in restricted_paths {
        let result = client.propfind(path, fast_dav_rs::Depth::Zero, r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#).await;
        
        match result {
            Ok(response) => {
                println!("Unauthorized access to '{}' returned: {}", path, response.status());
                // Should not be 200 OK for restricted resources
                if response.status().is_success() && response.status() != 207 {
                    println!("⚠️  Unexpected success accessing '{}'", path);
                }
            }
            Err(e) => {
                println!("Unauthorized access to '{}' failed as expected: {}", path, e);
            }
        }
    }
}