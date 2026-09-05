//! Wire tests for [`CalDavClient::calendar_timezone`] (issue #173): a
//! `Depth: 0` calendar `PROPFIND` for `calendar-timezone` (RFC 4791 §5.2.2)
//! mapped to `Option<String>` — and for [`CalDavClient::set_calendar_timezone`]
//! (issue #187): a `Depth: 0` `PROPPATCH` setting/removing the same property.

use fast_dav_rs::{CalDavClient, Error, Operation, RequestCompressionMode};

use crate::common::http_helpers::{response_head, serve_capture, serve_once, unreachable_base};

const TIMEZONE_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-timezone><![CDATA[BEGIN:VCALENDAR
BEGIN:VTIMEZONE
TZID:Europe/Paris
END:VTIMEZONE
END:VCALENDAR]]></C:calendar-timezone>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

const NO_TIMEZONE_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop/>
      <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

#[tokio::test]
async fn calendar_timezone_returns_prop_and_sends_depth_zero_propfind() {
    let (base, captured) = serve_capture(
        response_head("", TIMEZONE_BODY.len()),
        TIMEZONE_BODY.as_bytes().to_vec(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let tz = client.calendar_timezone("cal/").await.unwrap();

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("PROPFIND"),
        "expected PROPFIND method in request: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected 'Depth: 0' in request: {req}"
    );
    assert!(
        req.contains("<C:calendar-timezone/>"),
        "calendar-timezone prop request missing: {req}"
    );
    assert_eq!(
        tz.as_deref(),
        Some("BEGIN:VCALENDAR\nBEGIN:VTIMEZONE\nTZID:Europe/Paris\nEND:VTIMEZONE\nEND:VCALENDAR"),
        "multi-line iCalendar content must be preserved verbatim"
    );
}

#[tokio::test]
async fn calendar_timezone_absent_prop_returns_none() {
    let (base, captured) = serve_capture(
        response_head("", NO_TIMEZONE_BODY.len()),
        NO_TIMEZONE_BODY.as_bytes().to_vec(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let tz = client.calendar_timezone("cal/").await.unwrap();

    assert_eq!(tz, None);
    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("<C:calendar-timezone/>"),
        "calendar-timezone prop request missing: {req}"
    );
}

#[tokio::test]
async fn calendar_timezone_non_success_maps_unexpected_status() {
    let head =
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
    let base = serve_once(head, Vec::new()).await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = client.calendar_timezone("cal/").await.unwrap_err();

    assert!(
        matches!(
            &err,
            Error::UnexpectedStatus { operation, .. }
                if *operation == Operation::PropfindCalendarTimezone
        ),
        "expected UnexpectedStatus(PropfindCalendarTimezone), got {err:?}"
    );
}

const PROPPATCH_OK_HEAD: &str =
    "HTTP/1.1 207 Multi-Status\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n";

fn proppatch_207_body(status_line: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-timezone/>
      </D:prop>
      <D:status>{status_line}</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
    )
}

#[tokio::test]
async fn set_calendar_timezone_sends_proppatch_with_escaped_value() {
    let body = proppatch_207_body("HTTP/1.1 200 OK");
    let head = PROPPATCH_OK_HEAD.replace("{len}", &body.len().to_string());
    let (base, captured) = serve_capture(head, body.into_bytes()).await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let vtz = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Paris\r\n\
               X-NOTE:<tz & notes>\r\nEND:VTIMEZONE\r\nEND:VCALENDAR";
    client
        .set_calendar_timezone("cal/", Some(vtz))
        .await
        .unwrap();

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("PROPPATCH"),
        "expected PROPPATCH method in request: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected 'Depth: 0' in request: {req}"
    );
    assert!(
        req.contains("<D:propertyupdate") && req.contains("<D:set>"),
        "expected a propertyupdate with a set directive: {req}"
    );
    assert!(
        req.contains("TZID:Europe/Paris"),
        "expected the VTIMEZONE content verbatim: {req}"
    );
    assert!(
        req.contains("X-NOTE:&lt;tz &amp; notes&gt;"),
        "expected the XML-escaped VTIMEZONE: {req}"
    );
}

#[tokio::test]
async fn set_calendar_timezone_none_sends_remove() {
    let body = proppatch_207_body("HTTP/1.1 200 OK");
    let head = PROPPATCH_OK_HEAD.replace("{len}", &body.len().to_string());
    let (base, captured) = serve_capture(head, body.into_bytes()).await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client.set_calendar_timezone("cal/", None).await.unwrap();

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("PROPPATCH"),
        "expected PROPPATCH method in request: {req}"
    );
    assert!(
        req.contains("<D:remove>"),
        "expected a remove directive in the propertyupdate: {req}"
    );
    assert!(
        req.contains("<C:calendar-timezone/>"),
        "expected an empty calendar-timezone element in the remove: {req}"
    );
}

#[tokio::test]
async fn set_calendar_timezone_propstat_failure_maps_to_unexpected_status() {
    let body = proppatch_207_body("HTTP/1.1 403 Forbidden");
    let head = PROPPATCH_OK_HEAD.replace("{len}", &body.len().to_string());
    let base = serve_capture(head, body.into_bytes()).await.0;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = client
        .set_calendar_timezone("cal/", Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR"))
        .await
        .unwrap_err();

    assert!(
        matches!(
            &err,
            Error::UnexpectedStatus { operation, status, .. }
                if *operation == Operation::ProppatchCalendarTimezone && status.as_u16() == 403
        ),
        "expected UnexpectedStatus(ProppatchCalendarTimezone, 403), got {err:?}"
    );
}

#[tokio::test]
async fn set_calendar_timezone_rejects_empty_value() {
    let base = unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let err = client
        .set_calendar_timezone("cal/", Some("   "))
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::InvalidInput(_)),
        "expected InvalidInput for an empty value, got {err:?}"
    );
}
