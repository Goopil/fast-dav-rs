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
