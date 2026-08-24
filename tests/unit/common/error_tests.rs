use fast_dav_rs::{CalDavClient, Error, EtagReason, Operation, Result, WebDavClient};
use hyper::StatusCode;
use hyper::{HeaderMap, Method};
use std::error::Error as _;
use std::time::Duration;

#[test]
fn invalid_url_preserves_the_offending_value() {
    let error = CalDavClient::new("not a valid url", None, None)
        .err()
        .expect("an invalid URL should be rejected");

    assert!(matches!(
        error,
        Error::InvalidUrl { url, .. } if url == "not a valid url"
    ));
}

#[tokio::test]
async fn invalid_etag_is_a_typed_input_error() {
    let client = CalDavClient::new("http://localhost/", None, None).unwrap();
    let error = client
        .put_if_match("event.ics", bytes::Bytes::new(), "")
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Error::InvalidEtag {
            reason: EtagReason::Empty,
            ..
        }
    ));
}

#[tokio::test]
async fn invalid_calendar_component_is_a_typed_input_error() {
    let client = CalDavClient::new("http://localhost/", None, None).unwrap();
    let error = client
        .calendar_query_timerange("calendar/", "VEVENT/INVALID", None, None, false)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Error::InvalidComponentName { name, bad_char: Some('/'), .. } if name == "VEVENT/INVALID"
    ));
}

#[test]
fn public_error_variants_expose_retry_relevant_context() {
    let status_error =
        Error::unexpected_status(Operation::PropfindCollections, StatusCode::FORBIDDEN);
    assert_eq!(
        status_error.to_string(),
        "PROPFIND collections failed with 403 Forbidden"
    );

    let timeout_error = Error::timeout(Duration::from_secs(20));
    assert_eq!(timeout_error.to_string(), "operation timed out after 20s");
}

#[test]
fn standard_error_conversions_remain_typed() {
    let io_error: Error = std::io::Error::other("broken pipe").into();
    assert!(matches!(io_error, Error::Io(_)));

    let result: Result<()> = Err(Error::other("application callback failed"));
    assert!(
        matches!(result, Err(Error::Other { context, .. }) if context == "application callback failed")
    );
}

#[test]
fn from_invalid_header_value() {
    let error: Error = hyper::header::HeaderValue::from_str("invalid\n")
        .unwrap_err()
        .into();
    assert!(matches!(error, Error::InvalidHeader(_)));
}

#[test]
fn from_invalid_method() {
    let error: Error = hyper::http::Method::from_bytes(b"INVALID METHOD")
        .unwrap_err()
        .into();
    assert!(matches!(error, Error::InvalidMethod(_)));
}

#[test]
fn from_http_error() {
    // hyper::http::Error::from(InvalidUriParts) is the most direct path.
    // InvalidUriParts is produced when parts are inconsistent (e.g. scheme
    // set but authority missing for an absolute URI).
    let mut parts = hyper::http::uri::Parts::default();
    parts.scheme = Some("http".parse().unwrap());
    // No authority — this is invalid for an absolute URI.
    let invalid_uri = hyper::http::uri::Uri::from_parts(parts).unwrap_err();
    let http_err: hyper::http::Error = invalid_uri.into();
    let error: Error = http_err.into();
    assert!(matches!(error, Error::Http(_)));
}

// NOTE: hyper::Error (the Hyper variant) cannot be unit-tested in isolation
// because all its constructors are pub(super). It is exercised by the
// integration and streaming tests that perform real HTTP I/O.

#[test]
fn from_quick_xml_error() {
    let escape_error = quick_xml::escape::EscapeError::UnrecognizedEntity(0..3, "foo".into());
    let xml_error: quick_xml::Error = escape_error.into();
    let error: Error = xml_error.into();
    assert!(matches!(error, Error::Xml(_)));
}

#[test]
fn from_xml_attribute_error() {
    let attr_error = quick_xml::events::attributes::AttrError::ExpectedEq(0);
    let error: Error = attr_error.into();
    assert!(matches!(error, Error::XmlAttribute(_)));
}

#[test]
fn from_xml_escape_error() {
    let escape_error = quick_xml::escape::EscapeError::UnrecognizedEntity(0..3, "foo".into());
    let error: Error = escape_error.into();
    assert!(matches!(error, Error::XmlEscape(_)));
}

#[test]
fn from_utf8_error() {
    #![allow(invalid_from_utf8)]
    let utf8_error = std::str::from_utf8(&[0xFF, 0xFE, 0xFD]).unwrap_err();
    let error: Error = utf8_error.into();
    assert!(matches!(error, Error::Utf8(_)));
}

#[test]
fn other_with_source_preserves_chain() {
    let source = std::io::Error::other("inner failure");
    let error = Error::with_source("outer context", source);

    assert!(matches!(
        &error,
        Error::Other { context, source: Some(_), .. } if context == "outer context"
    ));

    assert!(
        error.source().is_some(),
        "source() must return the inner error"
    );
    assert!(
        error
            .source()
            .unwrap()
            .to_string()
            .contains("inner failure"),
        "source chain must preserve the inner message"
    );
}

#[test]
fn other_without_source_has_no_chain() {
    let error = Error::other("standalone message");
    assert!(matches!(
        &error,
        Error::Other { context, .. } if context == "standalone message"
    ));
    assert!(error.source().is_none());
}

#[test]
fn tls_error_preserves_source_chain() {
    let source = std::io::Error::other("PEM parse failed");
    let error = Error::tls("failed to parse PEM certificate", source);

    assert!(matches!(
        &error,
        Error::Tls { context, source: Some(_), .. } if context == "failed to parse PEM certificate"
    ));
    assert!(
        error.source().is_some(),
        "source() must return the inner error"
    );
    assert!(
        error
            .source()
            .unwrap()
            .to_string()
            .contains("PEM parse failed"),
        "source chain must preserve the inner message"
    );
}

#[test]
fn tls_error_display_includes_context() {
    let source = std::io::Error::other("bad cert");
    let error = Error::tls("rustls config", source);
    let display = error.to_string();
    assert!(display.contains("rustls config"), "display: {display}");
}

#[test]
fn invalid_etag_preserves_reason_without_source() {
    let error = Error::invalid_etag(EtagReason::InvalidFormat);
    assert!(matches!(
        &error,
        Error::InvalidEtag {
            reason: EtagReason::InvalidFormat,
            source: None,
            ..
        }
    ));
    assert!(
        error.to_string().contains("invalid entity-tag format"),
        "display: {}",
        error
    );
    assert!(error.source().is_none());
}

#[test]
fn invalid_etag_with_source_preserves_chain() {
    let source = std::io::Error::other("bad header value");
    let error = Error::invalid_etag_with_source(EtagReason::InvalidHeaderValue, source);
    assert!(matches!(
        &error,
        Error::InvalidEtag {
            reason: EtagReason::InvalidHeaderValue,
            source: Some(_),
            ..
        }
    ));
    assert!(
        error.source().is_some(),
        "source() must return the inner error"
    );
    assert!(
        error
            .source()
            .unwrap()
            .to_string()
            .contains("bad header value"),
        "source chain must preserve the inner message"
    );
}

#[test]
fn invalid_component_name_preserves_fields() {
    let error = Error::invalid_component_name("VEVENT/INVALID", "component name must not be empty");
    assert!(matches!(
        &error,
        Error::InvalidComponentName { name, reason, bad_char: None, .. }
            if name == "VEVENT/INVALID" && *reason == "component name must not be empty"
    ));
    assert!(
        error.to_string().contains("VEVENT/INVALID"),
        "display: {}",
        error
    );
}

#[test]
fn invalid_component_name_with_char_preserves_bad_char() {
    let error = Error::invalid_component_name_with_char("VEVENT/INVALID", "invalid character", '/');
    assert!(matches!(
        &error,
        Error::InvalidComponentName { name, reason, bad_char: Some('/'), .. }
            if name == "VEVENT/INVALID" && *reason == "invalid character"
    ));
    assert!(
        error.to_string().contains("VEVENT/INVALID"),
        "display: {}",
        error
    );
}

#[test]
fn invalid_datetime_preserves_fields() {
    let error = Error::invalid_datetime(
        "calendar-query start",
        "2024-01-01T00:00:00Z",
        "expected iCalendar format YYYYMMDDTHHMMSSZ",
    );
    assert!(matches!(
        &error,
        Error::InvalidDateTime { context, value, reason, .. }
            if context == "calendar-query start"
            && value == "2024-01-01T00:00:00Z"
            && *reason == "expected iCalendar format YYYYMMDDTHHMMSSZ"
    ));
    assert!(
        error.to_string().contains("2024-01-01T00:00:00Z"),
        "display: {}",
        error
    );
    assert!(
        error.to_string().contains("calendar-query start"),
        "display should include context: {}",
        error
    );
}

/// A connection-refused error must be classified as `Error::Connection`.
///
/// `hyper_util::client::legacy::Error`'s `ErrorKind` enum is private, so
/// we cannot construct a connect error in isolation. Instead we trigger a
/// real connection failure by pointing the client at `127.0.0.1:1`, a port
/// that is virtually always closed (connection refused → `is_connect()` is
/// `true` → `Error::Connection`). If something *is* listening on port 1,
/// pick another closed port.
#[tokio::test]
async fn connection_error_maps_to_connection_variant() {
    // Port 1 is reserved and almost never has a listener.
    let client = WebDavClient::builder("http://127.0.0.1:1/")
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder must succeed for a valid URL");

    let result = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await;

    let err = result.expect_err("connection to a closed port should fail");
    assert!(
        matches!(err, Error::Connection(_)),
        "connect-refused should map to Error::Connection, got: {err:?}"
    );
}

// NOTE: A transport-specific error (`Error::Transport`) requires a server
// that accepts a connection and then breaks the response stream mid-flight
// (e.g. an early EOF after the status line). This cannot be unit-tested
// without a real or mock server and is exercised by the e2e test suite
// against a live DAV server. The test above covers the connect-path, which
// is the most common retry-relevant case.
