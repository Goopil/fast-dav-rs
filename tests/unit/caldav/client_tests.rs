use fast_dav_rs::{CalDavClient, Depth};

#[test]
fn test_client_creation() {
    let client = CalDavClient::new("https://example.com/dav/", Some("user"), Some("pass"));
    assert!(client.is_ok());
}

#[test]
fn test_client_without_auth() {
    let client = CalDavClient::new("https://example.com/dav/", None, None);
    assert!(client.is_ok());
}

// --- URI building tests ---

#[test]
fn test_build_uri_relative() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client.build_uri("calendar/").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/calendar/");
}

#[test]
fn test_build_uri_absolute() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("https://other.com/test/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://other.com/test/");
}

#[test]
fn test_build_uri_with_query() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("calendar/?param=value")
        .expect("Failed to build URI");
    assert_eq!(
        uri.to_string(),
        "https://example.com/dav/user/calendar/?param=value"
    );
}

#[test]
fn test_build_uri_empty_path() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client.build_uri("").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/");
}

#[test]
fn test_build_uri_root_path_only() {
    let client =
        CalDavClient::new("https://example.com/", None, None).expect("Failed to create client");

    let uri = client.build_uri("calendar/").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/calendar/");
}

#[test]
fn test_build_uri_with_special_characters() {
    let client =
        CalDavClient::new("https://example.com/dav/", None, None).expect("Failed to create client");

    let uri = client
        .build_uri("my-calendar_123/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/my-calendar_123/");
}

// --- Depth enum ---

#[test]
fn test_depth_values() {
    assert_eq!(Depth::Zero.as_str(), "0");
    assert_eq!(Depth::One.as_str(), "1");
    assert_eq!(Depth::Infinity.as_str(), "infinity");
}

// --- escape_xml (stays pub) ---

#[test]
fn test_escape_xml_basic() {
    assert_eq!(
        fast_dav_rs::client::escape_xml("Hello & World"),
        "Hello &amp; World"
    );
    assert_eq!(
        fast_dav_rs::client::escape_xml("Test <tag>"),
        "Test &lt;tag&gt;"
    );
    assert_eq!(
        fast_dav_rs::client::escape_xml("\"quotes\""),
        "&quot;quotes&quot;"
    );
    assert_eq!(
        fast_dav_rs::client::escape_xml("'apos'"),
        "&apos;apos&apos;"
    );
}

#[test]
fn test_escape_xml_complex() {
    let input = "Mix & match <tag attr=\"value\"> with 'quotes'";
    let expected = "Mix &amp; match &lt;tag attr=&quot;value&quot;&gt; with &apos;quotes&apos;";
    assert_eq!(fast_dav_rs::client::escape_xml(input), expected);
}

#[test]
fn test_escape_xml_empty() {
    assert_eq!(fast_dav_rs::client::escape_xml(""), "");
}

#[test]
fn test_escape_xml_no_special_chars() {
    assert_eq!(
        fast_dav_rs::client::escape_xml("normal text"),
        "normal text"
    );
}

#[test]
fn test_escape_xml_multiple_same_char() {
    assert_eq!(
        fast_dav_rs::client::escape_xml("&&&&"),
        "&amp;&amp;&amp;&amp;"
    );
}
