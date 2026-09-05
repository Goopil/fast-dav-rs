use fast_dav_rs::webdav::DavCompliance;
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

/// Typed `OPTIONS` compliance probe (RFC 4918 §10.1) against the live
/// fixture: SabreDAV 4.x advertises `DAV: 1, 3, extended-mkcol,
/// access-control, calendarserver-principal-property-search, calendar-access,
/// calendar-proxy, addressbook, 2` — every named class plus one
/// `calendarserver-*` vendor token that must pass through as
/// [`DavCompliance::Other`].
#[tokio::test]
async fn test_options_dav_compliance_probe_is_typed() {
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let caps = client
        .capabilities("/")
        .await
        .expect("OPTIONS compliance probe must succeed");
    let classes = caps.compliance();

    for expected in [
        DavCompliance::One,
        DavCompliance::Two,
        DavCompliance::Three,
        DavCompliance::ExtendedMkcol,
        DavCompliance::AccessControl,
        DavCompliance::CalendarAccess,
        DavCompliance::CalendarProxy,
        DavCompliance::Addressbook,
    ] {
        assert!(
            classes.contains(&expected),
            "SabreDAV advertises {expected:?}, got: {classes:?}"
        );
    }
    assert!(
        classes.contains(&DavCompliance::Other(
            "calendarserver-principal-property-search".to_string()
        )),
        "the calendarserver-* vendor token must pass through as Other, got: {classes:?}"
    );
}
