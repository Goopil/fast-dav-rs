//! Scheduling (RFC 6638) against the SabreDAV fixture (Schedule plugin enabled).

use crate::util::{event_ics, sabredav_caldav_client, unique_calendar_name, unique_uid};
use bytes::Bytes;

const MKCALENDAR_BODY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>e2e scheduling fixture</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#;

/// The fixture advertises the principal's scheduling collections
/// (RFC 6638 §2): `schedule-inbox-URL`, `schedule-outbox-URL`, and a
/// non-empty `calendar-user-address-set`.
#[tokio::test]
async fn test_discover_schedule_endpoints_on_sabredav() {
    let client = sabredav_caldav_client();
    let principal = client
        .discover_current_user_principal()
        .await
        .expect("principal discovery PROPFIND")
        .expect("fixture must advertise the current user principal");
    let endpoints = client
        .discover_schedule_endpoints(&principal)
        .await
        .expect("schedule endpoints PROPFIND");

    let inbox = endpoints
        .inbox
        .clone()
        .expect("SabreDAV Schedule plugin must advertise the schedule inbox");
    assert!(
        inbox.ends_with("/calendars/test/inbox/"),
        "unexpected schedule inbox href, got {endpoints:?}"
    );
    let outbox = endpoints
        .outbox
        .clone()
        .expect("SabreDAV Schedule plugin must advertise the schedule outbox");
    assert!(
        outbox.ends_with("/calendars/test/outbox/"),
        "unexpected schedule outbox href, got {endpoints:?}"
    );
    assert!(
        endpoints
            .user_addresses
            .iter()
            .any(|address| address.starts_with("mailto:")),
        "calendar-user-address-set must contain a mailto: address, got {endpoints:?}"
    );
}

/// `list_inbox` on the fixture's fresh (empty) schedule inbox returns
/// `Ok(vec![])`: the Depth-1 PROPFIND succeeds and the collection's own
/// entry (no etag, no calendar-data) is filtered out.
#[tokio::test]
async fn test_list_inbox_empty_on_sabredav() {
    let client = sabredav_caldav_client();
    let principal = client
        .discover_current_user_principal()
        .await
        .expect("principal discovery PROPFIND")
        .expect("fixture must advertise the current user principal");
    let endpoints = client
        .discover_schedule_endpoints(&principal)
        .await
        .expect("schedule endpoints PROPFIND");
    let inbox = endpoints
        .inbox
        .expect("SabreDAV Schedule plugin must advertise the schedule inbox");

    let items = client
        .list_inbox(&inbox)
        .await
        .expect("list_inbox PROPFIND");
    assert!(
        items.is_empty(),
        "fresh fixture inbox must be empty, got {items:?}"
    );
}

/// A minimal valid iTIP free-busy REQUEST (RFC 6638 §5: the outbox `POST`
/// body MUST be a `VFREEBUSY` component with `METHOD:REQUEST`). The UID is
/// substituted by the caller for fixture hygiene.
fn free_busy_request_ics(uid: &str, organizer: &str, attendee: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//e2e//EN\r\n\
         METHOD:REQUEST\r\n\
         BEGIN:VFREEBUSY\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20261004T120000Z\r\n\
         DTSTART:20261005T000000Z\r\n\
         DTEND:20261006T000000Z\r\n\
         ORGANIZER:{organizer}\r\n\
         ATTENDEE:{attendee}\r\n\
         END:VFREEBUSY\r\n\
         END:VCALENDAR\r\n"
    )
}

/// The fixture user's `mailto:` calendar user address, as advertised by
/// `calendar-user-address-set`.
fn fixture_user_address(endpoints: &fast_dav_rs::caldav::ScheduleEndpoints) -> String {
    endpoints
        .user_addresses
        .iter()
        .find(|a| a.starts_with("mailto:"))
        .cloned()
        .expect("fixture must advertise a mailto: calendar user address")
}

/// Outbox `POST` of a well-formed iTIP `VFREEBUSY` REQUEST (RFC 6638 §5)
/// via [`CalDavClient::post_schedule`]. SabreDAV 4.7.1 answers **200 OK**
/// with a `CALDAV:schedule-response` XML body whose `request-status` is
/// `2.0;Success` (observed live; RFC 6638 §5.2 allows a 200/204 family
/// success), so the documented success status is asserted and the body
/// shape is recorded here.
#[tokio::test]
async fn test_outbox_post_well_formed_free_busy_request() {
    let client = sabredav_caldav_client();
    let principal = client
        .discover_current_user_principal()
        .await
        .expect("principal discovery PROPFIND")
        .expect("fixture must advertise the current user principal");
    let endpoints = client
        .discover_schedule_endpoints(&principal)
        .await
        .expect("schedule endpoints PROPFIND");
    let outbox = endpoints
        .outbox
        .clone()
        .expect("SabreDAV Schedule plugin must advertise the schedule outbox");
    let address = fixture_user_address(&endpoints);

    let itip = Bytes::from(free_busy_request_ics(
        &unique_uid("outbox-fb"),
        &address,
        &address,
    ));
    let response = client
        .post_schedule(&outbox, &address, &[&address], itip)
        .await
        .expect("well-formed free-busy REQUEST must succeed");

    assert_eq!(
        response.status.as_u16(),
        200,
        "fixture answers the free-busy REQUEST with 200 (schedule-response)"
    );
    let body = String::from_utf8_lossy(&response.body).into_owned();
    assert!(
        body.contains("schedule-response"),
        "expected a CALDAV:schedule-response body"
    );
    assert!(
        body.contains("2.0;Success"),
        "expected request-status 2.0;Success in the schedule response"
    );
}

/// Outbox `POST` of a malformed (non-iCalendar) body: the server rejects it
/// with a non-2xx status — SabreDAV 4.7.1 answers **400 Bad Request** with
/// a `<D:error>` body (observed live) — which `post_schedule` maps to
/// `Error::UnexpectedStatus { operation: PostSchedule }`.
#[tokio::test]
async fn test_outbox_post_malformed_itip_rejected_on_sabredav() {
    let client = sabredav_caldav_client();
    let principal = client
        .discover_current_user_principal()
        .await
        .expect("principal discovery PROPFIND")
        .expect("fixture must advertise the current user principal");
    let endpoints = client
        .discover_schedule_endpoints(&principal)
        .await
        .expect("schedule endpoints PROPFIND");
    let outbox = endpoints
        .outbox
        .clone()
        .expect("SabreDAV Schedule plugin must advertise the schedule outbox");
    let address = fixture_user_address(&endpoints);

    let err = client
        .post_schedule(
            &outbox,
            &address,
            &[&address],
            Bytes::from_static(b"not a calendar at all"),
        )
        .await
        .expect_err("malformed iTIP must be rejected");

    assert!(
        matches!(
            err,
            fast_dav_rs::Error::UnexpectedStatus {
                operation: fast_dav_rs::Operation::PostSchedule,
                ref status,
                ..
            } if !status.is_success()
        ),
        "expected UnexpectedStatus(PostSchedule) with a non-2xx status, got {err:?}"
    );
}

/// SabreDAV 4.7.1 does not implement the RFC 6638 §8 schedule-tag
/// mechanism: scheduling object responses carry no `Schedule-Tag` header
/// (fixture limitation, verified live), so the conditional round-trip
/// cannot be exercised against a server-managed tag. This test records
/// the observed behavior instead: the response carries no `Schedule-Tag`
/// header, and `If-Schedule-Tag-Match` (an unrecognized header for this
/// server) is ignored, so `put_if_schedule_tag`/`delete_if_schedule_tag`
/// degenerate to unconditional writes.
#[tokio::test]
async fn test_schedule_tag_unsupported_records_observed_behavior_on_sabredav() {
    let client = sabredav_caldav_client();
    let calendar_name = unique_calendar_name("e2e_sched_tag");
    let calendar_path = format!("calendars/test/{calendar_name}/");
    let mk = client
        .mkcalendar(&calendar_path, MKCALENDAR_BODY)
        .await
        .expect("MKCALENDAR request");
    assert!(
        mk.status().is_success(),
        "Expected successful calendar creation, got {}",
        mk.status()
    );

    let uid = unique_uid("sched-tag");
    let object_path = format!("{calendar_path}{uid}.ics");
    let put = client
        .put(
            &object_path,
            Bytes::from(event_ics(&uid, "Schedule Tag Probe")),
        )
        .await
        .expect("PUT request");
    assert!(
        put.status().is_success(),
        "Expected successful event creation, got {}",
        put.status()
    );
    assert!(
        put.headers().get("schedule-tag").is_none(),
        "fixture does not implement RFC 6638 §8.2: no Schedule-Tag header expected"
    );

    // The schedule-tag header value is opaque and arbitrary here: the
    // fixture ignores it, so any non-empty tag documents the same behavior.
    let put_conditional = client
        .put_if_schedule_tag(
            &object_path,
            Bytes::from(event_ics(&uid, "Schedule Tag Probe 2")),
            "fixture-does-not-implement-schedule-tag",
        )
        .await
        .expect("conditional PUT request");
    assert!(
        put_conditional.status().is_success(),
        "If-Schedule-Tag-Match is ignored by the fixture, got {}",
        put_conditional.status()
    );
    let delete_conditional = client
        .delete_if_schedule_tag(&object_path, "fixture-does-not-implement-schedule-tag")
        .await
        .expect("conditional DELETE request");
    assert!(
        delete_conditional.status().is_success(),
        "Expected successful conditional delete, got {}",
        delete_conditional.status()
    );

    let cleanup = client.delete(&calendar_path).await;
    assert!(
        cleanup.is_ok(),
        "calendar cleanup must succeed: {:?}",
        cleanup.err()
    );
}
