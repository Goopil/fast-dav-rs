//! Parsing of the legacy `principal-URL` property (RFC 3744 §4.2) into
//! [`DavItem::principal_url`].
//!
//! `principal-URL` is a legacy principal property: RFC 5397 §3 defines
//! `current-user-principal` as the discovery bootstrap and superseded it for
//! that purpose, but some servers still return `principal-URL` in principal
//! PROPFINDs — it is surfaced as-is, populated only when the server sends it.

use fast_dav_rs::webdav::streaming::parse_multistatus_bytes;

#[test]
fn principal_url_present_parses_to_some_href() {
    // Canonical RFC 3744 §4.2 spelling (`principal-URL`) inside a 200 propstat.
    let xml = "<?xml version=\"1.0\"?>\
<D:multistatus xmlns:D=\"DAV:\">\
<D:response><D:href>/principals/users/alice/</D:href>\
<D:propstat><D:prop>\
<D:principal-URL><D:href>/principals/users/alice/</D:href></D:principal-URL>\
</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>\
</D:multistatus>";

    let result = parse_multistatus_bytes(xml.as_bytes()).unwrap();

    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].principal_url.as_deref(),
        Some("/principals/users/alice/"),
        "principal-URL href must land in DavItem::principal_url"
    );
}

#[test]
fn principal_url_absent_is_none() {
    let xml = "<?xml version=\"1.0\"?>\
<D:multistatus xmlns:D=\"DAV:\">\
<D:response><D:href>/principals/users/alice/</D:href>\
<D:propstat><D:prop>\
<D:displayname>Alice</D:displayname>\
</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>\
</D:multistatus>";

    let result = parse_multistatus_bytes(xml.as_bytes()).unwrap();

    assert_eq!(result.items.len(), 1);
    assert!(
        result.items[0].principal_url.is_none(),
        "absent principal-URL must stay None, got {:?}",
        result.items[0].principal_url
    );
}
