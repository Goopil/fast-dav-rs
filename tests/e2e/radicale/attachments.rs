//! Radicale managed attachments: records the observed unsupported behavior
//! (RFC 8607).

use super::util::{RADICALE_USER, radicale_caldav_client};
use bytes::Bytes;
use fast_dav_rs::{Error, Operation};

/// Records Radicale 3.7.6's observed behavior for managed attachments
/// (RFC 8607, issue #172): the server does **not** implement the feature —
/// `POST <calendar>?action=attachment-add` answers `405 Method Not Allowed`
/// on any resource (its POST handler only serves `/.web` and `/.sharing`).
/// The client maps this to
/// [`Error::UnexpectedStatus`](fast_dav_rs::Error::UnexpectedStatus) with
/// [`Operation::PostManagedAttachment`](fast_dav_rs::Operation::PostManagedAttachment);
/// the RFC 8607 success contract itself is covered by the unit wire tests.
#[tokio::test]
async fn test_managed_attachment_unsupported_records_observed_behavior() {
    let client = radicale_caldav_client();

    let err = client
        .post_managed_attachment(
            &format!("{RADICALE_USER}/fixture-calendar/"),
            "fixture-event-1@example.com",
            None,
            Bytes::from_static(b"attachment body"),
            "text/plain",
        )
        .await
        .expect_err("Radicale must not implement managed attachments (observed 405)");
    match err {
        Error::UnexpectedStatus {
            operation, status, ..
        } => {
            assert_eq!(operation, Operation::PostManagedAttachment);
            println!("Radicale attachment-add POST -> UnexpectedStatus {status}");
            assert_eq!(
                status.as_u16(),
                405,
                "observed 405 Method Not Allowed for ?action=attachment-add"
            );
        }
        other => panic!("expected UnexpectedStatus for attachment-add POST, got: {other:?}"),
    }

    // Robustness: normal operations keep working after the rejected POST.
    let alive = client
        .get(&format!(
            "{RADICALE_USER}/fixture-calendar/fixture-event-1@example.com.ics"
        ))
        .await
        .expect("GET after rejected POST");
    assert!(
        alive.status().is_success(),
        "server must stay healthy, got {}",
        alive.status()
    );
}
