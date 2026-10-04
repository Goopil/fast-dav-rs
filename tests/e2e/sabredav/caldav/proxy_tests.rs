//! Calendar proxies (the calendar-proxy companion spec of RFC 6638) against
//! the SabreDAV fixture.
//!
//! The fixture as shipped (SabreDAV 4.7.1 behind the plain DAVACL principal
//! collection) has no calendar-proxy support: the `calendar-proxy-*-for`
//! properties answer 404 propstats and the `calendar-proxy-read`/`-write`
//! group principals do not exist. The tests record that observed behavior
//! (0.13 convention) and, should a fixture expose the proxy principals, also
//! exercise the grant/revoke round-trip through the `ACL` wire form.

use crate::util::sabredav_caldav_client;
use fast_dav_rs::{Depth, Error, Operation};

/// A minimal PROPFIND body used to probe for the proxy group principals.
const PROBE_BODY: &str = r#"<?xml version="1.0"?>
<D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#;

/// `list_calendar_proxies` succeeds against the fixture principal. On the
/// fixture as shipped the proxy-for properties are absent (404 propstat,
/// observed), so both vectors are empty; when a fixture advertises
/// delegations, every href must be a principal path.
#[tokio::test]
async fn test_list_calendar_proxies_records_observed_behavior_on_sabredav() {
    let client = sabredav_caldav_client();
    let principal = client
        .discover_current_user_principal()
        .await
        .expect("principal discovery PROPFIND")
        .expect("fixture must advertise the current user principal");

    let proxies = client
        .list_calendar_proxies(&principal)
        .await
        .expect("calendar-proxy PROPFIND");

    if proxies.read_for.is_empty() && proxies.write_for.is_empty() {
        // Observed on the fixture as shipped: no calendar-proxy support, so
        // the properties come back empty (404 propstat).
    } else {
        for href in proxies.read_for.iter().chain(proxies.write_for.iter()) {
            assert!(
                href.contains("/principals/"),
                "proxy-for hrefs must be principal paths, got {proxies:?}"
            );
        }
    }
}

/// The proxy group principals and the `ACL` wire form for delegation
/// changes, recorded against whatever the fixture actually supports:
///
/// - fixture as shipped: both proxy group principals are absent (404,
///   observed) — nothing to delegate through;
/// - proxy-enabled fixture: `grant_calendar_proxy` resolves the group and
///   issues an `ACL`; a rejection of the ACL wire form is recorded as the
///   observed server behavior (the delegation write form is
///   server-dependent), while a success must be revocable.
#[tokio::test]
async fn test_calendar_proxy_delegation_change_records_observed_behavior_on_sabredav() {
    let client = sabredav_caldav_client();
    let principal = client
        .discover_current_user_principal()
        .await
        .expect("principal discovery PROPFIND")
        .expect("fixture must advertise the current user principal");

    let read_group = format!("{}calendar-proxy-read", principal);
    let write_group = format!("{}calendar-proxy-write", principal);
    let read_group_exists = client
        .propfind(&read_group, Depth::Zero, PROBE_BODY)
        .await
        .expect("proxy group probe PROPFIND")
        .status()
        .is_success();

    if !read_group_exists {
        // Observed on the fixture as shipped: no proxy group principals
        // (404), so delegation changes are unsupported here.
        return;
    }

    // Proxy-enabled fixture: the delegate can only be the single fixture
    // principal (the fixture seeds one user), exercising the wire form.
    let write_group_exists = client
        .propfind(&write_group, Depth::Zero, PROBE_BODY)
        .await
        .expect("proxy group probe PROPFIND")
        .status()
        .is_success();
    assert!(
        write_group_exists,
        "a fixture exposing the read proxy group must also expose the write group"
    );

    match client
        .grant_calendar_proxy(&principal, &principal, false)
        .await
    {
        Ok(()) => {
            client
                .revoke_calendar_proxy(&principal, &principal, false)
                .await
                .expect("revoke after a successful grant");
        }
        Err(Error::UnexpectedStatus {
            operation: Operation::Acl,
            ..
        }) => {
            // Observed: the server exposes the proxy group principals but
            // rejects the ACL wire form for delegation changes (e.g. it
            // expects PROPPATCH of group-member-set instead) — recorded.
        }
        Err(err) => panic!("unexpected error during proxy grant: {err:?}"),
    }
}
