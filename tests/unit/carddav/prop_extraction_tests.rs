use fast_dav_rs::carddav::client::extract_prop_inner;

#[test]
fn extract_prop_inner_d_prefix() {
    let xml = r#"<outer xmlns:D="DAV:"><D:prop>inner content</D:prop></outer>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some("inner content"));
}

#[test]
fn extract_prop_inner_lowercase_prefix() {
    let xml = r#"<outer xmlns:d="DAV:"><d:prop>inner content</d:prop></outer>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some("inner content"));
}

#[test]
fn extract_prop_inner_custom_prefix() {
    let xml = r#"<x:mkcol xmlns:x="DAV:"><x:prop>inner content</x:prop></x:mkcol>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some("inner content"));
}

#[test]
fn extract_prop_inner_no_prefix_default_namespace() {
    let xml = r#"<prop xmlns="DAV:">inner content</prop>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some("inner content"));
}

#[test]
fn extract_prop_inner_attributes_on_element() {
    let xml = r#"<D:mkcol xmlns:D="DAV:"><D:prop xmlns:x="urn:example" foo="bar">inner content</D:prop></D:mkcol>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some("inner content"));
}

#[test]
fn extract_prop_inner_nested_elements_captured_raw() {
    let xml = r#"<D:mkcol xmlns:D="DAV:">
        <D:prop>
            <D:resourcetype><D:collection/><x:addressbook xmlns:x="urn:example"/></D:resourcetype>
            <D:displayname>My Book</D:displayname>
        </D:prop>
    </D:mkcol>"#;
    let result = extract_prop_inner(xml);
    let inner = result.expect("prop inner must be found");
    assert!(inner.contains("<D:resourcetype>"));
    assert!(inner.contains("<D:displayname>My Book</D:displayname>"));
    assert!(!inner.contains("<D:prop>"));
}

#[test]
fn extract_prop_inner_self_closing_returns_empty() {
    let xml = r#"<x:mkcol xmlns:x="DAV:"><x:prop/></x:mkcol>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some(""));
}

#[test]
fn extract_prop_inner_absent_returns_none() {
    let xml = r#"<outer xmlns:D="DAV:"><D:something>content</D:something></outer>"#;
    let result = extract_prop_inner(xml);
    assert!(result.is_none());
}

#[test]
fn extract_prop_inner_no_closing_tag_returns_none() {
    let xml = r#"<outer xmlns:D="DAV:"><D:prop>content"#;
    let result = extract_prop_inner(xml);
    assert!(result.is_none());
}

#[test]
fn extract_prop_inner_returns_first_match() {
    let xml = r#"<root xmlns:D="DAV:"><D:prop>first</D:prop><D:prop>second</D:prop></root>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some("first"));
}

#[test]
fn extract_prop_inner_empty_inner() {
    let xml = r#"<root xmlns:D="DAV:"><D:prop></D:prop></root>"#;
    let result = extract_prop_inner(xml);
    assert_eq!(result.as_deref(), Some(""));
}

#[test]
fn extract_prop_inner_non_dav_prop_ignored() {
    let xml = r#"<root xmlns:x="urn:example"><x:prop>not dav</x:prop></root>"#;
    let result = extract_prop_inner(xml);
    assert!(result.is_none());
}
