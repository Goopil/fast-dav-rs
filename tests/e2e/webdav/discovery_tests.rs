use fast_dav_rs::{WebDavClient, discover_caldav, discover_carddav};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// RFC 6764 §5 discovery against the live fixture, full redirect path:
/// the SabreDAV server (index.php `beforeMethod:*` handler) answers
/// `301 /principals/test/` on `/.well-known/caldav`. Same origin, so the
/// client keeps `Authorization`, re-issues the PROPFIND there (`207` with
/// `current-user-principal`) and `discover_caldav` returns the **final
/// request URI**.
#[tokio::test]
async fn test_discover_caldav_follows_well_known_redirect() {
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let service_url = discover_caldav(&client)
        .await
        .expect("discovery must follow the well-known redirect and succeed");
    assert_eq!(
        service_url, "http://localhost:8080/principals/test/",
        "discover_caldav must return the final (post-redirect) request URI"
    );
}

/// Same RFC 6764 §5 redirect path for the CardDAV service: `301` on
/// `/.well-known/carddav` → PROPFIND on the user principal → final URI.
#[tokio::test]
async fn test_discover_carddav_follows_well_known_redirect() {
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let service_url = discover_carddav(&client)
        .await
        .expect("discovery must follow the well-known redirect and succeed");
    assert_eq!(
        service_url, "http://localhost:8080/principals/test/",
        "discover_carddav must return the final (post-redirect) request URI"
    );
}
