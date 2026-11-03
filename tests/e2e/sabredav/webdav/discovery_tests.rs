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

/// Raw property capture (universal vendor-namespace support): an allprop
/// PROPFIND against the live fixture must surface every returned property
/// element exactly once, with its namespace resolved. SabreDAV answers
/// with real vendor properties here (`http://sabredav.org/ns`,
/// `http://calendarserver.org/ns/` getctag, `http://apple.com/ns/ical/`
/// calendar-order) and structured properties (`resourcetype` with child
/// elements) whose flattened text is empty but whose entry must exist.
#[tokio::test]
async fn test_allprop_propfind_captures_raw_properties() {
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let resp = client
        .propfind(
            "calendars/test/",
            fast_dav_rs::Depth::One,
            &fast_dav_rs::webdav::build_propfind_allprop(),
        )
        .await
        .expect("allprop PROPFIND must succeed");
    assert_eq!(resp.status(), 207);
    let parsed = fast_dav_rs::webdav::streaming::parse_multistatus_bytes(&resp.into_body())
        .expect("multistatus body must parse");
    assert!(!parsed.items.is_empty());
    for item in &parsed.items {
        let names: Vec<&str> = item
            .propstats
            .iter()
            .flat_map(|ps| ps.prop_names.iter().map(String::as_str))
            .collect();
        assert_eq!(
            item.properties.len(),
            names.len(),
            "every property element must be captured exactly once for {}",
            item.href
        );
        for prop in &item.properties {
            assert!(
                prop.namespace.is_some(),
                "every property's namespace must resolve (vendor URIs included), got: {prop:?}"
            );
        }
    }
    // The fixture's default calendar carries real vendor properties.
    let default_cal = parsed
        .items
        .iter()
        .find(|item| item.href.ends_with("/default/"))
        .expect("the default calendar must be listed");
    assert!(
        default_cal.properties.iter().any(|p| p.namespace.as_deref()
            == Some("http://calendarserver.org/ns/")
            && p.name == "getctag"
            && !p.value.is_empty()),
        "the CalendarServer getctag must be captured with its value, got: {:?}",
        default_cal.properties
    );
}
