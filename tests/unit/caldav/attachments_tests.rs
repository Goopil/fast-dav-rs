//! Wire tests for managed attachments (RFC 8607, issue #172): the
//! `attachment-add` POST query, `Location` / `Cal-Managed-ID` response
//! handling, and the no-managed-id error path.

use bytes::Bytes;
use fast_dav_rs::{CalDavClient, RequestCompressionMode};

use crate::common::http_helpers::serve_capture;

fn make_caldav_client(base: &str) -> CalDavClient {
    let client = CalDavClient::new(base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

const ATTACHMENT_BODY: &[u8] = b"attachment-bytes";

#[tokio::test]
async fn post_managed_attachment_returns_href_and_managed_id_from_headers() {
    let head = "HTTP/1.1 201 Created\r\nLocation: /calendars/c/uid-att.bin\r\nCal-Managed-ID: mid-42\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
    let (base, captured) = serve_capture(head, Vec::new()).await;
    let client = make_caldav_client(&base);

    let att = client
        .post_managed_attachment(
            "c/",
            "uid-123",
            None,
            Bytes::from_static(ATTACHMENT_BODY),
            "application/pdf",
        )
        .await
        .unwrap();
    assert_eq!(att.href, "/calendars/c/uid-att.bin");
    assert_eq!(att.managed_id, "mid-42");

    let request = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        request.starts_with("POST /c/?action=attachment-add&uid=uid-123 HTTP/1.1"),
        "unexpected request line: {}",
        request.lines().next().unwrap_or_default()
    );
    assert!(
        !request.contains("recurrence-id"),
        "recurrence-id must be absent when None"
    );
    assert!(
        request.lines().any(|line| {
            line.split_once(':').is_some_and(|(n, v)| {
                n.eq_ignore_ascii_case("content-type") && v.trim() == "application/pdf"
            })
        }),
        "attachment content type must be sent verbatim"
    );
    assert!(request.contains("attachment-bytes"), "body must be sent");
}

#[tokio::test]
async fn post_managed_attachment_percent_encodes_uid_and_sends_recurrence_id() {
    let head = "HTTP/1.1 201 Created\r\nLocation: /calendars/c/att.bin\r\nCal-Managed-ID: mid-43\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
    let (base, captured) = serve_capture(head, Vec::new()).await;
    let client = make_caldav_client(&base);

    client
        .post_managed_attachment(
            "c/",
            "uid with spaces&co",
            Some("20260601T100000Z"),
            Bytes::from_static(ATTACHMENT_BODY),
            "text/plain",
        )
        .await
        .unwrap();

    let request = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        request.starts_with(
            "POST /c/?action=attachment-add&uid=uid%20with%20spaces%26co&recurrence-id=20260601T100000Z HTTP/1.1"
        ),
        "unexpected request line: {}",
        request.lines().next().unwrap_or_default()
    );
}

#[tokio::test]
async fn post_managed_attachment_extracts_managed_id_from_location_query() {
    let head = "HTTP/1.1 201 Created\r\nLocation: /calendars/c/att.bin?managed-id=loc-mid\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
    let (base, _captured) = serve_capture(head, Vec::new()).await;
    let client = make_caldav_client(&base);

    let att = client
        .post_managed_attachment(
            "c/",
            "uid-123",
            None,
            Bytes::from_static(ATTACHMENT_BODY),
            "text/plain",
        )
        .await
        .unwrap();
    // The Location is returned verbatim (opaque resource URI); the managed
    // id is extracted from its query parameter separately.
    assert_eq!(att.href, "/calendars/c/att.bin?managed-id=loc-mid");
    assert_eq!(att.managed_id, "loc-mid");
}

#[tokio::test]
async fn post_managed_attachment_fails_without_any_managed_id() {
    let head = "HTTP/1.1 201 Created\r\nLocation: /calendars/c/att.bin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
    let (base, _captured) = serve_capture(head, Vec::new()).await;
    let client = make_caldav_client(&base);

    let err = client
        .post_managed_attachment(
            "c/",
            "uid-123",
            None,
            Bytes::from_static(ATTACHMENT_BODY),
            "text/plain",
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("no managed id"),
        "expected a no-managed-id error, got {err:?}"
    );
}

#[tokio::test]
async fn post_managed_attachment_non_success_maps_to_unexpected_status() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .post_managed_attachment(
            "c/",
            "uid-123",
            None,
            Bytes::from_static(ATTACHMENT_BODY),
            "text/plain",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            fast_dav_rs::Error::UnexpectedStatus {
                operation: fast_dav_rs::Operation::PostManagedAttachment,
                ..
            }
        ),
        "expected UnexpectedStatus(PostManagedAttachment), got {err:?}"
    );
}
