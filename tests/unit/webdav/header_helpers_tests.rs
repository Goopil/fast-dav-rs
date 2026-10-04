//! Header helper tests: `schedule_tag_from_headers` and the
//! `schedule-tag` property parse into [`fast_dav_rs::webdav::DavItem`]
//! (RFC 6638 §10.1).

#[test]
fn schedule_tag_from_headers_quoted_and_bare() {
    let mut h = hyper::HeaderMap::new();
    h.insert("Schedule-Tag", r#""tag-1""#.parse().unwrap());
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&h).as_deref(),
        Some("tag-1")
    );
    h.clear();
    h.insert("Schedule-Tag", "tag-2".parse().unwrap());
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&h).as_deref(),
        Some("tag-2")
    );
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&hyper::HeaderMap::new()),
        None
    );
}

#[test]
fn schedule_tag_property_parses_into_dav_item() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/event.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
        <C:schedule-tag>opaque-tag-42</C:schedule-tag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = fast_dav_rs::webdav::streaming::parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].schedule_tag.as_deref(),
        Some("opaque-tag-42")
    );
}

#[test]
fn schedule_tag_property_absent_yields_none() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/event.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = fast_dav_rs::webdav::streaming::parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].schedule_tag, None);
}
