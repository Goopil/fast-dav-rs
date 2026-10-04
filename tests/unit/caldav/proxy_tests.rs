//! Wire tests for the CalDAV calendar-proxy helpers (RFC 6638 companion
//! spec): listing delegators, resolving proxy group members, and the
//! grant/revoke round-trip through the low-level `ACL` primitive.

use fast_dav_rs::{CalDavClient, Error, Operation, RequestCompressionMode};

use crate::common::http_helpers::{
    response_head, serve_capture, serve_once, serve_sequence, unreachable_base,
};

fn make_client(base: &str) -> CalDavClient {
    let client = CalDavClient::new(base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

fn group_member_set_body() -> Vec<u8> {
    r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/principals/users/test/calendar-proxy-read/</D:href>
    <D:propstat>
      <D:prop>
        <D:group-member-set>
          <D:href>/principals/users/delegate/</D:href>
          <D:href>/principals/users/other/</D:href>
        </D:group-member-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
        .as_bytes()
        .to_vec()
}

#[tokio::test]
async fn list_calendar_proxies_maps_proxy_for_properties() {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/principals/users/test/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-proxy-read-for><D:href>/principals/users/alice/</D:href></C:calendar-proxy-read-for>
        <C:calendar-proxy-write-for><D:href>/principals/users/bob/</D:href></C:calendar-proxy-write-for>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
        .to_string();
    let (base, captured) = serve_capture(response_head("", body.len()), body.into_bytes()).await;
    let client = make_client(&base);

    let info = client
        .list_calendar_proxies("principals/users/test/")
        .await
        .unwrap();
    assert_eq!(
        info.read_for,
        vec!["/principals/users/alice/".to_string()],
        "read-for hrefs must be mapped"
    );
    assert_eq!(
        info.write_for,
        vec!["/principals/users/bob/".to_string()],
        "write-for hrefs must be mapped"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(req.starts_with("PROPFIND "), "expected PROPFIND: {req}");
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected 'Depth: 0': {req}"
    );
    assert!(
        req.contains("calendar-proxy-read-for") && req.contains("calendar-proxy-write-for"),
        "the PROPFIND body must request both proxy-for properties: {req}"
    );
}

#[tokio::test]
async fn list_calendar_proxies_non_success_maps_to_unexpected_status() {
    let base = serve_once(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);
    let err = client
        .list_calendar_proxies("principals/users/test/")
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            Error::UnexpectedStatus { operation, .. } if *operation == Operation::PropfindCalendarProxy
        ),
        "got: {err:?}"
    );
}

#[tokio::test]
async fn calendar_proxy_group_members_resolves_group_member_set() {
    let body = group_member_set_body();
    let (base, captured) = serve_capture(response_head("", body.len()), body).await;
    let client = make_client(&base);

    let members = client
        .calendar_proxy_group_members("principals/users/test/", false)
        .await
        .unwrap();
    assert_eq!(
        members,
        vec![
            "/principals/users/delegate/".to_string(),
            "/principals/users/other/".to_string(),
        ],
        "group-member-set hrefs must be resolved and sorted"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.starts_with("PROPFIND /principals/users/test/calendar-proxy-read "),
        "the PROPFIND must target the read proxy group principal: {req}"
    );
    assert!(
        req.contains("group-member-set"),
        "the PROPFIND body must request group-member-set: {req}"
    );
}

#[tokio::test]
async fn calendar_proxy_group_members_targets_write_group() {
    let body = group_member_set_body();
    let (base, captured) = serve_capture(response_head("", body.len()), body).await;
    let client = make_client(&base);

    client
        .calendar_proxy_group_members("principals/users/test/", true)
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.starts_with("PROPFIND /principals/users/test/calendar-proxy-write "),
        "the PROPFIND must target the write proxy group principal: {req}"
    );
}

#[tokio::test]
async fn grant_calendar_proxy_issues_acl_for_delegate() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (response_head("", 0), Vec::new()),
    ])
    .await;
    let client = make_client(&base);

    client
        .grant_calendar_proxy(
            "principals/users/test/",
            "/principals/users/delegate/",
            false,
        )
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2, "PROPFIND then ACL");

    let propfind = String::from_utf8_lossy(&captured[0]);
    assert!(
        propfind.starts_with("PROPFIND /principals/users/test/calendar-proxy-read "),
        "grant must resolve the current proxy group members first: {propfind}"
    );

    let acl = String::from_utf8_lossy(&captured[1]);
    assert!(acl.starts_with("ACL "), "expected ACL method: {acl}");
    assert!(
        acl.contains("/principals/users/delegate/"),
        "the ACL body must grant the delegate: {acl}"
    );
    assert!(acl.contains("<D:read/>"), "read proxy grants `read`: {acl}");
    assert!(
        !acl.contains("<D:write/>"),
        "read proxy must not grant `write`: {acl}"
    );
    assert!(
        acl.contains("/principals/users/other/"),
        "existing group members must keep their grant: {acl}"
    );
}

#[tokio::test]
async fn grant_calendar_proxy_write_grants_write_privilege() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (response_head("", 0), Vec::new()),
    ])
    .await;
    let client = make_client(&base);

    client
        .grant_calendar_proxy(
            "principals/users/test/",
            "/principals/users/delegate/",
            true,
        )
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let acl = String::from_utf8_lossy(&guard[1]);
    assert!(
        acl.contains("<D:write/>"),
        "write proxy grants `write`: {acl}"
    );
}

#[tokio::test]
async fn revoke_calendar_proxy_removes_delegate_from_acl() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (response_head("", 0), Vec::new()),
    ])
    .await;
    let client = make_client(&base);

    client
        .revoke_calendar_proxy(
            "principals/users/test/",
            "/principals/users/delegate/",
            false,
        )
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2, "PROPFIND then ACL");

    let acl = String::from_utf8_lossy(&captured[1]);
    assert!(acl.starts_with("ACL "), "expected ACL method: {acl}");
    assert!(
        !acl.contains("/principals/users/delegate/"),
        "the revoked delegate must be absent from the ACL body: {acl}"
    );
    assert!(
        acl.contains("/principals/users/other/"),
        "the other group member must keep its grant: {acl}"
    );
}

#[tokio::test]
async fn grant_rejects_empty_delegate_before_io() {
    let base = unreachable_base().await;
    let client = make_client(&base);
    let err = client
        .grant_calendar_proxy("principals/users/test/", "   ", false)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "empty delegate must fail pre-I/O validation, got: {err:?}"
    );
}

#[tokio::test]
async fn calendar_proxy_group_members_non_success_maps_to_unexpected_status() {
    let base = serve_once(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);
    let err = client
        .calendar_proxy_group_members("principals/users/test/", false)
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            Error::UnexpectedStatus { operation, .. } if *operation == Operation::PropfindCalendarProxy
        ),
        "got: {err:?}"
    );
}

#[tokio::test]
async fn grant_calendar_proxy_acl_non_success_maps_to_unexpected_status() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
            Vec::new(),
        ),
    ])
    .await;
    let client = make_client(&base);
    let err = client
        .grant_calendar_proxy(
            "principals/users/test/",
            "/principals/users/delegate/",
            false,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::UnexpectedStatus { operation, .. } if *operation == Operation::Acl),
        "the failing ACL step must surface as Operation::Acl, got: {err:?}"
    );
    assert_eq!(captured.lock().unwrap().len(), 2, "PROPFIND then ACL");
}

#[tokio::test]
async fn revoke_calendar_proxy_acl_non_success_maps_to_unexpected_status() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
            Vec::new(),
        ),
    ])
    .await;
    let client = make_client(&base);
    let err = client
        .revoke_calendar_proxy(
            "principals/users/test/",
            "/principals/users/delegate/",
            false,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::UnexpectedStatus { operation, .. } if *operation == Operation::Acl),
        "the failing ACL step must surface as Operation::Acl, got: {err:?}"
    );
    assert_eq!(captured.lock().unwrap().len(), 2, "PROPFIND then ACL");
}

#[tokio::test]
async fn revoke_last_member_sends_empty_acl_body() {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/principals/users/test/calendar-proxy-read/</D:href>
    <D:propstat>
      <D:prop>
        <D:group-member-set>
          <D:href>/principals/users/delegate/</D:href>
        </D:group-member-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
        .as_bytes()
        .to_vec();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (response_head("", 0), Vec::new()),
    ])
    .await;
    let client = make_client(&base);

    client
        .revoke_calendar_proxy(
            "principals/users/test/",
            "/principals/users/delegate/",
            false,
        )
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2, "PROPFIND then ACL");
    let acl = String::from_utf8_lossy(&captured[1]);
    let acl_body = &acl[acl.find("\r\n\r\n").map(|pos| pos + 4).unwrap_or(0)..];
    assert_eq!(
        acl_body, "<D:acl xmlns:D=\"DAV:\"></D:acl>",
        "revoking the last member must send the empty <D:acl> document: {acl}"
    );
}

#[tokio::test]
async fn revoke_rejects_empty_delegate_before_io() {
    let base = unreachable_base().await;
    let client = make_client(&base);
    let err = client
        .revoke_calendar_proxy("principals/users/test/", "", false)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "empty delegate must fail pre-I/O validation, got: {err:?}"
    );
}

#[tokio::test]
async fn revoke_calendar_proxy_normalizes_delegate_href_form() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (response_head("", 0), Vec::new()),
    ])
    .await;
    let client = make_client(&base);

    // The server lists the delegate as a path href; the caller passes the
    // same principal as an absolute URL — href normalization must still
    // revoke it instead of re-issuing an unchanged ACL.
    let delegate = format!("{base}principals/users/delegate/");
    client
        .revoke_calendar_proxy("principals/users/test/", &delegate, false)
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2, "PROPFIND then ACL");
    let acl = String::from_utf8_lossy(&captured[1]);
    assert!(
        !acl.contains("/principals/users/delegate/"),
        "the revoked delegate must be absent from the ACL body: {acl}"
    );
    assert!(
        acl.contains("/principals/users/other/"),
        "the other group member must keep its grant: {acl}"
    );
    assert!(
        !acl.contains("http://"),
        "members must be re-issued in the server's path form: {acl}"
    );
}

#[tokio::test]
async fn revoke_non_member_fails_without_sending_acl() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![(response_head("", body.len()), body)]).await;
    let client = make_client(&base);

    let err = client
        .revoke_calendar_proxy(
            "principals/users/test/",
            "/principals/users/stranger/",
            false,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "revoking a non-member must fail with InvalidInput, got: {err:?}"
    );

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "only the member PROPFIND is sent");
    assert!(
        String::from_utf8_lossy(&captured[0]).starts_with("PROPFIND "),
        "the single captured request must be the member PROPFIND"
    );
}

#[tokio::test]
async fn grant_calendar_proxy_normalizes_delegate_href_form() {
    let body = group_member_set_body();
    let (base, captured) = serve_sequence(vec![
        (response_head("", body.len()), body),
        (response_head("", 0), Vec::new()),
    ])
    .await;
    let client = make_client(&base);

    // The delegate is already a member in path form; granting through the
    // absolute-URL spelling must reuse the server's href, not emit a second
    // ACE for the same principal.
    let delegate = format!("{base}principals/users/delegate/");
    client
        .grant_calendar_proxy("principals/users/test/", &delegate, false)
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2, "PROPFIND then ACL");
    let acl = String::from_utf8_lossy(&captured[1]);
    assert_eq!(
        acl.matches("/principals/users/delegate/").count(),
        1,
        "the delegate must appear in exactly one ACE: {acl}"
    );
    assert!(
        !acl.contains("http://"),
        "the delegate ACE must reuse the server's path form: {acl}"
    );
    assert!(
        acl.contains("/principals/users/other/"),
        "the other group member must keep its grant: {acl}"
    );
}
