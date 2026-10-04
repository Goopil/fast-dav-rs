//! Parsing of the calendar-proxy principal properties (`calendar-proxy-read-for`
//! / `calendar-proxy-write-for`) into `DavItem` (calendar-proxy companion spec
//! of RFC 6638).

use fast_dav_rs::webdav::streaming::parse_multistatus_bytes;

fn proxy_multistatus(read_for: &str, write_for: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/principals/users/test/</D:href>
    <D:propstat>
      <D:prop>{read_for}{write_for}</D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
    )
    .into_bytes()
}

#[test]
fn parse_calendar_proxy_read_for_and_write_for_hrefs() {
    let body = proxy_multistatus(
        "<C:calendar-proxy-read-for><D:href>/principals/users/alice/</D:href></C:calendar-proxy-read-for>",
        "<C:calendar-proxy-write-for><D:href>/principals/users/bob/</D:href></C:calendar-proxy-write-for>",
    );
    let items = parse_multistatus_bytes(&body).unwrap().items;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].calendar_proxy_read_for,
        vec!["/principals/users/alice/".to_string()],
        "read-for hrefs must be collected verbatim"
    );
    assert_eq!(
        items[0].calendar_proxy_write_for,
        vec!["/principals/users/bob/".to_string()],
        "write-for hrefs must be collected verbatim"
    );
}

#[test]
fn parse_calendar_proxy_read_for_collects_all_hrefs() {
    let body = proxy_multistatus(
        "<C:calendar-proxy-read-for>\
         <D:href>/principals/users/alice/</D:href>\
         <D:href>/principals/users/bob/</D:href>\
         </C:calendar-proxy-read-for>",
        "",
    );
    let items = parse_multistatus_bytes(&body).unwrap().items;
    assert_eq!(
        items[0].calendar_proxy_read_for,
        vec![
            "/principals/users/alice/".to_string(),
            "/principals/users/bob/".to_string(),
        ],
        "every href child must be collected"
    );
    assert!(items[0].calendar_proxy_write_for.is_empty());
}

#[test]
fn parse_absent_calendar_proxy_properties_yields_empty_vecs() {
    // Servers without calendar-proxy support answer with an empty element in
    // a 404 propstat (observed on the SabreDAV fixture): no href children, so
    // both fields stay empty.
    let body = proxy_multistatus(
        "<C:calendar-proxy-read-for/>",
        "<C:calendar-proxy-write-for/>",
    );
    let items = parse_multistatus_bytes(&body).unwrap().items;
    assert_eq!(items.len(), 1);
    assert!(items[0].calendar_proxy_read_for.is_empty());
    assert!(items[0].calendar_proxy_write_for.is_empty());
}
