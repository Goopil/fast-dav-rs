//! Radicale locking: records the observed no-LOCK behavior.

use super::util::{RADICALE_USER, radicale_caldav_client};
use fast_dav_rs::{Depth, Error, LockScope};

/// Records Radicale's observed behavior for WebDAV LOCK. Observed on
/// Radicale 3.7.6: `OPTIONS /` advertises `DAV: 1, 2, 3` but LOCK answers
/// `405 Method Not Allowed`. The assertion is deliberately loose (any
/// client/server error); the status is printed for the record.
#[tokio::test]
async fn test_lock_unsupported_records_observed_behavior() {
    let client = radicale_caldav_client();

    let fixture_calendar = format!("{RADICALE_USER}/fixture-calendar/");
    let err = client
        .lock(
            &fixture_calendar,
            LockScope::Exclusive,
            "<D:href>fast-dav-rs-e2e</D:href>",
            Some(60),
        )
        .await
        .expect_err("Radicale must not support LOCK (observed 405)");
    match err {
        Error::UnexpectedStatus { status, .. } => {
            println!("Radicale LOCK -> UnexpectedStatus {}", status);
            assert!(status.is_client_error() || status.is_server_error());
        }
        Error::UnexpectedStatusWithDav { status, .. } => {
            println!("Radicale LOCK -> UnexpectedStatusWithDav {status}");
            assert!(status.is_client_error() || status.is_server_error());
        }
        other => panic!("expected an UnexpectedStatus error for LOCK, got: {other:?}"),
    }

    // Robustness: normal operations keep working after the rejected LOCK.
    let alive = client
        .propfind(
            &fixture_calendar,
            Depth::Zero,
            r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#,
        )
        .await
        .expect("PROPFIND after rejected LOCK");
    assert!(
        alive.status().is_success(),
        "server must stay healthy, got {}",
        alive.status()
    );
}
