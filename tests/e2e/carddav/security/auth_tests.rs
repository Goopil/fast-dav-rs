//! Auth-on-wire e2e for CardDAV (#117): Basic credentials, good → success,
//! bad → typed 401 error, mirroring the CalDAV security domain.

use fast_dav_rs::{CardDavClient, Error};

const SABREDAV_URL: &str = "http://localhost:8080/";

fn client(user: Option<&str>, pass: Option<&str>) -> CardDavClient {
    CardDavClient::new(SABREDAV_URL, user, pass).expect("client construction")
}

#[tokio::test]
async fn test_basic_auth_good_credentials_list_addressbooks() {
    let books = client(Some("test"), Some("test"))
        .list_addressbooks("addressbooks/test/")
        .await
        .expect("list_addressbooks with valid credentials must succeed");
    assert!(
        books.iter().all(|b| b.href.contains("addressbooks/test/")),
        "all returned hrefs must be under the addressbooks home, got: {books:?}"
    );
}

#[tokio::test]
async fn test_basic_auth_bad_credentials_typed_401() {
    let err = client(Some("wave3-wrong-user"), Some("wave3-wrong-pass"))
        .list_addressbooks("addressbooks/test/")
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
        .list_addressbooks("addressbooks/test/")
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
