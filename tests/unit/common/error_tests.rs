use fast_dav_rs::{CalDavClient, Error, Result};
use hyper::StatusCode;
use std::error::Error as StdError;
use std::time::Duration;

#[test]
fn invalid_url_preserves_the_offending_value() {
    let error = CalDavClient::new("not a valid url", None, None)
        .err()
        .expect("an invalid URL should be rejected");

    assert!(matches!(
        error,
        Error::InvalidUrl { ref url, .. } if url == "not a valid url"
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
        Error::InvalidInput(ref message) if message == "ETag cannot be empty"
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
        Error::InvalidInput(ref message)
            if message.starts_with("invalid calendar-query component:")
    ));
}

#[test]
fn public_error_variants_expose_retry_relevant_context() {
    let status_error = Error::UnexpectedStatus {
        operation: "PROPFIND calendars".to_owned(),
        status: StatusCode::FORBIDDEN,
    };
    assert_eq!(
        status_error.to_string(),
        "PROPFIND calendars failed with 403 Forbidden"
    );

    let timeout_error = Error::Timeout {
        limit: Duration::from_secs(20),
    };
    assert!(timeout_error.to_string().contains("20s"));
}

#[test]
fn standard_error_conversions_remain_typed() {
    let io_error: Error = std::io::Error::other("broken pipe").into();
    assert!(matches!(io_error, Error::Io(_)));

    let result: Result<()> = Err(Error::other("application callback failed"));
    assert!(
        matches!(result, Err(Error::Other { context, source: None }) if context == "application callback failed")
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
    let uri_result: std::result::Result<hyper::http::uri::Uri, _> = "bad uri with spaces".parse();
    let uri_err = uri_result.unwrap_err();
    let http_err: hyper::http::Error = uri_err.into();
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
        Error::Other { context, source: Some(_) } if context == "outer context"
    ));

    assert!(
        StdError::source(&error).is_some(),
        "source() must return the inner error"
    );
    assert!(
        StdError::source(&error)
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
        Error::Other { context, source: None } if context == "standalone message"
    ));
    assert!(StdError::source(&error).is_none());
}
