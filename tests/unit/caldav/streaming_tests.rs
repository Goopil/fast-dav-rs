use anyhow::anyhow;
use fast_dav_rs::streaming::*;

#[test]
fn test_element_from_bytes() {
    // Test basic elements
    assert_eq!(element_from_bytes(b"multistatus"), ElementName::Multistatus);
    assert_eq!(element_from_bytes(b"response"), ElementName::Response);
    assert_eq!(element_from_bytes(b"propstat"), ElementName::Propstat);
    assert_eq!(element_from_bytes(b"href"), ElementName::Href);
    assert_eq!(element_from_bytes(b"displayname"), ElementName::Displayname);
    assert_eq!(element_from_bytes(b"getetag"), ElementName::Getetag);

    // Test namespaced elements
    assert_eq!(element_from_bytes(b"D:href"), ElementName::Href);
    assert_eq!(
        element_from_bytes(b"C:calendar-data"),
        ElementName::CalendarData
    );

    // Test unknown elements
    assert_eq!(element_from_bytes(b"unknown-element"), ElementName::Other);
}

#[test]
fn test_decode_text() {
    // Test normal text
    assert_eq!(decode_text(b"hello").unwrap(), "hello");

    // Test escaped text
    assert_eq!(decode_text(b"hello &amp; world").unwrap(), "hello & world");
    assert_eq!(decode_text(b"test &lt;tag&gt;").unwrap(), "test <tag>");

    // Test invalid UTF-8 handling
    assert!(decode_text(b"\xFF\xFE").is_ok()); // Should handle gracefully
}

#[test]
fn test_multistatus_visit_matches_vec() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal1/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Calendar One</D:displayname>
        <D:getetag>"etag-1"</D:getetag>
      </D:prop>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal2/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Calendar Two</D:displayname>
        <D:getetag>"etag-2"</D:getetag>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let items = parse_multistatus_bytes(xml.as_bytes())
        .expect("parse bytes")
        .items;

    let mut visited = Vec::new();
    parse_multistatus_bytes_visit(xml.as_bytes(), |item| {
        visited.push(item);
        Ok(())
    })
    .expect("visit parse");

    assert_eq!(items.len(), visited.len());
    for (lhs, rhs) in items.iter().zip(&visited) {
        assert_eq!(lhs.href, rhs.href);
        assert_eq!(lhs.displayname, rhs.displayname);
        assert_eq!(lhs.etag, rhs.etag);
    }
}

#[test]
fn test_multistatus_visit_error_propagates() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/err/</D:href>
  </D:response>
</D:multistatus>
"#;

    let err = parse_multistatus_bytes_visit(xml.as_bytes(), |_item| Err(anyhow!("boom")));

    assert!(err.is_err(), "expected visitor error to propagate");
}
