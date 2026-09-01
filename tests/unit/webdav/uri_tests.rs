use fast_dav_rs::WebDavClient;
use fast_dav_rs::webdav::client::encode_path_segments;

fn make_client(base: &str) -> WebDavClient {
    WebDavClient::new(base, None, None).unwrap()
}

#[test]
fn encode_spaces_in_segments() {
    assert_eq!(
        encode_path_segments("/my cal/event.ics"),
        "/my%20cal/event.ics"
    );
}

#[test]
fn encode_preserves_slash_separators() {
    assert_eq!(encode_path_segments("a/b c/d/"), "a/b%20c/d/");
}

#[test]
fn encode_preserves_valid_percent_escapes() {
    assert_eq!(encode_path_segments("e%20f/g%2fh"), "e%20f/g%2fh");
}

#[test]
fn encode_replaces_invalid_percent_with_escaped_percent() {
    assert_eq!(encode_path_segments("50% off"), "50%25%20off");
    assert_eq!(encode_path_segments("trailing%"), "trailing%25");
    assert_eq!(encode_path_segments("short%2x"), "short%252x");
}

#[test]
fn encode_percent_escape_uppercase_and_lowercase_hex() {
    assert_eq!(encode_path_segments("a%2Fb"), "a%2Fb");
    assert_eq!(encode_path_segments("a%2fb"), "a%2fb");
}

#[test]
fn encode_non_ascii_bytes() {
    assert_eq!(encode_path_segments("cal/ü/"), "cal/%C3%BC/");
}

#[test]
fn encode_control_characters() {
    assert_eq!(encode_path_segments("a\tb"), "a%09b");
}

#[test]
fn encode_leaves_unreserved_and_sub_delims_untouched() {
    assert_eq!(
        encode_path_segments("a-zA-Z0-9._~!$&'()*+,;=:@-"),
        "a-zA-Z0-9._~!$&'()*+,;=:@-"
    );
}

#[test]
fn encode_empty_string() {
    assert_eq!(encode_path_segments(""), "");
}

#[test]
fn encode_reserved_characters_in_segments() {
    assert_eq!(encode_path_segments("/a#b/c?d/e|f"), "/a%23b/c%3Fd/e%7Cf");
}

#[test]
fn build_uri_encodes_relative_path_with_spaces() {
    let client = make_client("http://127.0.0.1:8080/base/");
    let uri = client.build_uri("my cal/event.ics").unwrap();
    assert_eq!(uri.path(), "/base/my%20cal/event.ics");
}

#[test]
fn build_uri_preserves_already_encoded_path() {
    let client = make_client("http://127.0.0.1:8080/");
    let uri = client.build_uri("e%20v/").unwrap();
    assert_eq!(uri.path(), "/e%20v/");
}

#[test]
fn build_uri_preserves_escapes_in_base_path() {
    // Base URLs are validated at construction, so their escapes are always
    // valid and must survive the re-encoding of the combined path.
    let client = make_client("http://127.0.0.1:8080/my%20base/");
    let uri = client.build_uri("cal/").unwrap();
    assert_eq!(uri.path(), "/my%20base/cal/");
}

#[test]
fn build_uri_leaves_query_untouched() {
    let client = make_client("http://127.0.0.1:8080/");
    // `%zz` is not a valid escape: it would become `%25zz` if the path
    // encoder ran over the query. It must survive verbatim.
    let uri = client.build_uri("cal/?a=b&c=%zz").unwrap();
    assert_eq!(uri.path(), "/cal/");
    assert_eq!(uri.query().unwrap(), "a=b&c=%zz");
}

#[test]
fn build_uri_absolute_url_still_parsed_directly() {
    let client = make_client("http://127.0.0.1:8080/base/");
    let uri = client.build_uri("https://other.example.com/foo").unwrap();
    assert_eq!(uri.to_string(), "https://other.example.com/foo");
}
