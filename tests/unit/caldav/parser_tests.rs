use fast_dav_rs::parse_multistatus_bytes;

#[test]
fn parse_multistatus_extracts_calendar_properties() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-home-set>
          <D:href>/dav/user01/</D:href>
        </C:calendar-home-set>
        <D:resourcetype>
          <D:collection/>
        </D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <D:getetag>"etag-123"</D:getetag>
        <D:resourcetype>
          <D:collection/>
          <C:calendar/>
        </D:resourcetype>
        <C:supported-calendar-component-set>
          <C:comp name="VEVENT"/>
          <C:comp name="VTODO"/>
        </C:supported-calendar-component-set>
        <C:calendar-data><![CDATA[BEGIN:VCALENDAR
END:VCALENDAR
]]></C:calendar-data>
        <D:sync-token>token-123</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let items = parse_multistatus_bytes(xml.as_bytes())
        .expect("xml parsing succeeds")
        .items;
    assert_eq!(items.len(), 2);

    let collection = &items[0];
    assert!(collection.is_collection);
    assert_eq!(collection.href, "/dav/user01/");
    assert_eq!(collection.calendar_home_set, vec!["/dav/user01/"]);

    let calendar = &items[1];
    assert!(calendar.is_calendar);
    assert_eq!(calendar.displayname.as_deref(), Some("Personal"));
    assert_eq!(
        calendar.supported_components,
        vec!["VEVENT".to_string(), "VTODO".to_string()]
    );
    assert_eq!(calendar.etag.as_deref(), Some("\"etag-123\""));
    assert_eq!(calendar.sync_token.as_deref(), Some("token-123"));
    let data = calendar
        .calendar_data
        .as_ref()
        .expect("calendar data present");
    assert!(data.contains("BEGIN:VCALENDAR"));
    assert_eq!(calendar.href, "/dav/user01/personal/");
}

#[test]
fn parse_multistatus_extracts_common_properties_and_top_level_sync_token() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:sync-token>top-token</D:sync-token>
  <D:response>
    <D:href>/dav/user01/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Work</D:displayname>
        <D:getetag>"etag-999"</D:getetag>
        <D:resourcetype>
          <D:collection/>
          <C:calendar/>
        </D:resourcetype>
        <D:sync-token>item-token</D:sync-token>
        <D:current-user-principal>
          <D:href>/principals/user01/</D:href>
        </D:current-user-principal>
        <D:owner>
          <D:href>/principals/user01/</D:href>
        </D:owner>
        <D:getcontenttype>text/calendar</D:getcontenttype>
        <D:getlastmodified>Mon, 01 Jan 2024 00:00:00 GMT</D:getlastmodified>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("xml parsing succeeds");
    assert_eq!(result.sync_token.as_deref(), Some("top-token"));
    assert_eq!(result.items.len(), 1);

    let item = &result.items[0];
    assert_eq!(item.href, "/dav/user01/cal/");
    assert_eq!(item.status.as_deref(), Some("HTTP/1.1 200 OK"));
    assert_eq!(item.displayname.as_deref(), Some("Work"));
    assert_eq!(item.etag.as_deref(), Some("\"etag-999\""));
    assert!(item.is_collection);
    assert!(item.is_calendar);
    assert_eq!(item.sync_token.as_deref(), Some("item-token"));
    assert_eq!(item.current_user_principal, vec!["/principals/user01/"]);
    assert_eq!(item.owner.as_deref(), Some("/principals/user01/"));
    assert_eq!(item.content_type.as_deref(), Some("text/calendar"));
    assert_eq!(
        item.last_modified.as_deref(),
        Some("Mon, 01 Jan 2024 00:00:00 GMT")
    );
}

#[test]
fn parse_multistatus_preserves_multiline_calendar_data() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/cal/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-data><![CDATA[BEGIN:VCALENDAR
]]><![CDATA[END:VCALENDAR
]]></C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("xml parsing succeeds");
    assert_eq!(result.items.len(), 1);

    let item = &result.items[0];
    let data = item.calendar_data.as_ref().expect("calendar data present");
    assert_eq!(data, "BEGIN:VCALENDAR\nEND:VCALENDAR\n");
}
