use fast_dav_rs::{
    build_calendar_multiget_body, build_calendar_query_body, build_sync_collection_body,
    map_calendar_list, map_calendar_objects, map_sync_response, parse_multistatus_bytes,
};
use hyper::http::HeaderMap;

#[test]
fn builds_calendar_query_with_timerange() {
    let body = build_calendar_query_body(
        "VEVENT",
        Some("20240101T000000Z"),
        Some("20240201T000000Z"),
        true,
    );
    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("name=\"VEVENT\""));
    assert!(body.contains("start=\"20240101T000000Z\""));
    assert!(body.contains("end=\"20240201T000000Z\""));
}

#[test]
fn builds_calendar_multiget_and_escapes() {
    let body = build_calendar_multiget_body(
        vec![
            "/dav/user01/Calendars/Personal/meeting.ics",
            "/dav/user01/Calendars/Personal/tasks&todo.ics",
        ],
        true,
    )
    .expect("hrefs present");
    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("tasks&amp;todo.ics"));
}

#[test]
fn builds_sync_collection_body_with_token_and_limit() {
    let body = build_sync_collection_body(Some("http://token"), Some(50), false);
    assert!(body.contains("<D:sync-token>http://token</D:sync-token>"));
    assert!(body.contains("<D:nresults>50</D:nresults>"));
    assert!(!body.contains("<C:calendar-data/>"));
}

#[test]
fn maps_caldav_multistatus_structures() {
    let calendars_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/Calendars/Personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <C:calendar-description>Work + private events</C:calendar-description>
        <C:calendar-color>#ff0000</C:calendar-color>
        <C:calendar-timezone><![CDATA[BEGIN:VTIMEZONE
TZID:Europe/Paris
END:VTIMEZONE]]></C:calendar-timezone>
        <D:getetag>"cal-etag"</D:getetag>
        <D:sync-token>http://sabre.io/ns/sync/42</D:sync-token>
        <D:resourcetype>
          <D:collection/>
          <C:calendar/>
        </D:resourcetype>
        <C:supported-calendar-component-set>
          <C:comp name="VEVENT"/>
        </C:supported-calendar-component-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let calendars = map_calendar_list(
        parse_multistatus_bytes(calendars_xml.as_bytes())
            .unwrap()
            .items,
    );
    assert_eq!(calendars.len(), 1);
    let calendar = &calendars[0];
    assert_eq!(calendar.href, "/dav/user01/Calendars/Personal/");
    assert_eq!(calendar.displayname.as_deref(), Some("Personal"));
    assert_eq!(
        calendar.sync_token.as_deref(),
        Some("http://sabre.io/ns/sync/42")
    );
    assert_eq!(calendar.supported_components, vec!["VEVENT".to_string()]);

    let multiget_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/Calendars/Personal/meeting.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"1234-1"</D:getetag>
        <C:calendar-data><![CDATA[BEGIN:VCALENDAR
BEGIN:VEVENT
UID:meeting-1
END:VEVENT
END:VCALENDAR]]></C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let objects = map_calendar_objects(
        parse_multistatus_bytes(multiget_xml.as_bytes())
            .unwrap()
            .items,
    );
    assert_eq!(objects.len(), 1);
    let object = &objects[0];
    assert_eq!(object.href, "/dav/user01/Calendars/Personal/meeting.ics");
    assert!(
        object
            .calendar_data
            .as_ref()
            .unwrap()
            .contains("UID:meeting-1")
    );

    let sync_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/Calendars/Personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:sync-token>http://sabre.io/ns/sync/43</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/Calendars/Personal/meeting.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"1234-2"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/Calendars/Personal/outdated.ics</D:href>
    <D:propstat>
      <D:prop/>
      <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", "http://sabre.io/ns/sync/43".parse().unwrap());
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://sabre.io/ns/sync/43")
    );
    assert_eq!(sync.items.len(), 2);
    assert!(sync.items.iter().any(|item| item.is_deleted));
}

#[test]
fn test_top_level_sync_token_parsing() {
    // Test Apple CalDAV format where sync-token is at top level of multistatus
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==</D:sync-token>
    <D:response>
        <D:href>/calendars/user/calendar/event1.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"abc123"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Verify the sync token was captured at the top level
    assert_eq!(
        parsed.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // The sync response should use the top-level token
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );
    assert_eq!(sync.items.len(), 1);
    assert_eq!(sync.items[0].href, "/calendars/user/calendar/event1.ics");
    assert_eq!(sync.items[0].etag.as_deref(), Some("\"abc123\""));
}

#[test]
fn test_sync_token_no_changes() {
    // Test response with no changes - only sync token returned (Apple CalDAV behavior)
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<D:multistatus xmlns="DAV:">
    <sync-token>HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==</sync-token>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Verify the sync token was captured
    assert_eq!(
        parsed.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );

    // No items should be present
    assert_eq!(parsed.items.len(), 0);

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // The sync response should have the token but no items
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );
    assert_eq!(sync.items.len(), 0);
}

#[test]
fn test_sync_token_priority_top_level_over_header() {
    // Test that top-level sync token takes priority over header
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>top-level-token-123</D:sync-token>
    <D:response>
        <D:href>/calendars/user/event.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", "header-token-456".parse().unwrap());
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Top-level token should win
    assert_eq!(sync.sync_token.as_deref(), Some("top-level-token-123"));
}

#[test]
fn test_sync_token_priority_top_level_over_per_item() {
    // Test that top-level sync token takes priority over per-item tokens
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>top-level-token-abc</D:sync-token>
    <D:response>
        <D:href>/calendars/user/</D:href>
        <D:propstat>
            <D:prop>
                <D:sync-token>per-item-token-xyz</D:sync-token>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Top-level token should win over per-item token
    assert_eq!(sync.sync_token.as_deref(), Some("top-level-token-abc"));
}

#[test]
fn test_sync_token_fallback_per_item() {
    // Test that per-item sync token is used when no top-level token exists
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/calendars/user/</D:href>
        <D:propstat>
            <D:prop>
                <D:sync-token>per-item-token-fallback</D:sync-token>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Per-item token should be used as fallback
    assert_eq!(sync.sync_token.as_deref(), Some("per-item-token-fallback"));
}

#[test]
fn test_sync_token_fallback_header() {
    // Test that header sync token is used when no top-level or per-item tokens exist
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/calendars/user/event.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", "header-token-fallback".parse().unwrap());
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Header token should be used as last fallback
    assert_eq!(sync.sync_token.as_deref(), Some("header-token-fallback"));
}

#[test]
fn test_sync_with_deleted_items() {
    // Test sync response with deleted items (404 status)
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>sync-token-with-deletes</D:sync-token>
    <D:response>
        <D:href>/calendars/user/deleted1.ics</D:href>
        <D:propstat>
            <D:prop/>
            <D:status>HTTP/1.1 404 Not Found</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/calendars/user/deleted2.ics</D:href>
        <D:status>HTTP/1.1 410 Gone</D:status>
    </D:response>
    <D:response>
        <D:href>/calendars/user/updated.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"new-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    assert_eq!(sync.sync_token.as_deref(), Some("sync-token-with-deletes"));
    assert_eq!(sync.items.len(), 3);

    // Check that deleted items are marked correctly
    let deleted_count = sync.items.iter().filter(|item| item.is_deleted).count();
    assert_eq!(deleted_count, 2);

    // Check that updated item is not marked as deleted
    let updated = sync
        .items
        .iter()
        .find(|item| item.href.contains("updated.ics"))
        .unwrap();
    assert!(!updated.is_deleted);
    assert_eq!(updated.etag.as_deref(), Some("\"new-etag\""));
}

#[test]
fn test_sync_with_multiple_changes() {
    // Test sync response with multiple changed items
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>multi-change-token</D:sync-token>
    <D:response>
        <D:href>/calendars/user/calendar/</D:href>
        <D:propstat>
            <D:prop>
                <D:resourcetype>
                    <D:collection/>
                </D:resourcetype>
                <D:getetag>"cal-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/calendars/user/calendar/event1.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/calendars/user/calendar/event2.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag2"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    assert_eq!(sync.sync_token.as_deref(), Some("multi-change-token"));
    // Collection item should be filtered out, only event items remain
    assert_eq!(sync.items.len(), 2);
    assert!(sync.items.iter().all(|item| item.href.ends_with(".ics")));
}

#[test]
fn test_sync_token_without_namespace_prefix() {
    // Test Apple CalDAV format without namespace prefix (default namespace)
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<multistatus xmlns="DAV:">
    <sync-token>token-no-prefix</sync-token>
    <response>
        <href>/calendars/user/event.ics</href>
        <propstat>
            <prop>
                <getetag>"etag-no-prefix"</getetag>
            </prop>
            <status>HTTP/1.1 200 OK</status>
        </propstat>
    </response>
</multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Should parse correctly even without D: prefix
    assert_eq!(parsed.sync_token.as_deref(), Some("token-no-prefix"));
    assert_eq!(parsed.items.len(), 1);

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);
    assert_eq!(sync.sync_token.as_deref(), Some("token-no-prefix"));
    assert_eq!(sync.items[0].etag.as_deref(), Some("\"etag-no-prefix\""));
}

#[test]
fn test_sync_collection_filters_out_collections() {
    // Test that collection items are properly filtered out
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>filter-test-token</D:sync-token>
    <D:response>
        <D:href>/calendars/user/calendar/</D:href>
        <D:propstat>
            <D:prop>
                <D:resourcetype>
                    <D:collection/>
                </D:resourcetype>
                <D:getetag>"cal-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/calendars/user/calendar/event.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"event-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Both items should be parsed
    assert_eq!(parsed.items.len(), 2);

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Collection should be filtered out, only event remains
    assert_eq!(sync.items.len(), 1);
    assert_eq!(sync.items[0].href, "/calendars/user/calendar/event.ics");
}
