//! Wire tests for RFC 6638 scheduling support (issue #171): schedule
//! endpoint discovery, outbox `POST`, schedule-inbox listing, and
//! `If-Schedule-Tag-Match` conditional writes.

use bytes::Bytes;
use fast_dav_rs::{CalDavClient, RequestCompressionMode};

use crate::common::http_helpers::{response_head, serve_capture, serve_once};

const DISCOVERY_MULTISTATUS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/principals/test/</D:href>
    <D:propstat>
      <D:prop>
        <C:schedule-inbox-URL><D:href>/calendars/inbox/</D:href></C:schedule-inbox-URL>
        <C:schedule-outbox-URL><D:href>/calendars/outbox/</D:href></C:schedule-outbox-URL>
        <C:calendar-user-address-set>
          <D:href>mailto:me@example.com</D:href>
        </C:calendar-user-address-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

fn multistatus_response(body: &str) -> (String, Vec<u8>) {
    (
        format!(
            "HTTP/1.1 207 Multi-Status\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        ),
        body.as_bytes().to_vec(),
    )
}

fn make_caldav_client(base: &str) -> CalDavClient {
    let client = CalDavClient::new(base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

#[tokio::test]
async fn discover_schedule_endpoints_parses_schedule_props() {
    let (head, body) = multistatus_response(DISCOVERY_MULTISTATUS);
    let base = serve_once(head, body).await;
    let client = make_caldav_client(&base);

    let endpoints = client
        .discover_schedule_endpoints("principals/test/")
        .await
        .unwrap();
    assert_eq!(endpoints.inbox.as_deref(), Some("/calendars/inbox/"));
    assert_eq!(endpoints.outbox.as_deref(), Some("/calendars/outbox/"));
    assert_eq!(endpoints.user_addresses, vec!["mailto:me@example.com"]);
}

const OUTBOX_ICS: &[u8] = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nBEGIN:VFREEBUSY\r\nDTSTAMP:20260101T000000Z\r\nDTSTART:20260101T000000Z\r\nDTEND:20260102T000000Z\r\nEND:VFREEBUSY\r\nEND:VCALENDAR\r\n";

const OUTBOX_SUCCESS_BODY: &[u8] =
    b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nREQUEST-STATUS:2.0;Success\r\nEND:VCALENDAR\r\n";

const INBOX_MULTISTATUS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/calendars/inbox/</D:href>
    <D:propstat>
      <D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/calendars/inbox/invite-1.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-invite-1"</D:getetag>
        <C:calendar-data>BEGIN:VCALENDAR&#13;&#10;VERSION:2.0&#13;&#10;END:VCALENDAR</C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

#[tokio::test]
async fn post_schedule_returns_raw_success_response() {
    let body = OUTBOX_SUCCESS_BODY.to_vec();
    let head = response_head("Content-Type: text/calendar; charset=utf-8\r\n", body.len());
    let (base, captured) = serve_capture(head, body).await;
    let client = make_caldav_client(&base);

    let resp = client
        .post_schedule("outbox/", Bytes::from_static(OUTBOX_ICS))
        .await
        .unwrap();
    assert_eq!(resp.status.as_u16(), 200);
    assert!(String::from_utf8_lossy(&resp.body).contains("REQUEST-STATUS:2.0;Success"));

    let request = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(request.starts_with("POST "));
    assert!(request.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && value.trim().contains("text/calendar")
        })
    }));
    assert!(request.contains("BEGIN:VFREEBUSY"));
}

#[tokio::test]
async fn post_schedule_non_success_maps_to_unexpected_status() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client
        .post_schedule("outbox/", Bytes::from_static(OUTBOX_ICS))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            fast_dav_rs::Error::UnexpectedStatus {
                operation: fast_dav_rs::Operation::PostSchedule,
                ..
            }
        ),
        "expected UnexpectedStatus(PostSchedule), got {err:?}"
    );
}

#[tokio::test]
async fn list_inbox_parses_etag_and_calendar_data() {
    let (head, body) = multistatus_response(INBOX_MULTISTATUS);
    let base = serve_once(head, body).await;
    let client = make_caldav_client(&base);

    let items = client.list_inbox("calendars/inbox/").await.unwrap();
    assert_eq!(items.len(), 1, "collection entry must be skipped");
    assert_eq!(items[0].href, "/calendars/inbox/invite-1.ics");
    assert_eq!(items[0].etag.as_deref(), Some("etag-invite-1"));
    assert!(
        items[0]
            .data
            .as_deref()
            .is_some_and(|d| d.contains("BEGIN:VCALENDAR"))
    );
}

#[tokio::test]
async fn list_inbox_non_success_maps_to_unexpected_status() {
    let base = serve_once(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);

    let err = client.list_inbox("calendars/inbox/").await.unwrap_err();
    assert!(
        matches!(
            err,
            fast_dav_rs::Error::UnexpectedStatus {
                operation: fast_dav_rs::Operation::ScheduleInbox,
                ..
            }
        ),
        "expected UnexpectedStatus(ScheduleInbox), got {err:?}"
    );
}

fn assert_if_schedule_tag_header(request: &str, expected: &str) {
    assert!(request.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("if-schedule-tag-match") && value.trim() == expected
        })
    }));
}

#[tokio::test]
async fn put_if_schedule_tag_sends_quoted_header() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = make_caldav_client(&base);

    let resp = client
        .put_if_schedule_tag("cal/event.ics", Bytes::from_static(OUTBOX_ICS), "abc123")
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let request = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(request.starts_with("PUT "));
    assert_if_schedule_tag_header(&request, "\"abc123\"");
    assert!(request.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && value.trim().contains("text/calendar")
        })
    }));
}

#[tokio::test]
async fn delete_if_schedule_tag_sends_quoted_header() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = make_caldav_client(&base);

    let resp = client
        .delete_if_schedule_tag("cal/event.ics", "abc123")
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let request = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(request.starts_with("DELETE "));
    assert_if_schedule_tag_header(&request, "\"abc123\"");
}

#[tokio::test]
async fn schedule_tag_writes_pass_non_success_status_through() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);
    let resp = client
        .put_if_schedule_tag("cal/event.ics", Bytes::from_static(OUTBOX_ICS), "abc123")
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 412);

    let (base, _captured) = serve_capture(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_caldav_client(&base);
    let resp = client
        .delete_if_schedule_tag("cal/event.ics", "abc123")
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn schedule_tag_rejects_empty_tag_before_io() {
    // Unroutable port: any network I/O would surface as a transport error
    // instead of the expected InvalidInput.
    let client = CalDavClient::new("http://127.0.0.1:9/", None, None).unwrap();

    for tag in ["", "   "] {
        let err = client
            .put_if_schedule_tag("cal/event.ics", Bytes::from_static(OUTBOX_ICS), tag)
            .await
            .unwrap_err();
        assert!(
            matches!(err, fast_dav_rs::Error::InvalidInput(_)),
            "expected InvalidInput for PUT, got {err:?}"
        );
        let err = client
            .delete_if_schedule_tag("cal/event.ics", tag)
            .await
            .unwrap_err();
        assert!(
            matches!(err, fast_dav_rs::Error::InvalidInput(_)),
            "expected InvalidInput for DELETE, got {err:?}"
        );
    }
}
