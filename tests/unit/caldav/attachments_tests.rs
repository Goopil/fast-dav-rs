//! Wire tests for managed attachments (RFC 8607, issue #172): the
//! `attachment-add` POST query, `Location` / `Cal-Managed-ID` response
//! handling, the no-managed-id error path, and the update/removal methods
//! (`Cal-Managed-ID` request header, RFC 8607 §5.2/§5.3, issue #249).

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
    // No captured-request interpolation in the failure message (CodeQL:
    // uids must not reach test logs); the pattern itself is the diagnostic.
    assert!(request.starts_with("POST /c/?action=attachment-add&uid=uid-123 HTTP/1.1"));
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
        )
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

// --- update / removal (RFC 8607 §5.2/§5.3, issue #249) ---

/// RFC 8607 §5.2: a managed-attachment update is `PUT` on the attachment
/// resource with `Cal-Managed-ID` (authorization to overwrite) and
/// `Content-Type`.
#[tokio::test]
async fn put_managed_attachment_sends_method_managed_id_and_content_type() {
    let head =
        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
    let (base, captured) = serve_capture(head, Vec::new()).await;
    let client = make_caldav_client(&base);

    let resp = client
        .put_managed_attachment(
            "/calendars/c/uid-att.bin",
            ATTACHMENT_BODY,
            "text/plain",
            "mid-1",
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let request = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        request.starts_with("PUT /calendars/c/uid-att.bin HTTP/1.1"),
        "expected PUT on the attachment resource"
    );
    assert!(
        request.lines().any(|line| {
            line.split_once(':').is_some_and(|(n, v)| {
                n.eq_ignore_ascii_case("cal-managed-id") && v.trim() == "mid-1"
            })
        }),
        "Cal-Managed-ID header missing"
    );
    assert!(
        request.lines().any(|line| {
            line.split_once(':').is_some_and(|(n, v)| {
                n.eq_ignore_ascii_case("content-type") && v.trim() == "text/plain"
            })
        }),
        "Content-Type header missing"
    );
    assert!(request.contains("attachment-bytes"), "body must be sent");
}

/// RFC 8607 §5.3: a managed-attachment removal is `DELETE` on the attachment
/// resource with `Cal-Managed-ID`.
#[tokio::test]
async fn delete_managed_attachment_sends_method_and_managed_id() {
    let head =
        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
    let (base, captured) = serve_capture(head, Vec::new()).await;
    let client = make_caldav_client(&base);

    let resp = client
        .delete_managed_attachment("/calendars/c/uid-att.bin", "mid-1")
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let request = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        request.starts_with("DELETE /calendars/c/uid-att.bin HTTP/1.1"),
        "expected DELETE on the attachment resource"
    );
    assert!(
        request.lines().any(|line| {
            line.split_once(':').is_some_and(|(n, v)| {
                n.eq_ignore_ascii_case("cal-managed-id") && v.trim() == "mid-1"
            })
        }),
        "Cal-Managed-ID header missing"
    );
}

#[tokio::test]
async fn put_managed_attachment_non_success_maps_to_unexpected_status() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .put_managed_attachment(
            "/calendars/c/uid-att.bin",
            ATTACHMENT_BODY,
            "text/plain",
            "mid-1",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            fast_dav_rs::Error::UnexpectedStatus {
                operation: fast_dav_rs::Operation::PutManagedAttachment,
                ..
            }
        ),
        "expected UnexpectedStatus(PutManagedAttachment), got {err:?}"
    );
}

#[tokio::test]
async fn delete_managed_attachment_non_success_maps_to_unexpected_status() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .delete_managed_attachment("/calendars/c/uid-att.bin", "mid-1")
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            fast_dav_rs::Error::UnexpectedStatus {
                operation: fast_dav_rs::Operation::DeleteManagedAttachment,
                ..
            }
        ),
        "expected UnexpectedStatus(DeleteManagedAttachment), got {err:?}"
    );
}

/// Empty `managed_id` is rejected before any network I/O (no request
/// captured on the wire mock).
#[tokio::test]
async fn put_managed_attachment_empty_managed_id_is_invalid_input_before_io() {
    let (base, captured) = serve_capture(
        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .put_managed_attachment(
            "/calendars/c/uid-att.bin",
            ATTACHMENT_BODY,
            "text/plain",
            "",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "expected InvalidInput for empty managed id, got {err:?}"
    );
    assert!(
        captured.lock().unwrap().is_empty(),
        "no request may reach the wire on pre-I/O validation failure"
    );
}

#[tokio::test]
async fn put_managed_attachment_empty_href_is_invalid_input_before_io() {
    let (base, captured) = serve_capture(
        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .put_managed_attachment("", ATTACHMENT_BODY, "text/plain", "mid-1")
        .await
        .unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "expected InvalidInput for empty href, got {err:?}"
    );
    assert!(
        captured.lock().unwrap().is_empty(),
        "no request may reach the wire on pre-I/O validation failure"
    );
}

#[tokio::test]
async fn put_managed_attachment_empty_content_type_is_invalid_input_before_io() {
    let (base, captured) = serve_capture(
        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .put_managed_attachment("/calendars/c/uid-att.bin", ATTACHMENT_BODY, "", "mid-1")
        .await
        .unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "expected InvalidInput for empty content type, got {err:?}"
    );
    assert!(
        captured.lock().unwrap().is_empty(),
        "no request may reach the wire on pre-I/O validation failure"
    );
}

#[tokio::test]
async fn delete_managed_attachment_empty_href_is_invalid_input_before_io() {
    let (base, captured) = serve_capture(
        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .delete_managed_attachment("", "mid-1")
        .await
        .unwrap_err();
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "expected InvalidInput for empty href, got {err:?}"
    );
    assert!(
        captured.lock().unwrap().is_empty(),
        "no request may reach the wire on pre-I/O validation failure"
    );
}
