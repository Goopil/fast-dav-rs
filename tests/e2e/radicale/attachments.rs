//! Radicale managed attachments: records the observed unsupported behavior
//! (RFC 8607) for the full managed-attachment surface — store (POST), update
//! (PUT), and removal (DELETE).

use super::util;
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

/// Records Radicale 3.7.6's observed behavior for the managed-attachment
/// update/removal methods (RFC 8607 §5.2/§5.3, issue #249):
///
/// - the POST that would mint a managed attachment is rejected with 405
///   (see
///   [`test_managed_attachment_unsupported_records_observed_behavior`]), so
///   no server-managed attachment resource can ever exist here;
/// - a `PUT` of non-iCalendar content into a calendar collection is
///   rejected with 400 (Radicale validates collection item data, ignoring
///   the `Cal-Managed-ID` semantics);
/// - a `DELETE` of a never-created attachment resource is 404.
///
/// Both methods surface the observed status as
/// [`Error::UnexpectedStatus`](fast_dav_rs::Error::UnexpectedStatus) with
/// the matching `Operation`; the RFC 8607 success contract itself is
/// covered by the unit wire tests.
#[tokio::test]
async fn test_managed_attachment_put_delete_unsupported_records_observed_behavior() {
    let client = radicale_caldav_client();
    let calendar_path = format!("{RADICALE_USER}/fixture-calendar/");

    // Live event so the calendar is exercised by the same GET-by-href
    // prerequisite the round-trip would use on a managed-attachment server.
    let uid = util::unique_uid("radicale-att");
    let event_path = format!("{calendar_path}{uid}.ics");
    let put = client
        .put(
            &event_path,
            Bytes::from(util::event_ics(&uid, "attachment probe")),
        )
        .await
        .expect("event PUT");
    assert!(
        put.status().is_success(),
        "event PUT must succeed, got {}",
        put.status()
    );
    let get = client.get(&event_path).await.expect("event GET");
    assert!(
        get.status().is_success(),
        "GET of the event href must succeed, got {}",
        get.status()
    );

    // §5.2 update: PUT of non-iCalendar attachment content into the
    // calendar collection -> observed 400.
    let attachment_href = format!("{calendar_path}{uid}.bin");
    let put_err = client
        .put_managed_attachment(
            &attachment_href,
            b"attachment body",
            "text/plain",
            "e2e-mid",
        )
        .await
        .expect_err("Radicale must reject a non-iCalendar PUT into a calendar (observed 400)");
    match put_err {
        Error::UnexpectedStatus {
            operation, status, ..
        } => {
            assert_eq!(operation, Operation::PutManagedAttachment);
            assert_eq!(
                status.as_u16(),
                400,
                "observed 400 for a non-iCalendar PUT into a calendar collection"
            );
        }
        other => panic!("expected UnexpectedStatus for managed-attachment PUT, got: {other:?}"),
    }

    // §5.3 removal: DELETE of a never-created attachment resource ->
    // observed 404.
    let delete_err = client
        .delete_managed_attachment(&format!("{calendar_path}missing-attachment.bin"), "e2e-mid")
        .await
        .expect_err("Radicale must 404 a DELETE of a never-created attachment resource");
    match delete_err {
        Error::UnexpectedStatus {
            operation, status, ..
        } => {
            assert_eq!(operation, Operation::DeleteManagedAttachment);
            assert_eq!(
                status.as_u16(),
                404,
                "observed 404 for a DELETE of a missing resource"
            );
        }
        other => panic!("expected UnexpectedStatus for managed-attachment DELETE, got: {other:?}"),
    }

    // Robustness: normal operations keep working after the rejected writes.
    let alive = client
        .get(&event_path)
        .await
        .expect("GET after rejected writes");
    assert!(
        alive.status().is_success(),
        "server must stay healthy, got {}",
        alive.status()
    );
}
