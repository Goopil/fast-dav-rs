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
fn build_uri_encodes_question_mark_and_hash_in_resource_names() {
    // A `?`/`#` in a resource name is part of the path: it must be encoded,
    // never interpreted as a query/fragment separator (issue #139).
    let client = make_client("http://127.0.0.1:8080/");
    let uri = client.build_uri("cal/report?q.ics").unwrap();
    assert_eq!(uri.path(), "/cal/report%3Fq.ics");
    assert!(uri.query().is_none(), "no query may be split off the path");

    let uri = client.build_uri("cal/a#b.ics").unwrap();
    assert_eq!(uri.path(), "/cal/a%23b.ics");

    // Query-ish input keeps its characters encoded too.
    let uri = client.build_uri("cal/?a=b&c=%zz").unwrap();
    assert_eq!(uri.path(), "/cal/%3Fa=b&c=%25zz");
    assert!(uri.query().is_none());
}

#[test]
fn build_uri_keeps_percent_escapes_verbatim() {
    // `%41` is not decoded: pre-encoded input addresses the resource named
    // by its encoded form (`a%41b` != `aAb`).
    let client = make_client("http://127.0.0.1:8080/");
    let uri = client.build_uri("a%41b.txt").unwrap();
    assert_eq!(uri.path(), "/a%41b.txt");
}

#[tokio::test]
async fn move_rejects_destination_without_absolute_uri() {
    // Unroutable base: if validation passed, the request would fail with a
    // connection error — never with InvalidInput.
    let client = make_client("http://127.0.0.1:1/");

    let err = client.r#move("src", "not a uri", true).await.unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "bare relative string must be rejected, got: {err:?}"
    );

    // A root-relative reference has no scheme/authority: rejected.
    let err = client.r#move("src", "/only/path", true).await.unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "path-only reference must be rejected, got: {err:?}"
    );

    // Scheme without authority: rejected.
    let err = client.copy("src", "http://", true).await.unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "scheme-only reference must be rejected, got: {err:?}"
    );
}

#[tokio::test]
async fn move_with_valid_absolute_destination_passes_validation() {
    // Unroutable origin: validation must let a well-formed absolute URL
    // through and the failure must be a transport error, not InvalidInput.
    let client = make_client("http://127.0.0.1:1/");
    let err = client
        .r#move("src", "http://other.example.com/dav/dest", true)
        .await
        .unwrap_err();
    assert!(
        !matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "a valid absolute Destination must not be rejected by validation, got: {err:?}"
    );
}

#[tokio::test]
async fn move_rejects_destination_with_userinfo() {
    let client = make_client("http://127.0.0.1:1/");

    // Userinfo in the Destination is never generated (RFC 9110 §3.2):
    // rejected before any network I/O.
    let err = client
        .r#move("src", "https://user:hunter2@dav.example.com/dest", true)
        .await
        .unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "a userinfo-bearing Destination must be rejected, got: {err:?}"
    );

    // Validation failures redact credentials carried in the raw value.
    let err = client
        .copy("src", "https://user:hunter2@", true)
        .await
        .unwrap_err();
    match err {
        fast_dav_rs::Error::InvalidInput(msg) => {
            assert!(
                !msg.contains("hunter2"),
                "error messages must not echo credentials: {msg}"
            );
            assert!(msg.contains("***"), "credentials must be redacted: {msg}");
        }
        other => panic!("expected InvalidInput, got: {other:?}"),
    }
}

#[test]
fn build_uri_absolute_url_still_parsed_directly() {
    let client = make_client("http://127.0.0.1:8080/base/");
    let uri = client.build_uri("https://other.example.com/foo").unwrap();
    assert_eq!(uri.to_string(), "https://other.example.com/foo");
}
