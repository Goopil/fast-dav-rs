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

    let calendars = map_calendar_list(parse_multistatus_bytes(calendars_xml.as_bytes()).unwrap());
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

    let objects = map_calendar_objects(parse_multistatus_bytes(multiget_xml.as_bytes()).unwrap());
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
    let sync = map_sync_response(
        &headers,
        parse_multistatus_bytes(sync_xml.as_bytes()).unwrap(),
    );
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://sabre.io/ns/sync/43")
    );
    assert_eq!(sync.items.len(), 2);
    assert!(sync.items.iter().any(|item| item.is_deleted));
}
