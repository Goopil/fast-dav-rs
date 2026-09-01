use bytes::Bytes;
use fast_dav_rs::caldav::{ICalendarViolation, ValidationLevel};
use fast_dav_rs::{CalDavClient, Error, RequestCompressionMode};

/// Minimal valid iCalendar body with a `VEVENT`.
const VALID_ICAL: &[u8] = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nBEGIN:VEVENT\r\nUID:evt-1@example\r\nDTSTAMP:20260101T000000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

fn violation_of(body: &[u8]) -> ICalendarViolation {
    match fast_dav_rs::caldav::validate_icalendar(body) {
        Ok(()) => panic!(
            "expected validation to fail for {:?}",
            String::from_utf8_lossy(body)
        ),
        Err(Error::InvalidICalendar { violation, .. }) => violation,
        Err(other) => panic!("expected InvalidICalendar, got: {other:?}"),
    }
}

#[test]
fn accepts_minimal_valid_icalendar() {
    assert!(fast_dav_rs::caldav::validate_icalendar(VALID_ICAL).is_ok());
}

#[test]
fn accepts_lf_only_and_lowercase_names() {
    let body = b"begin:vcalendar\nversion:2.0\nprodid:-//t//en\nbegin:vevent\nuid:x@y\nend:vevent\nend:vcalendar\n";
    assert!(fast_dav_rs::caldav::validate_icalendar(body).is_ok());
}

#[test]
fn accepts_vtodo_with_uid_and_nested_valarm() {
    let body = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\n\
        BEGIN:VTODO\r\nUID:todo-1\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
    assert!(fast_dav_rs::caldav::validate_icalendar(body).is_ok());
}

#[test]
fn rejects_invalid_utf8() {
    assert_eq!(violation_of(b"\xFF\xFE\xFD"), ICalendarViolation::NotUtf8);
}

#[test]
fn rejects_empty_body_as_missing_begin() {
    assert_eq!(violation_of(b""), ICalendarViolation::MissingBegin);
}

#[test]
fn rejects_body_not_starting_with_vcalendar() {
    assert_eq!(
        violation_of(b"VERSION:2.0\r\nBEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"),
        ICalendarViolation::MissingBegin
    );
}

#[test]
fn rejects_missing_end_vcalendar() {
    assert_eq!(
        violation_of(b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\n"),
        ICalendarViolation::MissingEnd
    );
}

#[test]
fn rejects_missing_version() {
    assert_eq!(
        violation_of(b"BEGIN:VCALENDAR\r\nPRODID:-//t//EN\r\nEND:VCALENDAR\r\n"),
        ICalendarViolation::MissingVersion
    );
}

#[test]
fn rejects_unsupported_version() {
    assert_eq!(
        violation_of(b"BEGIN:VCALENDAR\r\nVERSION:1.0\r\nPRODID:-//t//EN\r\nEND:VCALENDAR\r\n"),
        ICalendarViolation::UnsupportedVersion
    );
}

#[test]
fn rejects_missing_prodid() {
    assert_eq!(
        violation_of(b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n"),
        ICalendarViolation::MissingProdId
    );
}

#[test]
fn rejects_unclosed_component() {
    let body = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\nBEGIN:VEVENT\r\nUID:1\r\nEND:VCALENDAR\r\n";
    assert_eq!(violation_of(body), ICalendarViolation::UnbalancedComponents);
}

#[test]
fn rejects_mismatched_end_name() {
    let body = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\nBEGIN:VEVENT\r\nUID:1\r\nEND:VJOURNAL\r\nEND:VCALENDAR\r\n";
    assert_eq!(violation_of(body), ICalendarViolation::UnbalancedComponents);
}

#[test]
fn rejects_trailing_content_after_vcalendar_end() {
    let body =
        b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\nEND:VCALENDAR\r\nBEGIN:VEVENT\r\n";
    assert_eq!(violation_of(body), ICalendarViolation::MissingEnd);
}

#[test]
fn rejects_vevent_without_uid() {
    let body = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\nBEGIN:VEVENT\r\nDTSTAMP:20260101T000000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    assert_eq!(violation_of(body), ICalendarViolation::MissingUid);
}

#[test]
fn rejects_vtodo_without_uid() {
    let body = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\nBEGIN:VTODO\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
    assert_eq!(violation_of(body), ICalendarViolation::MissingUid);
}

#[test]
fn uid_inside_valarm_does_not_satisfy_vevent_uid() {
    let body = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\n\
        BEGIN:VEVENT\r\nBEGIN:VALARM\r\nUID:alarm-1\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    assert_eq!(violation_of(body), ICalendarViolation::MissingUid);
}

#[test]
fn non_property_lines_without_colon_are_ignored() {
    // A stray line with no `:` (and no BEGIN/END) must not break the scan.
    let body = b"BEGIN:VCALENDAR\r\nSTRAY FOLDED CONTINUATION\r\nVERSION:2.0\r\n\
        PRODID:-//t//EN\r\nBEGIN:VEVENT\r\nUID:x@y\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    assert!(fast_dav_rs::caldav::validate_icalendar(body).is_ok());
}

#[test]
fn violation_display_covers_every_violation() {
    // Each violation-producing body, with the variant it must yield.
    let cases: &[(&[u8], ICalendarViolation)] = &[
        (b"\xFF\xFE", ICalendarViolation::NotUtf8),
        (b"", ICalendarViolation::MissingBegin),
        (
            b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\n",
            ICalendarViolation::MissingEnd,
        ),
        (
            b"BEGIN:VCALENDAR\r\nPRODID:-//t//EN\r\nEND:VCALENDAR\r\n",
            ICalendarViolation::MissingVersion,
        ),
        (
            b"BEGIN:VCALENDAR\r\nVERSION:1.0\r\nPRODID:-//t//EN\r\nEND:VCALENDAR\r\n",
            ICalendarViolation::UnsupportedVersion,
        ),
        (
            b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n",
            ICalendarViolation::MissingProdId,
        ),
        (
            b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\n\
             BEGIN:VEVENT\r\nUID:1\r\nEND:VJOURNAL\r\nEND:VCALENDAR\r\n",
            ICalendarViolation::UnbalancedComponents,
        ),
        (
            b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\n\
             BEGIN:VEVENT\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            ICalendarViolation::MissingUid,
        ),
    ];

    for (body, expected) in cases {
        let err = fast_dav_rs::caldav::validate_icalendar(body).unwrap_err();
        assert!(
            matches!(err, Error::InvalidICalendar { violation, .. } if violation == *expected),
            "expected {expected:?} for {body:?}, got: {err:?}"
        );
        // Calling `Display` covers the message arm for each violation.
        assert!(
            err.to_string().starts_with("invalid iCalendar: "),
            "display should describe the violation: {err}"
        );
    }
}

fn ical_client(base: &str, level: ValidationLevel) -> CalDavClient {
    let client = CalDavClient::builder(base)
        .validation_level(level)
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

#[tokio::test]
async fn put_rejects_invalid_body_before_any_network_io() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    // Compression left on Auto: even the request-compression probe is network
    // I/O, so a clean capture proves validation fired before *any* I/O.
    let client = CalDavClient::new(&base, None, None).unwrap();

    let err = client
        .put("event.ics", Bytes::from_static(b"not an icalendar body"))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            Error::InvalidICalendar {
                violation: ICalendarViolation::MissingBegin,
                ..
            }
        ),
        "expected InvalidICalendar(MissingBegin), got: {err:?}"
    );
    assert!(
        captured.lock().unwrap().is_empty(),
        "no request bytes may be sent when validation fails"
    );
}

#[tokio::test]
async fn put_if_match_rejects_invalid_body_before_sending() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::Structural);

    let err = client
        .put_if_match(
            "event.ics",
            Bytes::from_static(b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"),
            "etag-1",
        )
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            Error::InvalidICalendar {
                violation: ICalendarViolation::MissingVersion,
                ..
            }
        ),
        "expected InvalidICalendar(MissingVersion), got: {err:?}"
    );
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn put_if_none_match_rejects_invalid_body_before_sending() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::Structural);

    let err = client
        .put_if_none_match("event.ics", Bytes::from_static(b"\xFF\xFE"))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            Error::InvalidICalendar {
                violation: ICalendarViolation::NotUtf8,
                ..
            }
        ),
        "expected InvalidICalendar(NotUtf8), got: {err:?}"
    );
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn put_sends_version_parameter_for_valid_body() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::Structural);

    client
        .put("event.ics", Bytes::from_static(VALID_ICAL))
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase()
            .contains("content-type: text/calendar; charset=utf-8; version=2.0"),
        "expected version parameter on the wire: {req}"
    );
}

#[tokio::test]
async fn put_if_match_sends_version_parameter_and_if_match() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::Structural);

    client
        .put_if_match("event.ics", Bytes::from_static(VALID_ICAL), "\"etag-1\"")
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase()
            .contains("content-type: text/calendar; charset=utf-8; version=2.0"),
        "expected version parameter on the wire: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("if-match: \"etag-1\""),
        "expected If-Match guard: {req}"
    );
}

#[tokio::test]
async fn put_if_none_match_sends_version_parameter_and_if_none_match() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::Structural);

    client
        .put_if_none_match("event.ics", Bytes::from_static(VALID_ICAL))
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase()
            .contains("content-type: text/calendar; charset=utf-8; version=2.0"),
        "expected version parameter on the wire: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("if-none-match: *"),
        "expected If-None-Match: * guard: {req}"
    );
}

#[tokio::test]
async fn structural_level_does_not_require_uid() {
    let no_uid = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\nBEGIN:VEVENT\r\nDTSTAMP:20260101T000000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::Structural);

    client
        .put("event.ics", Bytes::from_static(no_uid))
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(req.starts_with("PUT "), "request must be sent: {req}");
}

#[tokio::test]
async fn strict_level_rejects_missing_uid_before_sending() {
    let no_uid = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\nBEGIN:VEVENT\r\nDTSTAMP:20260101T000000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::Strict);

    let err = client
        .put("event.ics", Bytes::from_static(no_uid))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            Error::InvalidICalendar {
                violation: ICalendarViolation::MissingUid,
                ..
            }
        ),
        "expected InvalidICalendar(MissingUid), got: {err:?}"
    );
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn validation_none_restores_passthrough_behavior() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = ical_client(&base, ValidationLevel::None);

    // A body that fails every structural check must still be sent unvalidated.
    client
        .put("event.ics", Bytes::from_static(b"total garbage \xFF"))
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(req.starts_with("PUT "), "request must be sent: {req}");
    assert!(
        req.to_ascii_lowercase()
            .contains("content-type: text/calendar; charset=utf-8\r\n"),
        "plain Content-Type without version parameter expected: {req}"
    );
    assert!(
        !req.to_ascii_lowercase().contains("version="),
        "no version parameter expected when validation is off: {req}"
    );
}
