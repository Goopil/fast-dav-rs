//! Auth-on-wire e2e for CalDAV (#117): Basic credentials, good → 200-class
//! success, bad → typed 401 error. The header is proven to be sent by the
//! transport-level echo test in `core/transport_tests.rs`.

use fast_dav_rs::{CalDavClient, Error};

const SABREDAV_URL: &str = "http://localhost:8080/";

fn client(user: Option<&str>, pass: Option<&str>) -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, user, pass).expect("client construction")
}

#[tokio::test]
async fn test_basic_auth_good_credentials_list_calendars() {
    let calendars = client(Some("test"), Some("test"))
        .list_calendars("calendars/test/")
        .await
        .expect("list_calendars with valid credentials must succeed");
    // Sabre/DAV always lists the collection itself at minimum; don't assert a
    // minimum count (order-independence), just that parsing succeeded.
    assert!(
        calendars.iter().all(|c| c.href.contains("calendars/test/")),
        "all returned hrefs must be under the calendars home, got: {calendars:?}"
    );
}

#[tokio::test]
async fn test_basic_auth_bad_credentials_typed_401() {
    let err = client(Some("wave3-wrong-user"), Some("wave3-wrong-pass"))
        .list_calendars("calendars/test/")
        .await
        .expect_err("bad credentials must fail");
    match err {
        Error::UnexpectedStatus { status, .. } => {
            assert_eq!(
                status.as_u16(),
                401,
                "expected typed 401 Unauthorized, got {}",
                status
            );
        }
        other => panic!("expected UnexpectedStatus(401), got: {other:?}"),
    }
}

#[tokio::test]
async fn test_basic_auth_missing_credentials_typed_401() {
    let err = client(None, None)
        .list_calendars("calendars/test/")
        .await
        .expect_err("missing credentials must fail");
    match err {
        Error::UnexpectedStatus { status, .. } => {
            assert_eq!(
                status.as_u16(),
                401,
                "expected typed 401 Unauthorized, got {}",
                status
            );
        }
        other => panic!("expected UnexpectedStatus(401), got: {other:?}"),
    }
}
