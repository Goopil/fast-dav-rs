//! Raw property capture (universal vendor-namespace support): every
//! property element of a multistatus must be recorded as a
//! [`RawProperty`] with its resolved namespace, local name and flattened
//! text — independent of typed support.

use fast_dav_rs::RawProperty;
use fast_dav_rs::webdav::streaming::parse_multistatus_bytes;

/// Multi-vendor multistatus: DAV + CalendarServer + Apple iCal +
/// Nextcloud + a provider-style namespace, with entities, CDATA and a
/// nested child element.
const MULTIVENDOR: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:"
               xmlns:CS="http://calendarserver.org/ns/"
               xmlns:A="http://apple.com/ns/ical/"
               xmlns:NC="http://nextcloud.com/ns"
               xmlns:IK="https://provider.example/dav/ns">
  <D:response>
    <D:href>/calendars/user/work/</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"abc-1"</D:getetag>
        <CS:getctag>3119-117</CS:getctag>
        <A:calendar-order>2</A:calendar-order>
        <NC:fileid>42</NC:fileid>
        <IK:space-id>1&nbsp;234</IK:space-id>
        <CS:me-card><D:href>/principals/test/</D:href></CS:me-card>
        <A:icon><![CDATA[<svg/>]]></A:icon>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

#[test]
fn raw_properties_capture_all_vendor_namespaces_in_document_order() {
    let result = parse_multistatus_bytes(MULTIVENDOR.as_bytes()).unwrap();
    assert_eq!(result.items.len(), 1);
    let props = &result.items[0].properties;
    assert_eq!(
        props,
        &[
            RawProperty {
                namespace: Some("DAV:".to_string()),
                name: "getetag".to_string(),
                value: "\"abc-1\"".to_string(),
            },
            RawProperty {
                namespace: Some("http://calendarserver.org/ns/".to_string()),
                name: "getctag".to_string(),
                value: "3119-117".to_string(),
            },
            RawProperty {
                namespace: Some("http://apple.com/ns/ical/".to_string()),
                name: "calendar-order".to_string(),
                value: "2".to_string(),
            },
            RawProperty {
                namespace: Some("http://nextcloud.com/ns".to_string()),
                name: "fileid".to_string(),
                value: "42".to_string(),
            },
            // `&nbsp;` is an HTML entity the XML text pipeline keeps
            // verbatim (same behavior as typed fields).
            RawProperty {
                namespace: Some("https://provider.example/dav/ns".to_string()),
                name: "space-id".to_string(),
                value: "1&nbsp;234".to_string(),
            },
            // Descendant text flattened: the child <D:href> contributes
            // its text to the enclosing property's value.
            RawProperty {
                namespace: Some("http://calendarserver.org/ns/".to_string()),
                name: "me-card".to_string(),
                value: "/principals/test/".to_string(),
            },
            RawProperty {
                namespace: Some("http://apple.com/ns/ical/".to_string()),
                name: "icon".to_string(),
                value: "<svg/>".to_string(),
            },
        ]
    );
    // Typed fields remain projections and are unaffected (normalize_etag
    // strips the quotes; the raw value stays verbatim).
    assert_eq!(result.items[0].etag.as_deref(), Some("abc-1"));
}

#[test]
fn raw_property_preserves_true_namespace_on_typed_name_collision() {
    // A foreign-namespace element whose local name collides with a typed
    // element must NOT fill the typed field, but must appear in the raw
    // list with its true namespace and local name.
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:X="urn:evil">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop>
        <X:getetag>injected</X:getetag>
        <D:getetag>"real"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    let item = &result.items[0];
    assert_eq!(
        item.etag.as_deref(),
        Some("real"),
        "typed field must stay clean (normalize_etag strips quotes)"
    );
    assert_eq!(
        item.properties,
        &[
            RawProperty {
                namespace: Some("urn:evil".to_string()),
                name: "getetag".to_string(),
                value: "injected".to_string(),
            },
            RawProperty {
                namespace: Some("DAV:".to_string()),
                name: "getetag".to_string(),
                value: "\"real\"".to_string(),
            },
        ]
    );
}

#[test]
fn raw_property_self_closing_and_undeclared_prefix() {
    // Self-closing properties are recorded with an empty value (presence
    // signal); an undeclared prefix keeps namespace = None (existing
    // tolerance).
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/></D:resourcetype>
        <X:flag/>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(
        result.items[0].properties,
        &[
            RawProperty {
                namespace: Some("DAV:".to_string()),
                name: "resourcetype".to_string(),
                // The nested empty <D:collection/> contributes no text.
                value: String::new(),
            },
            RawProperty {
                namespace: None,
                name: "flag".to_string(),
                value: String::new(),
            },
        ]
    );
    assert!(result.items[0].is_collection);
}

#[test]
fn raw_properties_keep_duplicates_across_propstats() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:CS="http://calendarserver.org/ns/">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop><CS:getctag>first</CS:getctag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
    <D:propstat>
      <D:prop><CS:getctag>second</CS:getctag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    let values: Vec<&str> = result.items[0]
        .properties
        .iter()
        .map(|p| p.value.as_str())
        .collect();
    assert_eq!(values, ["first", "second"]);
}

#[test]
fn raw_properties_are_capped_per_response_item() {
    let mut xml = String::from(
        r#"<D:multistatus xmlns:D="DAV:" xmlns:V="urn:v"><D:response><D:href>/c/</D:href><D:propstat><D:prop>"#,
    );
    for i in 0..1100 {
        xml.push_str(&format!("<V:p{i}>x</V:p{i}>"));
    }
    xml.push_str("</D:prop></D:propstat></D:response></D:multistatus>");
    let result = parse_multistatus_bytes(xml.as_bytes()).unwrap();
    assert_eq!(result.items[0].properties.len(), 1024);
}
