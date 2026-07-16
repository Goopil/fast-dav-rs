use fast_dav_rs::CardDavClient;
use fast_dav_rs::carddav::Depth;

#[test]
fn test_client_creation() {
    let client = CardDavClient::new("https://example.com/dav/", Some("user"), Some("pass"));
    assert!(client.is_ok());
}

#[test]
fn test_client_without_auth() {
    let client = CardDavClient::new("https://example.com/dav/", None, None);
    assert!(client.is_ok());
}

// --- Security tests (Fix 2 / v0.5.0) ---

#[test]
fn test_client_allows_http_without_credentials() {
    let result = CardDavClient::new("http://localhost:5232/dav/", None, None);
    assert!(result.is_ok(), "Should allow HTTP without credentials");
}

#[test]
fn test_build_uri_relative() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("addressbook/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/addressbook/");
}

#[test]
fn test_build_uri_absolute() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("https://other.com/test/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://other.com/test/");
}

#[test]
fn test_build_uri_with_query() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("addressbook/?param=value")
        .expect("Failed to build URI");
    assert_eq!(
        uri.to_string(),
        "https://example.com/dav/user/addressbook/?param=value"
    );
}

#[test]
fn test_build_uri_empty_path() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client.build_uri("").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/");
}

#[test]
fn test_build_uri_root_path_only() {
    let client =
        CardDavClient::new("https://example.com/", None, None).expect("Failed to create client");

    let uri = client
        .build_uri("addressbook/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/addressbook/");
}

#[test]
fn test_build_uri_with_special_characters() {
    let client = CardDavClient::new("https://example.com/dav/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("my-addressbook_123/")
        .expect("Failed to build URI");
    assert_eq!(
        uri.to_string(),
        "https://example.com/dav/my-addressbook_123/"
    );
}

#[test]
fn test_depth_values() {
    assert_eq!(Depth::Zero.as_str(), "0");
    assert_eq!(Depth::One.as_str(), "1");
    assert_eq!(Depth::Infinity.as_str(), "infinity");
}

#[test]
fn test_escape_xml_basic() {
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("Hello & World"),
        "Hello &amp; World"
    );
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("Test <tag>"),
        "Test &lt;tag&gt;"
    );
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("\"quotes\""),
        "&quot;quotes&quot;"
    );
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("'apos'"),
        "&apos;apos&apos;"
    );
}

#[test]
fn test_escape_xml_complex() {
    let input = "Mix & match <tag attr=\"value\"> with 'quotes'";
    let expected = "Mix &amp; match &lt;tag attr=&quot;value&quot;&gt; with &apos;quotes&apos;";
    assert_eq!(fast_dav_rs::carddav::client::escape_xml(input), expected);
}

#[test]
fn test_escape_xml_empty() {
    assert_eq!(fast_dav_rs::carddav::client::escape_xml(""), "");
}

#[test]
fn test_escape_xml_no_special_chars() {
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("normal text"),
        "normal text"
    );
}

#[test]
fn test_escape_xml_multiple_same_char() {
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("&&&&"),
        "&amp;&amp;&amp;&amp;"
    );
}

// build_* functions are now pub(crate); their tests live in src/carddav/client.rs #[cfg(test)]
// test_map_addressbook_list_filters_addressbooks, test_map_address_objects, test_map_sync_response
// moved to src/carddav/client.rs #[cfg(test)] mod tests
