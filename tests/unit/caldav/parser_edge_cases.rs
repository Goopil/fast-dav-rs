use fast_dav_rs::parse_multistatus_bytes;
use std::time::Instant;

#[test]
fn test_parse_multistatus_performance() {
    // Create a large multistatus response with many items
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<D:multistatus xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">",
    );

    // Add 1000 response items
    for i in 0..1000 {
        xml.push_str(&format!(
            r#"
  <D:response>
    <D:href>/dav/user01/event{}.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-{}"</D:getetag>
        <D:resourcetype/>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>"#,
            i, i
        ));
    }

    xml.push_str("\n</D:multistatus>");

    let start = Instant::now();
    let items = parse_multistatus_bytes(xml.as_bytes())
        .expect("Parsing should succeed")
        .items;
    let duration = start.elapsed();

    assert_eq!(items.len(), 1000);
    assert!(
        duration.as_millis() < 1000,
        "Parsing should complete in less than 1 second"
    );
}

#[test]
fn test_parse_multistatus_malformed_xml() {
    let malformed_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/event1.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
      </D:prop>
      <!-- Missing closing tags -->
"#;

    let err = parse_multistatus_bytes(malformed_xml.as_bytes())
        .expect_err("truncated XML must be rejected");
    assert!(
        err.to_string().contains("unclosed element"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_parse_multistatus_duplicate_attribute_errors() {
    // quick-xml attribute checks are enabled: a duplicated attribute coming from a
    // buggy or malicious server must be a parse error, not silently tolerated.
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/personal/</D:href>
    <D:propstat>
      <D:prop>
        <C:supported-calendar-component-set>
          <C:comp name="VEVENT" name="VTODO"/>
        </C:supported-calendar-component-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let result = parse_multistatus_bytes(xml.as_bytes());
    assert!(
        result.is_err(),
        "duplicated attribute should be rejected as a parse error"
    );
}

#[test]
fn test_parse_multistatus_mismatched_closing_tag_errors() {
    // A closing tag that does not match the currently open element must be a hard
    // parse error instead of silently corrupting the parser state.
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/event1.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
      </D:prop>
    </D:response>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let result = parse_multistatus_bytes(xml.as_bytes());
    assert!(
        result.is_err(),
        "mismatched closing tag should be rejected as a parse error"
    );
}

#[test]
fn test_parse_multistatus_unexpected_elements() {
    // Test with unexpected XML elements
    let xml_with_extra = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/event1.ics</D:href>
    <unexpected-element>Should be ignored</unexpected-element>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
        <unknown-property>Should be ignored</unknown-property>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
      <extra-element>Should be ignored</extra-element>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let items = parse_multistatus_bytes(xml_with_extra.as_bytes())
        .expect("Parsing should succeed")
        .items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].href, "/dav/user01/event1.ics");
    assert_eq!(items[0].etag.as_deref(), Some("\"etag-1\""));
}
