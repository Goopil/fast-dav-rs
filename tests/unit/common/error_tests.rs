use fast_dav_rs::{CalDavClient, Error, Result};
use hyper::StatusCode;
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
    assert!(matches!(result, Err(Error::Other(_))));
}
