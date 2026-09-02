use fast_dav_rs::{WebDavClient, discover_caldav, discover_carddav};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

/// RFC 6764 §5 discovery against the live fixture. SabreDAV has no
/// well-known handler (empirically verified: a `PROPFIND` on
/// `/.well-known/caldav` answers `404`, no redirect, no nginx intercept), so
/// per RFC 6764 §6 the documented fallback applies: the base URL is returned
/// unchanged.
#[tokio::test]
async fn test_well_known_caldav_404_falls_back_to_base_url() {
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let service_url = discover_caldav(&client)
        .await
        .expect("discover_caldav must not fail on a 404 (fallback to base URL)");
    assert_eq!(
        service_url, SABREDAV_URL,
        "A 404 on .well-known/caldav must fall back to the base URL unchanged"
    );
}

/// Same RFC 6764 §6 fallback for the CardDAV service: the fixture answers
/// `404` on `/.well-known/carddav`, so the base URL is returned unchanged.
#[tokio::test]
async fn test_well_known_carddav_404_falls_back_to_base_url() {
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let service_url = discover_carddav(&client)
        .await
        .expect("discover_carddav must not fail on a 404 (fallback to base URL)");
    assert_eq!(
        service_url, SABREDAV_URL,
        "A 404 on .well-known/carddav must fall back to the base URL unchanged"
    );
}
