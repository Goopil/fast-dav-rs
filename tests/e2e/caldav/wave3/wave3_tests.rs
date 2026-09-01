//! E2E coverage for the wave-3 high-level APIs (#117):
//! `free_busy_query`, `expand` on calendar-query/multiget/sync-collection,
//! `supports_webdav_sync` → `SyncCapability`, and the bomb-guard interplay
//! (no false `BodyTooLarge` on legitimate collections).

use crate::util::{unique_calendar_name, unique_uid};
use bytes::Bytes;
use fast_dav_rs::caldav::{FreeBusyType, TimeRange};
use fast_dav_rs::{CalDavClient, Error, SyncCapability};
use futures::future::join_all;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CalDavClient {
    CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client")
}

async fn create_calendar(client: &CalDavClient, prefix: &str) -> (String, String) {
    let calendar_name = unique_calendar_name(prefix);
    let calendar_path = format!("calendars/{TEST_USER}/{calendar_name}/");
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_name
    );
    let resp = client
        .mkcalendar(&calendar_path, &calendar_xml)
        .await
        .expect("mkcalendar request");
    assert!(
        resp.status().is_success(),
        "Expected successful calendar creation, got {}",
        resp.status()
    );
    (calendar_name, calendar_path)
}

async fn put_event(client: &CalDavClient, calendar_path: &str, uid: &str, ics: String) -> String {
    let event_path = format!("{calendar_path}{uid}.ics");
    let resp = client
        .put(&event_path, Bytes::from(ics))
        .await
        .expect("event PUT request");
    assert!(
        resp.status().is_success(),
        "Expected successful event PUT for {uid}, got {}",
        resp.status()
    );
    event_path
}

fn recurring_event_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Wave3//EN\r\n\
         BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260105T100000Z\r\nDTEND:20260105T110000Z\r\n\
         RRULE:FREQ=DAILY;COUNT=5\r\nSUMMARY:{summary}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

fn single_event_ics(uid: &str, summary: &str, start: &str, end: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Wave3//EN\r\n\
         BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:20260101T000000Z\r\n\
         DTSTART:{start}\r\nDTEND:{end}\r\nSUMMARY:{summary}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

async fn cleanup(client: &CalDavClient, calendar_path: &str, event_paths: &[String]) {
    for path in event_paths {
        let _ = client.delete(path).await;
    }
    let _ = client.delete(calendar_path).await;
}

// ---------------------------------------------------------------------------
// free_busy_query (PR #116)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_free_busy_query_happy_path_parses_periods() {
    let client = create_test_client();
    let (_name, calendar_path) = create_calendar(&client, "fb_happy").await;

    let single_uid = unique_uid("fb-single");
    let recur_uid = unique_uid("fb-recur");
    let mut event_paths = Vec::new();
    event_paths.push(
        put_event(
            &client,
            &calendar_path,
            &single_uid,
            single_event_ics(
                &single_uid,
                "Busy block",
                "20260106T140000Z",
                "20260106T150000Z",
            ),
        )
        .await,
    );
    event_paths.push(
        put_event(
            &client,
            &calendar_path,
            &recur_uid,
            recurring_event_ics(&recur_uid, "Recurring busy"),
        )
        .await,
    );

    let periods = client
        .free_busy_query(&calendar_path, "20260101T000000Z", "20260112T000000Z")
        .await
        .expect("free_busy_query must succeed on a real calendar");

    assert!(
        !periods.is_empty(),
        "Expected busy periods for a calendar with events; got none"
    );
    // Sabre/DAV emits plain FREEBUSY lines (no FBTYPE) → Busy.
    assert!(
        periods.iter().all(|p| p.fb_type == FreeBusyType::Busy),
        "Sabre/DAV reports plain busy periods, got: {periods:?}"
    );
    // The single event's exact window must be reported.
    assert!(
        periods
            .iter()
            .any(|p| p.start == "20260106T140000Z" && p.end == "20260106T150000Z"),
        "Expected the single event window 20260106T140000Z/20260106T150000Z in {periods:?}"
    );
    // The recurring event contributes 5 instances (5 occurrences).
    assert!(
        periods.len() >= 6,
        "Expected at least 6 busy periods (1 single + 5 recurring), got {}",
        periods.len()
    );

    cleanup(&client, &calendar_path, &event_paths).await;
}

#[tokio::test]
async fn test_free_busy_query_invalid_window_rejected_before_network() {
    let client = create_test_client();
    let err = client
        .free_busy_query("calendars/test/", "not-a-date", "20260110T000000Z")
        .await
        .expect_err("invalid start must be rejected");
    assert!(
        matches!(err, Error::InvalidDateTime { .. }),
        "expected InvalidDateTime for an invalid window, got: {err:?}"
    );
}

#[tokio::test]
async fn test_free_busy_query_nonexistent_calendar_server_error() {
    let client = create_test_client();
    let err = client
        .free_busy_query(
            &format!(
                "calendars/{TEST_USER}/no_such_calendar_{}_/bogus/",
                unique_uid("fb-miss")
            ),
            "20260101T000000Z",
            "20260110T000000Z",
        )
        .await
        .expect_err("free-busy-query on a missing calendar must fail");
    match err {
        Error::UnexpectedStatus { status, .. } => {
            assert_eq!(status.as_u16(), 404, "expected 404 for a missing calendar");
        }
        other => panic!("expected UnexpectedStatus, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// expand on calendar-query / calendar-multiget / sync-collection (PR #116)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_expand_calendar_query_timerange_returns_instances() {
    let client = create_test_client();
    let (_name, calendar_path) = create_calendar(&client, "expand_query").await;

    let recur_uid = unique_uid("expand-q");
    let event_path = put_event(
        &client,
        &calendar_path,
        &recur_uid,
        recurring_event_ics(&recur_uid, "Expanded by query"),
    )
    .await;

    let objects = client
        .calendar_query_timerange(
            &calendar_path,
            "VEVENT",
            Some("20260101T000000Z"),
            Some("20260108T000000Z"),
            true,
            Some(TimeRange::new("20260101T000000Z").with_end("20260108T000000Z")),
        )
        .await
        .expect("calendar_query_timerange with expand");

    assert!(
        !objects.is_empty(),
        "Expected the recurring event in the expanded query result"
    );
    let data = objects
        .iter()
        .find_map(|o| o.calendar_data.as_deref())
        .expect("expanded query must return calendar data");
    // Server-side expansion: individual instances with RECURRENCE-ID, no RRULE.
    let recurrence_ids = data.matches("RECURRENCE-ID").count();
    assert!(
        recurrence_ids >= 3,
        "Expected ≥3 expanded instances for a COUNT=5 daily event clipped to Jan 1-8, got {recurrence_ids} in:\n{data}"
    );
    assert!(
        !data.contains("RRULE"),
        "Expanded data must not contain the RRULE anymore:\n{data}"
    );

    cleanup(&client, &calendar_path, &[event_path]).await;
}

#[tokio::test]
async fn test_expand_calendar_multiget_returns_instances() {
    let client = create_test_client();
    let (_name, calendar_path) = create_calendar(&client, "expand_mg").await;

    let recur_uid = unique_uid("expand-mg");
    let event_path = put_event(
        &client,
        &calendar_path,
        &recur_uid,
        recurring_event_ics(&recur_uid, "Expanded by multiget"),
    )
    .await;

    let href = format!("/{}", event_path.trim_start_matches('/'));
    let objects = client
        .calendar_multiget(
            &calendar_path,
            [href],
            true,
            Some(TimeRange::new("20260101T000000Z").with_end("20260108T000000Z")),
        )
        .await
        .expect("calendar_multiget with expand");

    assert_eq!(
        objects.len(),
        1,
        "Expected exactly the multiget target back, got: {objects:?}"
    );
    let data = objects[0]
        .calendar_data
        .as_deref()
        .expect("expanded multiget must return calendar data");
    let recurrence_ids = data.matches("RECURRENCE-ID").count();
    assert!(
        recurrence_ids >= 3,
        "Expected ≥3 expanded instances, got {recurrence_ids} in:\n{data}"
    );
    assert!(!data.contains("RRULE"), "Expanded data must drop the RRULE");

    cleanup(&client, &calendar_path, &[event_path]).await;
}

#[tokio::test]
async fn test_expand_sync_collection_returns_valid_delta() {
    let client = create_test_client();
    let (_name, calendar_path) = create_calendar(&client, "expand_sync").await;

    let recur_uid = unique_uid("expand-sync");
    let event_path = put_event(
        &client,
        &calendar_path,
        &recur_uid,
        recurring_event_ics(&recur_uid, "Expanded by sync"),
    )
    .await;

    let response = client
        .sync_collection(
            &calendar_path,
            None,
            Some(100),
            true,
            Some(TimeRange::new("20260101T000000Z").with_end("20260108T000000Z")),
        )
        .await
        .expect("sync_collection with expand must succeed");

    // Sabre/DAV limitation (documented exemption, see PR #117 body): it
    // answers sync-collection + expand with 207 but ignores the expand —
    // data comes back with the RRULE intact. Assert the request is accepted
    // and the delta is valid; expansion itself is covered by the
    // calendar-query and multiget tests above.
    assert!(
        !response.items.is_empty(),
        "Expected the recurring event in the sync delta"
    );
    let item = response
        .items
        .iter()
        .find(|i| i.href.contains(&recur_uid))
        .expect("Expected the recurring event in the sync delta");
    let data = item
        .calendar_data
        .as_deref()
        .expect("sync with include_data must return calendar data");
    assert!(
        data.contains(&recur_uid),
        "Sync item data should contain the event UID"
    );
    assert!(
        response.sync_token.is_some(),
        "Expected a sync token from the initial sync"
    );

    cleanup(&client, &calendar_path, &[event_path]).await;
}

#[tokio::test]
async fn test_expand_invalid_datetime_rejected_before_network() {
    let client = create_test_client();
    let err = client
        .calendar_query_timerange(
            "calendars/test/",
            "VEVENT",
            None,
            None,
            true,
            Some(TimeRange::new("definitely-not-a-date")),
        )
        .await
        .expect_err("invalid expand start must be rejected");
    assert!(
        matches!(err, Error::InvalidDateTime { .. }),
        "expected InvalidDateTime for invalid expand, got: {err:?}"
    );
}

#[tokio::test]
async fn test_expand_on_missing_calendar_server_error() {
    let client = create_test_client();
    let err = client
        .calendar_query_timerange(
            &format!(
                "calendars/{TEST_USER}/no_such_calendar_{}/",
                unique_uid("exp-miss")
            ),
            "VEVENT",
            None,
            None,
            true,
            Some(TimeRange::new("20260101T000000Z").with_end("20260108T000000Z")),
        )
        .await
        .expect_err("expanded query on a missing calendar must fail");
    match err {
        Error::UnexpectedStatus { status, .. } => {
            assert_eq!(status.as_u16(), 404, "expected 404 for a missing calendar");
        }
        other => panic!("expected UnexpectedStatus, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// supports_webdav_sync → SyncCapability (PR #115)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_supports_webdav_sync_supported_on_sabredav() {
    let client = create_test_client();
    let (_name, calendar_path) = create_calendar(&client, "sync_cap").await;

    // `supports_webdav_sync` probes the client's **base collection** (the
    // server root does not advertise sync-collection; calendar collections
    // do), so point a client at the created calendar.
    let base = format!("{SABREDAV_URL}{}", calendar_path.trim_start_matches('/'));
    let calendar_client = CalDavClient::new(&base, Some(TEST_USER), Some(TEST_PASS))
        .expect("client construction for the calendar base");
    let capability = calendar_client
        .supports_webdav_sync()
        .await
        .expect("supports_webdav_sync must not error on a reachable server");
    assert_eq!(
        capability,
        SyncCapability::Supported,
        "Sabre/DAV ships the WebDAV-Sync (RFC 6578) plugin; expected Supported"
    );

    let _ = client.delete(&calendar_path).await;
}

#[tokio::test]
async fn test_supports_webdav_sync_unknown_on_dead_endpoint() {
    // Bind and drop a listener to get a guaranteed-closed local port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let client = CalDavClient::new(
        &format!("http://127.0.0.1:{port}/"),
        Some(TEST_USER),
        Some(TEST_PASS),
    )
    .expect("client construction must succeed even for a dead endpoint");
    let capability = client
        .supports_webdav_sync()
        .await
        .expect("transport failure is reported as Ok(Unknown), never as an error");
    assert_eq!(
        capability,
        SyncCapability::Unknown,
        "A dead endpoint must yield Unknown (support could not be determined)"
    );
}

// ---------------------------------------------------------------------------
// Bomb-guard interplay (PR #114): no false BodyTooLarge on legit collections
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bomb_guard_large_legit_collection_no_false_body_too_large() {
    let client = create_test_client();
    let (_name, calendar_path) = create_calendar(&client, "bomb_guard").await;

    // 200 modest events (~60 KB aggregated) — far below the 256 MiB cap but
    // large enough to prove the guard does not misfire on legit collections.
    const EVENT_COUNT: usize = 200;
    let tasks = (0..EVENT_COUNT).map(|i| {
        let client = client.clone();
        let calendar_path = calendar_path.clone();
        let uid = unique_uid(&format!("bomb-{i}"));
        async move {
            let ics = single_event_ics(
                &uid,
                "Bomb guard filler",
                "20260301T090000Z",
                "20260301T100000Z",
            );
            let path = format!("{calendar_path}{uid}.ics");
            let resp = client
                .put(&path, Bytes::from(ics))
                .await
                .unwrap_or_else(|e| panic!("event PUT {i} failed: {e}"));
            assert!(
                resp.status().is_success(),
                "event PUT {i} returned {}",
                resp.status()
            );
            path
        }
    });
    let event_paths = join_all(tasks).await;

    let response = client
        .sync_collection(&calendar_path, None, Some(EVENT_COUNT as u32), true, None)
        .await;

    match response {
        Ok(delta) => {
            assert!(
                delta.items.len() >= EVENT_COUNT,
                "Expected ≥{} items in the delta, got {}",
                EVENT_COUNT,
                delta.items.len()
            );
            assert!(
                delta.items.iter().all(|i| !i.is_deleted),
                "No item in a fresh collection sync may be a deletion marker"
            );
        }
        Err(Error::BodyTooLarge { limit, .. }) => {
            panic!("Bomb guard misfired on a legitimate ~60 KB collection (limit {limit} bytes)")
        }
        Err(other) => panic!("sync_collection failed unexpectedly: {other:?}"),
    }

    cleanup(&client, &calendar_path, &event_paths).await;
}
