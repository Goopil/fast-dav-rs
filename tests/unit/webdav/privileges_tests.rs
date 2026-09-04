use fast_dav_rs::webdav::Privilege;
use fast_dav_rs::webdav::streaming::parse_multistatus_bytes;
use fast_dav_rs::{Error, Operation, RequestCompressionMode, WebDavClient};

use crate::common::http_helpers::{response_head, serve_capture};

fn make_client(base: &str) -> WebDavClient {
    let client = WebDavClient::builder(base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

fn multistatus_body(privilege_set: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-privilege-set>{privilege_set}</D:current-user-privilege-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
    )
    .into_bytes()
}

#[tokio::test]
async fn current_user_privileges_returns_typed_privileges() {
    let body = multistatus_body("<D:privilege><D:read/><D:write-content/></D:privilege>");
    let (base, captured) = serve_capture(response_head("", body.len()), body).await;
    let client = make_client(&base);

    let privileges = client.current_user_privileges("").await.unwrap();
    assert_eq!(
        privileges,
        vec![Privilege::Read, Privilege::WriteContent],
        "read and write-content must be reported as typed privileges"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("PROPFIND / HTTP/1.1"),
        "the probe must be a PROPFIND against the base path: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "the probe must send Depth: 0: {req}"
    );
    assert!(
        req.contains("current-user-privilege-set"),
        "the PROPFIND body must request current-user-privilege-set (RFC 3744 §5.4): {req}"
    );
}

#[tokio::test]
async fn current_user_privileges_unknown_element_maps_to_other() {
    let body = multistatus_body("<D:privilege><D:read/><D:dancing/></D:privilege>");
    let (base, _captured) = serve_capture(response_head("", body.len()), body).await;
    let client = make_client(&base);

    let privileges = client.current_user_privileges("").await.unwrap();
    assert_eq!(
        privileges,
        vec![Privilege::Read, Privilege::Other("dancing".to_owned())],
        "unknown privilege elements must surface as Other(local_name)"
    );
}

#[tokio::test]
async fn current_user_privileges_accepts_repeated_privilege_containers() {
    let body = multistatus_body(
        "<D:privilege><D:read/></D:privilege><D:privilege><D:bind/><D:unbind/></D:privilege>",
    );
    let (base, _captured) = serve_capture(response_head("", body.len()), body).await;
    let client = make_client(&base);

    let privileges = client.current_user_privileges("").await.unwrap();
    assert_eq!(
        privileges,
        vec![Privilege::Read, Privilege::Bind, Privilege::Unbind,],
        "privileges from repeated <D:privilege> containers must be merged in document order"
    );
}

#[tokio::test]
async fn current_user_privileges_absent_prop_returns_empty() {
    let body: Vec<u8> = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop/>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
        .as_bytes()
        .to_vec();
    let (base, _captured) = serve_capture(response_head("", body.len()), body).await;
    let client = make_client(&base);

    let privileges = client.current_user_privileges("").await.unwrap();
    assert!(
        privileges.is_empty(),
        "a response without current-user-privilege-set yields no privileges"
    );
}

#[tokio::test]
async fn current_user_privileges_non_success_maps_operation() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);

    let err = client.current_user_privileges("").await.unwrap_err();
    match &err {
        Error::UnexpectedStatus { operation, .. } => {
            assert_eq!(
                *operation,
                Operation::PropfindCurrentUserPrivilegeSet,
                "non-success statuses must name the operation"
            );
        }
        other => panic!("expected UnexpectedStatus, got: {other:?}"),
    }
    assert!(
        err.to_string()
            .contains("PROPFIND current-user-privilege-set"),
        "the operation must render in the error Display: {err}"
    );
}

#[test]
fn multistatus_bytes_populates_current_user_privileges() {
    // Both the streaming and the buffered multistatus paths share the same
    // parser, so the buffered path must populate the field identically.
    let body = multistatus_body(
        "<D:privilege><D:READ/><D:write-content/><D:write/><D:write-properties/><D:bind/><D:unbind/><D:unlock/><D:read-free-busy/></D:privilege>",
    );
    let result = parse_multistatus_bytes(&body).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].current_user_privileges,
        vec![
            Privilege::Read,
            Privilege::WriteContent,
            Privilege::Write,
            Privilege::WriteProperties,
            Privilege::Bind,
            Privilege::Unbind,
            Privilege::Unlock,
            Privilege::ReadFreeBusy,
        ],
        "every mapped privilege local-name must be captured, case-insensitively"
    );
}

#[test]
fn dav_item_defaults_to_no_privileges() {
    let item = fast_dav_rs::webdav::DavItem::new();
    assert!(
        item.current_user_privileges.is_empty(),
        "DavItem::new() must leave current_user_privileges empty"
    );
}
