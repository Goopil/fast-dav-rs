use fast_dav_rs::parse_multistatus_bytes;
use fast_dav_rs::webdav::{DavCapabilities, PropStat, WebDavError, parse_dav_header};

#[test]
fn parse_dav_header_class1_only() {
    let caps = parse_dav_header("1").expect("parse succeeds");
    assert!(caps.class1);
    assert!(!caps.class2);
    assert!(!caps.class3);
    assert!(caps.extensions.is_empty());
}

#[test]
fn parse_dav_header_class1_and_2() {
    let caps = parse_dav_header("1, 2").expect("parse succeeds");
    assert!(caps.class1);
    assert!(caps.class2);
    assert!(!caps.class3);
    assert!(caps.extensions.is_empty());
}

#[test]
fn parse_dav_header_all_classes() {
    let caps = parse_dav_header("1, 2, 3").expect("parse succeeds");
    assert!(caps.class1);
    assert!(caps.class2);
    assert!(caps.class3);
    assert!(caps.extensions.is_empty());
}

#[test]
fn parse_dav_header_with_calendar_extension() {
    let caps = parse_dav_header("1, 2, calendar-access").expect("parse succeeds");
    assert!(caps.class1);
    assert!(caps.class2);
    assert!(!caps.class3);
    assert_eq!(caps.extensions, vec!["calendar-access".to_string()]);
}

#[test]
fn parse_dav_header_with_multiple_extensions() {
    let caps = parse_dav_header("1, 2, calendar-access, addressbook").expect("parse succeeds");
    assert!(caps.class1);
    assert!(caps.class2);
    assert!(caps.extensions.len() == 2);
    assert!(caps.extensions.contains(&"calendar-access".to_string()));
    assert!(caps.extensions.contains(&"addressbook".to_string()));
}

#[test]
fn parse_dav_header_empty_string() {
    let caps = parse_dav_header("").expect("parse succeeds");
    assert!(!caps.class1);
    assert!(!caps.class2);
    assert!(!caps.class3);
    assert!(caps.extensions.is_empty());
}

#[test]
fn parse_dav_header_extension_only_no_classes() {
    let caps = parse_dav_header("calendar-access").expect("parse succeeds");
    assert!(!caps.class1);
    assert!(!caps.class2);
    assert!(!caps.class3);
    assert_eq!(caps.extensions, vec!["calendar-access".to_string()]);
}

#[test]
fn parse_dav_header_handles_extra_whitespace() {
    let caps = parse_dav_header("  1 ,  2  ,  calendar-access  ").expect("parse succeeds");
    assert!(caps.class1);
    assert!(caps.class2);
    assert_eq!(caps.extensions, vec!["calendar-access".to_string()]);
}

#[test]
fn parse_dav_header_with_compliance_class_3_and_extensions() {
    let caps = parse_dav_header("1, 2, 3, calendar-access, addressbook, version-control")
        .expect("parse succeeds");
    assert!(caps.class1);
    assert!(caps.class2);
    assert!(caps.class3);
    assert_eq!(caps.extensions.len(), 3);
}

#[test]
fn parse_error_body_extracts_precondition_code() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <C:no-uid-conflict/>
</D:error>"#;
    let err = fast_dav_rs::webdav::parse_error_body(xml).expect("parse succeeds");
    assert_eq!(err.precondition_code.as_deref(), Some("no-uid-conflict"));
}

#[test]
fn parse_error_body_extracts_dav_namespace_precondition() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:error xmlns:D="DAV:">
  <D:lock-token-matches-request-uri/>
</D:error>"#;
    let err = fast_dav_rs::webdav::parse_error_body(xml).expect("parse succeeds");
    assert_eq!(
        err.precondition_code.as_deref(),
        Some("lock-token-matches-request-uri")
    );
}

#[test]
fn parse_error_body_no_child_element() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:error xmlns:D="DAV:">
</D:error>"#;
    let err = fast_dav_rs::webdav::parse_error_body(xml).expect("parse succeeds");
    assert!(err.precondition_code.is_none());
    assert!(!err.parse_failed);
}

#[test]
fn parse_error_body_empty_body_returns_none() {
    let err = fast_dav_rs::webdav::parse_error_body(b"").expect("parse succeeds");
    assert!(err.precondition_code.is_none());
    assert!(!err.parse_failed);
}

#[test]
fn parse_error_body_non_xml_body_returns_none() {
    let err =
        fast_dav_rs::webdav::parse_error_body(b"Internal Server Error").expect("parse succeeds");
    assert!(err.precondition_code.is_none());
    assert!(!err.parse_failed);
}

#[test]
fn parse_error_body_malformed_xml_sets_parse_failed() {
    // AUDIT-015: a malformed error body must be distinguishable from "no
    // error body" — a hostile server must not be able to silently suppress
    // precondition diagnostics with garbage markup.
    let truncated = b"<D:error xmlns:D=\"DAV:\"><C:no-uid-conflict";
    let err = fast_dav_rs::webdav::parse_error_body(truncated).expect("parse succeeds");
    assert!(err.parse_failed, "truncated markup must set parse_failed");
    assert!(err.precondition_code.is_none());

    let mismatched = br#"<D:error xmlns:D="DAV:"><C:no-uid-conflict/></C:error>"#;
    let err = fast_dav_rs::webdav::parse_error_body(mismatched).expect("parse succeeds");
    assert!(err.parse_failed, "mismatched tags must set parse_failed");
}

#[test]
fn parse_error_body_takes_first_child_element() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <C:no-uid-conflict/>
  <C:some-other-code/>
</D:error>"#;
    let err = fast_dav_rs::webdav::parse_error_body(xml).expect("parse succeeds");
    assert_eq!(err.precondition_code.as_deref(), Some("no-uid-conflict"));
}

#[test]
fn parse_error_body_prefers_dav_namespace_over_vendor_elements() {
    // Real SabreDAV 4.7 body (e2e-verified): vendor extension elements come
    // first and the DAV: precondition is last — the code must not be
    // "sabredav-version".
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<d:error xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns">
  <s:sabredav-version>4.7.1</s:sabredav-version>
  <s:exception>Sabre\DAV\Exception\InvalidSyncToken</s:exception>
  <s:message>Invalid or unknown sync token</s:message>
  <d:valid-sync-token/>
</d:error>"#;
    let err = fast_dav_rs::webdav::parse_error_body(xml).expect("parse succeeds");
    assert_eq!(err.precondition_code.as_deref(), Some("valid-sync-token"));
    assert!(!err.parse_failed);
}

#[test]
fn parse_multistatus_distinguishes_multiple_propstat_groups() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/event.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>My Event</D:displayname>
        <D:getetag>"abc123"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
    <D:propstat>
      <D:prop>
        <D:resourcetype/>
      </D:prop>
      <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("parse succeeds");
    assert_eq!(result.items.len(), 1);
    let item = &result.items[0];
    assert_eq!(item.propstats.len(), 2, "should have two propstat groups");

    let first = &item.propstats[0];
    assert_eq!(first.status.as_deref(), Some("HTTP/1.1 200 OK"));
    assert!(
        first.prop_names.contains(&"displayname".to_string()),
        "first propstat should contain displayname, got {:?}",
        first.prop_names
    );
    assert!(
        first.prop_names.contains(&"getetag".to_string()),
        "first propstat should contain getetag, got {:?}",
        first.prop_names
    );

    let second = &item.propstats[1];
    assert_eq!(second.status.as_deref(), Some("HTTP/1.1 404 Not Found"));
    assert!(
        second.prop_names.contains(&"resourcetype".to_string()),
        "second propstat should contain resourcetype, got {:?}",
        second.prop_names
    );
}

#[test]
fn parse_multistatus_propstats_backward_compat_fields_populated_from_200() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/event.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>My Event</D:displayname>
        <D:getetag>"abc123"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
    <D:propstat>
      <D:prop>
        <D:owner>
          <D:href>/principals/unknown/</D:href>
        </D:owner>
      </D:prop>
      <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("parse succeeds");
    let item = &result.items[0];
    assert_eq!(item.displayname.as_deref(), Some("My Event"));
    assert_eq!(item.etag.as_deref(), Some("abc123"));
    assert_eq!(item.propstats.len(), 2);
}

#[test]
fn parse_multistatus_response_level_status() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/deleted.ics</D:href>
    <D:status>HTTP/1.1 404 Not Found</D:status>
    <D:propstat>
      <D:prop>
        <D:displayname>Deleted</D:displayname>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("parse succeeds");
    let item = &result.items[0];
    assert_eq!(
        item.response_status.as_deref(),
        Some("HTTP/1.1 404 Not Found")
    );
}

#[test]
fn parse_multistatus_single_propstat_still_works() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/event.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>My Event</D:displayname>
        <D:getetag>"abc123"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("parse succeeds");
    let item = &result.items[0];
    assert_eq!(item.propstats.len(), 1);
    assert_eq!(item.displayname.as_deref(), Some("My Event"));
    assert_eq!(item.etag.as_deref(), Some("abc123"));
}

#[test]
fn dav_capabilities_default_is_all_false() {
    let caps = DavCapabilities::default();
    assert!(!caps.class1);
    assert!(!caps.class2);
    assert!(!caps.class3);
    assert!(caps.extensions.is_empty());
}

#[test]
fn webdav_error_default_has_no_precondition() {
    let err = WebDavError::default();
    assert!(err.precondition_code.is_none());
}

#[test]
fn propstat_default_is_empty() {
    let ps = PropStat::default();
    assert!(ps.status.is_none());
    assert!(ps.prop_names.is_empty());
}
