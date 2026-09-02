use fast_dav_rs::caldav::{
    CalendarQueryFilter, FreeBusyType, ParamFilter, PropFilter, TextMatch, TimeRange,
};
use fast_dav_rs::{CalDavClient, Depth, Error, RequestCompressionMode, SyncLevel};
use hyper::http::HeaderMap;

#[tokio::test]
async fn free_busy_query_sends_report_and_parses_periods() {
    let ical = "BEGIN:VCALENDAR\r\nBEGIN:VFREEBUSY\r\n\
        FREEBUSY;FBTYPE=BUSY-UNAVAILABLE:19970101T180000Z/19970102T070000Z,19970103T180000Z/19970104T070000Z\r\n\
        FREEBUSY:19970105T100000Z/19970105T120000Z\r\n\
        FREEBUSY;FBTYPE=BUSY-TENTATIVE:19970106T100000Z/19970106T120000Z\r\n\
        END:VFREEBUSY\r\nEND:VCALENDAR";
    let body = format!(
        "<?xml version=\"1.0\"?>\
<D:multistatus xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\
<D:response><D:href>/cal/</D:href><D:propstat><D:prop>\
<C:calendar-data><![CDATA[{ical}]]></C:calendar-data>\
</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>\
</D:multistatus>"
    );
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body.into_bytes(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let periods = client
        .free_busy_query("cal/", "20240101T000000Z", "20240201T000000Z")
        .await
        .unwrap();

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("REPORT"),
        "expected REPORT method in request: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("depth: 1"),
        "expected 'Depth: 1' in request: {req}"
    );
    assert!(
        req.contains(
            "<C:free-busy-query xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">"
        ),
        "free-busy-query root element missing: {req}"
    );
    assert!(
        req.contains("<C:time-range start=\"20240101T000000Z\" end=\"20240201T000000Z\"/>"),
        "time-range element missing: {req}"
    );

    assert_eq!(periods.len(), 4);
    assert_eq!(periods[0].fb_type, FreeBusyType::BusyUnavailable);
    assert_eq!(periods[0].start, "19970101T180000Z");
    assert_eq!(periods[0].end, "19970102T070000Z");
    assert_eq!(periods[1].fb_type, FreeBusyType::BusyUnavailable);
    assert_eq!(periods[1].start, "19970103T180000Z");
    assert_eq!(periods[1].end, "19970104T070000Z");
    assert_eq!(periods[2].fb_type, FreeBusyType::Busy);
    assert_eq!(periods[2].start, "19970105T100000Z");
    assert_eq!(periods[2].end, "19970105T120000Z");
    assert_eq!(periods[3].fb_type, FreeBusyType::BusyTentative);
    assert_eq!(periods[3].start, "19970106T100000Z");
    assert_eq!(periods[3].end, "19970106T120000Z");
}

#[tokio::test]
async fn free_busy_query_parses_bare_text_calendar_body() {
    // Sabre/DAV answers free-busy-query with a bare text/calendar VFREEBUSY
    // body instead of the RFC 4791 multistatus; the client must not return
    // an empty result on it.
    let ical = "BEGIN:VCALENDAR\r\nBEGIN:VFREEBUSY\r\n\
        FREEBUSY:20260105T100000Z/20260105T110000Z\r\n\
        FREEBUSY;FBTYPE=BUSY-TENTATIVE:20260106T100000Z/20260106T110000Z\r\n\
        END:VFREEBUSY\r\nEND:VCALENDAR\r\n";
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head(
            "Content-Type: text/calendar;charset=UTF-8\r\n",
            ical.len(),
        ),
        ical.as_bytes().to_vec(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let periods = client
        .free_busy_query("cal/", "20260101T000000Z", "20260110T000000Z")
        .await
        .unwrap();

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("REPORT"),
        "expected REPORT method in request: {req}"
    );

    assert_eq!(periods.len(), 2);
    assert_eq!(periods[0].fb_type, FreeBusyType::Busy);
    assert_eq!(periods[0].start, "20260105T100000Z");
    assert_eq!(periods[0].end, "20260105T110000Z");
    assert_eq!(periods[1].fb_type, FreeBusyType::BusyTentative);
    assert_eq!(periods[1].start, "20260106T100000Z");
    assert_eq!(periods[1].end, "20260106T110000Z");
}

#[tokio::test]
async fn free_busy_query_rejects_invalid_start() {
    let client = CalDavClient::new("https://example.com/dav/", None, None).unwrap();
    let err = client
        .free_busy_query("cal/", "not-a-date", "20240201T000000Z")
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidDateTime { ref context, .. }
            if context.contains("free-busy-query start")),
        "expected InvalidDateTime for free-busy-query start, got: {err:?}"
    );
}

#[tokio::test]
async fn calendar_query_timerange_sends_expand() {
    let body =
        b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"><D:sync-token>t</D:sync-token></D:multistatus>"
            .to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let expand = TimeRange::new("20240101T000000Z").with_end("20240201T000000Z");
    client
        .calendar_query_timerange("cal/", "VEVENT", None, None, false, Some(expand))
        .await
        .unwrap();

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains(
            "<C:calendar-data><C:expand start=\"20240101T000000Z\" end=\"20240201T000000Z\"/></C:calendar-data>"
        ),
        "expected expand element in request body: {req}"
    );
}

#[tokio::test]
async fn calendar_multiget_sends_expand() {
    let body =
        b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"><D:sync-token>t</D:sync-token></D:multistatus>"
            .to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let expand = TimeRange::new("20240101T000000Z").with_end("20240201T000000Z");
    client
        .calendar_multiget("cal/", ["/cal/a.ics"], false, Some(expand))
        .await
        .unwrap();

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains(
            "<C:calendar-data><C:expand start=\"20240101T000000Z\" end=\"20240201T000000Z\"/></C:calendar-data>"
        ),
        "expected start+end expand element in request body: {req}"
    );
}

#[tokio::test]
async fn calendar_multiget_rejects_expand_without_end() {
    // RFC 4791 §9.6.5: both start and end are #REQUIRED attributes of
    // <C:expand>; a start-only expand must fail before any network I/O.
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let Err(err) = client
        .calendar_multiget(
            "cal/",
            ["/cal/a.ics"],
            false,
            Some(TimeRange::new("20240101T000000Z")),
        )
        .await
    else {
        panic!("expand without end must fail before any network I/O");
    };

    assert!(
        matches!(err, Error::InvalidInput(ref msg) if msg.contains("expand requires an `end`")),
        "expected InvalidInput for expand without end, got: {err:?}"
    );
}

#[tokio::test]
async fn calendar_multiget_rejects_expand_end_before_start() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let Err(err) = client
        .calendar_multiget(
            "cal/",
            ["/cal/a.ics"],
            false,
            Some(TimeRange::new("20240201T000000Z").with_end("20240101T000000Z")),
        )
        .await
    else {
        panic!("end <= start must be rejected before any network I/O");
    };

    assert!(
        matches!(
            err,
            Error::InvalidDateTime { ref context, ref value, reason, .. }
                if context.contains("calendar-multiget expand")
                    && value == "20240101T000000Z"
                    && reason == "end must be after start"
        ),
        "expected InvalidDateTime(end must be after start), got: {err:?}"
    );
}

#[tokio::test]
async fn calendar_query_timerange_rejects_end_before_start() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let Err(err) = client
        .calendar_query_timerange(
            "cal/",
            "VEVENT",
            Some("20240201T000000Z"),
            Some("20240101T000000Z"),
            false,
            None,
        )
        .await
    else {
        panic!("time-range end <= start must be rejected before any network I/O");
    };

    assert!(
        matches!(
            err,
            Error::InvalidDateTime { ref context, reason, .. }
                if context == "invalid calendar-query time-range"
                    && reason == "end must be after start"
        ),
        "expected InvalidDateTime for end <= start, got: {err:?}"
    );
}

#[tokio::test]
async fn sync_collection_rejects_expand_without_end() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let Err(err) = client
        .sync_collection(
            "cal/",
            None,
            None,
            false,
            Some(TimeRange::new("20240101T000000Z")),
        )
        .await
    else {
        panic!("expand without end must fail before any network I/O");
    };

    assert!(
        matches!(err, Error::InvalidInput(ref msg) if msg.contains("expand requires an `end`")),
        "expected InvalidInput for expand without end, got: {err:?}"
    );
}

#[tokio::test]
async fn sync_collection_sends_expand() {
    let body =
        b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"><D:sync-token>tok-1</D:sync-token></D:multistatus>"
            .to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let expand = TimeRange::new("20240101T000000Z").with_end("20240201T000000Z");
    let sync = client
        .sync_collection("cal/", None, None, false, Some(expand))
        .await
        .unwrap();
    assert_eq!(sync.sync_token.as_deref(), Some("tok-1"));

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains(
            "<C:calendar-data><C:expand start=\"20240101T000000Z\" end=\"20240201T000000Z\"/></C:calendar-data>"
        ),
        "expected expand element in request body: {req}"
    );
}

#[test]
fn test_client_creation() {
    let client = CalDavClient::new("https://example.com/dav/", Some("user"), Some("pass"));
    assert!(client.is_ok());
}

#[test]
fn test_client_without_auth() {
    let client = CalDavClient::new("https://example.com/dav/", None, None);
    assert!(client.is_ok());
}

#[test]
fn test_build_uri_relative() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client.build_uri("calendar/").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/calendar/");
}

#[test]
fn test_build_uri_absolute() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("https://other.com/test/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://other.com/test/");
}

#[test]
fn test_build_uri_encodes_question_mark_in_path() {
    // A query string is not part of the path contract (issue #139): a `?`
    // is a resource-name character and must be percent-encoded.
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("calendar/?param=value")
        .expect("Failed to build URI");
    assert_eq!(
        uri.to_string(),
        "https://example.com/dav/user/calendar/%3Fparam=value"
    );
    assert!(uri.query().is_none());
}

#[test]
fn test_build_uri_empty_path() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client.build_uri("").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/");
}

#[test]
fn test_build_uri_root_path_only() {
    let client =
        CalDavClient::new("https://example.com/", None, None).expect("Failed to create client");

    let uri = client.build_uri("calendar/").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/calendar/");
}

#[test]
fn test_build_uri_with_special_characters() {
    let client =
        CalDavClient::new("https://example.com/dav/", None, None).expect("Failed to create client");

    let uri = client
        .build_uri("my-calendar_123/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/my-calendar_123/");
}

#[test]
fn test_depth_values() {
    assert_eq!(Depth::Zero.as_str(), "0");
    assert_eq!(Depth::One.as_str(), "1");
    assert_eq!(Depth::Infinity.as_str(), "infinity");
}

#[test]
fn test_escape_xml_basic() {
    assert_eq!(
        fast_dav_rs::caldav::client::escape_xml("Hello & World"),
        "Hello &amp; World"
    );
    assert_eq!(
        fast_dav_rs::caldav::client::escape_xml("Test <tag>"),
        "Test &lt;tag&gt;"
    );
    assert_eq!(
        fast_dav_rs::caldav::client::escape_xml("\"quotes\""),
        "&quot;quotes&quot;"
    );
    assert_eq!(
        fast_dav_rs::caldav::client::escape_xml("'apos'"),
        "&apos;apos&apos;"
    );
}

#[test]
fn test_escape_xml_complex() {
    let input = "Mix & match <tag attr=\"value\"> with 'quotes'";
    let expected = "Mix &amp; match &lt;tag attr=&quot;value&quot;&gt; with &apos;quotes&apos;";
    assert_eq!(fast_dav_rs::caldav::client::escape_xml(input), expected);
}

#[test]
fn test_escape_xml_empty() {
    assert_eq!(fast_dav_rs::caldav::client::escape_xml(""), "");
}

#[test]
fn test_escape_xml_no_special_chars() {
    assert_eq!(
        fast_dav_rs::caldav::client::escape_xml("normal text"),
        "normal text"
    );
}

#[test]
fn test_escape_xml_multiple_same_char() {
    assert_eq!(
        fast_dav_rs::caldav::client::escape_xml("&&&&"),
        "&amp;&amp;&amp;&amp;"
    );
}

#[test]
fn test_build_calendar_query_body() {
    let body = fast_dav_rs::caldav::client::build_calendar_query_body(
        "VEVENT",
        Some("20240101T000000Z"),
        Some("20240201T000000Z"),
        true,
        None,
    );
    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("name=\"VEVENT\""));
    assert!(body.contains("start=\"20240101T000000Z\""));
    assert!(body.contains("end=\"20240201T000000Z\""));
}

#[test]
fn test_build_calendar_query_body_no_time_range() {
    let body =
        fast_dav_rs::caldav::client::build_calendar_query_body("VTODO", None, None, false, None);
    assert!(!body.contains("<C:calendar-data/>"));
    assert!(body.contains("name=\"VTODO\""));
    assert!(!body.contains("start="));
    assert!(!body.contains("end="));
}

#[test]
fn test_build_calendar_query_body_partial_time_range() {
    let body = fast_dav_rs::caldav::client::build_calendar_query_body(
        "VEVENT",
        Some("20240101T000000Z"),
        None,
        true,
        None,
    );
    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("start=\"20240101T000000Z\""));
    assert!(!body.contains("end="));
}

#[test]
fn test_build_calendar_multiget_and_escapes() {
    let body = fast_dav_rs::caldav::client::build_calendar_multiget_body(
        vec![
            "/calendars/user/event1.ics",
            "/calendars/user/event&special.ics",
        ],
        true,
        None,
    )
    .expect("Should create body");

    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("/calendars/user/event1.ics"));
    assert!(body.contains("event&amp;special.ics")); // Escaped ampersand
}

#[test]
fn test_build_calendar_multiget_empty() {
    let body =
        fast_dav_rs::caldav::client::build_calendar_multiget_body(Vec::<String>::new(), true, None);
    assert!(body.is_none());
}

#[test]
fn test_build_sync_collection_body() {
    let body = fast_dav_rs::caldav::client::build_sync_collection_body(
        Some("http://example.com/sync-token-123"),
        Some(50),
        true,
        None,
    );

    assert!(body.contains("<D:sync-token>http://example.com/sync-token-123</D:sync-token>"));
    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("<D:nresults>50</D:nresults>"));
}

#[test]
fn test_map_calendar_list_filters_calendars() {
    let mut item = fast_dav_rs::caldav::types::DavItem::new();
    item.href = "/calendars/user/personal/".to_string();
    item.displayname = Some("Personal".to_string());
    item.is_calendar = true;

    let mut collection_item = fast_dav_rs::caldav::types::DavItem::new();
    collection_item.href = "/calendars/user/collection/".to_string();
    collection_item.displayname = Some("Collection".to_string());
    collection_item.is_collection = true;

    let items = vec![item.clone(), collection_item.clone()];
    let calendars = fast_dav_rs::caldav::client::map_calendar_list(items);

    assert_eq!(calendars.len(), 1);
    assert_eq!(calendars[0].href, "/calendars/user/personal/");
    assert_eq!(calendars[0].displayname, Some("Personal".to_string()));
}

#[test]
fn test_map_calendar_objects() {
    let mut item1 = fast_dav_rs::caldav::types::DavItem::new();
    item1.href = "/calendars/user/event1.ics".to_string();
    item1.etag = Some("abc123".to_string());
    item1.calendar_data = Some("BEGIN:VCALENDAR...END:VCALENDAR".to_string());

    let mut item2 = fast_dav_rs::caldav::types::DavItem::new();
    item2.href = "/calendars/user/event2.ics".to_string();
    item2.etag = Some("def456".to_string());
    item2.status = Some("HTTP/1.1 404 Not Found".to_string());

    let items = vec![item1.clone(), item2.clone()];
    let objects = fast_dav_rs::caldav::client::map_calendar_objects(items);

    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].href, "/calendars/user/event1.ics");
    assert_eq!(objects[0].etag, Some("abc123".to_string()));
    assert_eq!(
        objects[0].calendar_data,
        Some("BEGIN:VCALENDAR...END:VCALENDAR".to_string())
    );
    assert_eq!(objects[1].href, "/calendars/user/event2.ics");
    assert_eq!(objects[1].etag, Some("def456".to_string()));
    assert_eq!(
        objects[1].status,
        Some("HTTP/1.1 404 Not Found".to_string())
    );
}

#[test]
fn test_map_sync_response() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Sync-Token",
        "http://example.com/sync-token-456".parse().unwrap(),
    );

    let mut item1 = fast_dav_rs::caldav::types::DavItem::new();
    item1.href = "/calendars/user/event1.ics".to_string();
    item1.etag = Some("abc123".to_string());
    item1.calendar_data = Some("BEGIN:VCALENDAR...END:VCALENDAR".to_string());

    let mut item2 = fast_dav_rs::caldav::types::DavItem::new();
    item2.href = "/calendars/user/event2.ics".to_string();
    item2.status = Some("HTTP/1.1 404 Not Found".to_string());

    let mut collection_item = fast_dav_rs::caldav::types::DavItem::new();
    collection_item.href = "/calendars/user/subcalendar/".to_string();
    collection_item.sync_token = Some("http://example.com/sync-token-789".to_string());
    collection_item.is_collection = true;

    let items = vec![item1, item2, collection_item];
    let response = fast_dav_rs::caldav::client::map_sync_response(&headers, items, None);

    assert_eq!(
        response.sync_token,
        Some("http://example.com/sync-token-456".to_string())
    );
    assert_eq!(response.items.len(), 2); // Collection item should be filtered out

    // Check the first item (regular item with data)
    assert_eq!(response.items[0].href, "/calendars/user/event1.ics");
    assert_eq!(response.items[0].etag, Some("abc123".to_string()));
    assert!(!response.items[0].is_deleted); // Should not be deleted

    // Check second item (deleted item)
    assert_eq!(response.items[1].href, "/calendars/user/event2.ics");
    assert_eq!(
        response.items[1].status,
        Some("HTTP/1.1 404 Not Found".to_string())
    );
    assert!(response.items[1].is_deleted); // Should be marked as deleted
}

#[tokio::test]
async fn test_calendar_query_timerange_rejects_malicious_component() {
    let client = CalDavClient::new("https://example.com/dav/", Some("user"), Some("pass"))
        .expect("Failed to create client");

    let err = client
        .calendar_query_timerange("calendar/", "VEVENT\"><evil/>", None, None, false, None)
        .await
        .expect_err("component with XML metacharacters must be rejected before any request");
    assert!(matches!(
        err,
        Error::InvalidComponentName { ref name, bad_char: Some('"'), .. } if name == "VEVENT\"><evil/>"
    ));
    assert!(
        err.to_string().contains("VEVENT\"><evil/>"),
        "display should include the offending name: {err}"
    );
}

#[tokio::test]
async fn test_calendar_query_timerange_rejects_empty_component() {
    let client =
        CalDavClient::new("https://example.com/dav/", None, None).expect("Failed to create client");

    let err = client
        .calendar_query_timerange("calendar/", "", None, None, false, None)
        .await
        .expect_err("empty component must be rejected before any request");
    assert!(matches!(
        err,
        Error::InvalidComponentName { ref name, bad_char: None, .. } if name.is_empty()
    ));
}

#[tokio::test]
async fn test_calendar_query_timerange_rejects_malformed_start() {
    let client =
        CalDavClient::new("https://example.com/dav/", None, None).expect("Failed to create client");

    let err = client
        .calendar_query_timerange(
            "calendar/",
            "VEVENT",
            Some("2024-01-01T00:00:00Z"),
            None,
            false,
            None,
        )
        .await
        .expect_err("malformed start must be rejected before any request");
    assert!(matches!(
        err,
        Error::InvalidDateTime { ref context, ref value, .. }
            if context == "invalid calendar-query start" && value == "2024-01-01T00:00:00Z"
    ));
    assert!(
        err.to_string().contains("invalid calendar-query start"),
        "error display should include context: {err}"
    );
    assert!(
        err.to_string().contains("YYYYMMDDTHHMMSSZ"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_calendar_query_timerange_rejects_malformed_end() {
    let client =
        CalDavClient::new("https://example.com/dav/", None, None).expect("Failed to create client");

    let err = client
        .calendar_query_timerange(
            "calendar/",
            "VEVENT",
            Some("20240101T000000Z"),
            Some("20240101T000000Z\"><inject/>"),
            false,
            None,
        )
        .await
        .expect_err("malformed end must be rejected before any request");
    assert!(matches!(
        err,
        Error::InvalidDateTime { ref context, ref value, .. }
            if context == "invalid calendar-query end"
            && value == "20240101T000000Z\"><inject/>"
    ));
    assert!(
        err.to_string().contains("invalid calendar-query end"),
        "error display should include context: {err}"
    );
}

#[test]
fn builder_propagates_options() {
    use fast_dav_rs::RequestCompressionMode;
    use std::time::Duration;

    let client = CalDavClient::builder("https://cal.example.com/dav/")
        .basic_auth("user", "pass")
        .timeout(Duration::from_secs(3))
        .pool_max_idle_per_host(8)
        .request_compression(RequestCompressionMode::Force(
            fast_dav_rs::ContentEncoding::Gzip,
        ))
        .build()
        .expect("build succeeds");

    assert_eq!(
        client.request_compression_mode(),
        RequestCompressionMode::Force(fast_dav_rs::ContentEncoding::Gzip)
    );
    assert_eq!(
        client.request_compression(),
        fast_dav_rs::ContentEncoding::Gzip
    );
}

#[test]
fn builder_invalid_url() {
    let result = CalDavClient::builder("not a valid url").build();
    assert!(result.is_err());
}

#[test]
fn builder_bearer_auth() {
    let client = CalDavClient::builder("https://cal.example.com/dav/")
        .bearer_token("test-token")
        .build()
        .expect("build succeeds");
    // We can't directly access the auth header from CalDavClient,
    // but we verified it compiles and builds successfully.
    let _ = client;
}

#[test]
fn clone_shares_compression_mode() {
    use fast_dav_rs::RequestCompressionMode;

    let client_a = CalDavClient::builder("https://cal.example.com/dav/")
        .build()
        .unwrap();
    let client_b = client_a.clone();

    client_a.set_request_compression_mode(RequestCompressionMode::Disabled);

    assert_eq!(
        client_b.request_compression_mode(),
        RequestCompressionMode::Disabled
    );
}

#[test]
fn sync_token_round_trip_unquoted_in_request_body() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Sync-Token",
        r#""http://example.com/sync/99""#.parse().unwrap(),
    );
    let sync = fast_dav_rs::caldav::client::map_sync_response(&headers, Vec::new(), None);
    let normalized = sync.sync_token.expect("sync token present");
    assert_eq!(normalized, "http://example.com/sync/99");

    let body = fast_dav_rs::caldav::client::build_sync_collection_body(
        Some(&normalized),
        None,
        true,
        None,
    );
    assert!(
        body.contains("<D:sync-token>http://example.com/sync/99</D:sync-token>"),
        "sync-token should appear unquoted in request body, got: {body}"
    );
    assert!(
        !body.contains("<D:sync-token>\""),
        "sync-token should not have extra quotes in request body"
    );
}

#[tokio::test]
async fn sync_collection_sends_depth_zero() {
    let body = b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"><D:sync-token>tok-1</D:sync-token></D:multistatus>".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection("cal/", None, None, true, None)
        .await
        .unwrap();
    assert_eq!(sync.sync_token.as_deref(), Some("tok-1"));

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected 'Depth: 0' in request: {req}"
    );
}

#[tokio::test]
async fn sync_collection_507_on_request_uri_sets_truncated_and_item_surfaces() {
    // RFC 6578 §3.10: a truncated result set is reported as a 207 whose
    // request-URI response element carries `HTTP/1.1 507 Insufficient Storage`.
    let body = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/</D:href>
    <D:status>HTTP/1.1 507 Insufficient Storage</D:status>
    <D:error><D:number-of-matches-within-limits/></D:error>
  </D:response>
  <D:sync-token>http://example.com/sync/1233</D:sync-token>
</D:multistatus>"#
        .to_vec();
    let base = crate::common::http_helpers::serve_once(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection("cal/", None, None, false, None)
        .await
        .unwrap();

    assert!(
        sync.truncated,
        "a 507 inside the multistatus must surface as a first-class truncation signal"
    );
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://example.com/sync/1233")
    );
    let item = sync
        .items
        .iter()
        .find(|i| i.href == "/cal/")
        .expect("the request-URI item must still surface");
    assert_eq!(
        item.status.as_deref(),
        Some("HTTP/1.1 507 Insufficient Storage"),
        "per-item status must be passed through unchanged"
    );
    assert!(!item.is_deleted, "507 is not a deletion");
    assert_eq!(sync.items.len(), 2, "the member item must also surface");
}

#[tokio::test]
async fn sync_collection_normal_response_is_not_truncated() {
    let body = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>http://example.com/sync/2</D:sync-token>
</D:multistatus>"#
        .to_vec();
    let base = crate::common::http_helpers::serve_once(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection("cal/", None, None, false, None)
        .await
        .unwrap();

    assert!(!sync.truncated);
    assert_eq!(sync.items.len(), 1);
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://example.com/sync/2")
    );
}

#[tokio::test]
async fn mkcalendar_sends_depth_zero() {
    let body = b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"></D:multistatus>".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .mkcalendar(
            "newcal/",
            r#"<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:set><D:prop><D:displayname>New</D:displayname></D:prop></D:set></C:mkcalendar>"#,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("MKCALENDAR"),
        "expected MKCALENDAR method in request: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected explicit 'Depth: 0' on MKCALENDAR: {req}"
    );
}

#[tokio::test]
async fn list_calendars_requests_apple_color_not_caldav_color() {
    let body = b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"></D:multistatus>".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let calendars = client.list_calendars("home/").await.unwrap();
    assert!(calendars.is_empty());

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("<A:calendar-color/>"),
        "the Apple calendar-color property must be requested: {req}"
    );
    assert!(
        !req.contains("<C:calendar-color/>"),
        "the non-existent CalDAV calendar-color property must not be requested: {req}"
    );
}

#[tokio::test]
async fn list_calendars_requests_and_maps_collection_properties() {
    let body = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/home/personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <D:resourcetype>
          <D:collection/>
          <C:calendar/>
        </D:resourcetype>
        <C:max-resource-size>102400</C:max-resource-size>
        <C:supported-calendar-data>
          <C:calendar-data-type content-type="text/calendar" version="2.0"/>
        </C:supported-calendar-data>
        <C:max-attendees-per-instance>100</C:max-attendees-per-instance>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#
        .as_bytes()
        .to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let calendars = client.list_calendars("home/").await.unwrap();
    assert_eq!(calendars.len(), 1);
    assert_eq!(calendars[0].max_resource_size, Some(102400));
    assert_eq!(calendars[0].max_attendees_per_instance, Some(100));
    assert_eq!(
        calendars[0].supported_calendar_data,
        vec![fast_dav_rs::caldav::MediaType::new(
            "text/calendar",
            Some("2.0")
        )]
    );

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("<C:max-resource-size/>"),
        "max-resource-size must be requested: {req}"
    );
    assert!(
        req.contains("<C:supported-calendar-data/>"),
        "supported-calendar-data must be requested: {req}"
    );
    assert!(
        req.contains("<C:max-attendees-per-instance/>"),
        "max-attendees-per-instance must be requested: {req}"
    );
}

#[test]
fn test_map_calendar_list_maps_collection_properties() {
    let mut item = fast_dav_rs::caldav::types::DavItem::new();
    item.href = "/calendars/user/personal/".to_string();
    item.is_calendar = true;
    item.max_resource_size = Some(102400);
    item.max_attendees_per_instance = Some(100);
    item.supported_calendar_data = vec![fast_dav_rs::caldav::MediaType::new(
        "text/calendar",
        Some("2.0"),
    )];

    let calendars = fast_dav_rs::caldav::client::map_calendar_list(vec![item]);
    assert_eq!(calendars.len(), 1);
    assert_eq!(calendars[0].max_resource_size, Some(102400));
    assert_eq!(calendars[0].max_attendees_per_instance, Some(100));
    assert_eq!(
        calendars[0].supported_calendar_data,
        vec![fast_dav_rs::caldav::MediaType::new(
            "text/calendar",
            Some("2.0")
        )]
    );
}

#[tokio::test]
async fn caldav_follow_redirects_false_propagates() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/never/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;

    let client = CalDavClient::builder(&base)
        .follow_redirects(false)
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .send(hyper::Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        302,
        "redirects must not be followed when disabled"
    );

    let guard = captured.lock().unwrap();
    let raw = String::from_utf8_lossy(&guard);
    assert!(
        !raw.contains("/never/"),
        "the redirect target must not be requested: {raw}"
    );
}

const GONE_410: &str = "HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

const INITIAL_SYNC_BODY: &str = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal/b.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-b"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>http://example.com/sync/2</D:sync-token>
</D:multistatus>"#;

#[tokio::test]
async fn sync_collection_resilient_recovers_from_gone() {
    let ok_head = crate::common::http_helpers::response_head("", INITIAL_SYNC_BODY.len());
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![
        (GONE_410.to_string(), Vec::new()),
        (ok_head, INITIAL_SYNC_BODY.as_bytes().to_vec()),
    ])
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection_resilient("cal/", Some("http://example.com/sync/stale"), None, true)
        .await
        .unwrap();

    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://example.com/sync/2")
    );
    assert_eq!(sync.items.len(), 2);
    assert_eq!(sync.items[0].href, "/cal/a.ics");
    assert_eq!(sync.items[0].etag.as_deref(), Some("etag-a"));
    assert!(!sync.items[0].is_deleted);

    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        2,
        "410 must trigger exactly one retry: {reqs:?}"
    );
    let first = String::from_utf8_lossy(&reqs[0]);
    let second = String::from_utf8_lossy(&reqs[1]);
    assert!(
        first.contains("<D:sync-token>http://example.com/sync/stale</D:sync-token>"),
        "first request must carry the stale token: {first}"
    );
    assert!(
        second.contains("<D:sync-token/>"),
        "retry must be an initial sync with an empty token: {second}"
    );
}

#[tokio::test]
async fn sync_collection_with_level_sends_infinite() {
    let head = crate::common::http_helpers::response_head("", INITIAL_SYNC_BODY.len());
    let (base, captured) =
        crate::common::http_helpers::serve_capture(head, INITIAL_SYNC_BODY.as_bytes().to_vec())
            .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection_with_level("cal/", None, None, false, SyncLevel::Infinite)
        .await
        .unwrap();
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://example.com/sync/2")
    );
    assert_eq!(sync.items.len(), 2);

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("<D:sync-level>infinite</D:sync-level>"),
        "expected the configured sync-level on the wire: {req}"
    );
}
#[tokio::test]
async fn delegate_options_sends_options_request() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client.options("cal/").await.unwrap();

    let guard = captured.lock().unwrap();
    let raw = String::from_utf8_lossy(&guard);
    assert!(
        raw.starts_with("OPTIONS "),
        "expected OPTIONS method in request: {raw}"
    );
}

#[tokio::test]
async fn delegate_delete_sends_delete_request() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client.delete("cal/event.ics").await.unwrap();

    let guard = captured.lock().unwrap();
    let raw = String::from_utf8_lossy(&guard);
    assert!(
        raw.starts_with("DELETE "),
        "expected DELETE method in request: {raw}"
    );
}

#[tokio::test]
async fn delegate_copy_sends_copy_with_destination() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client
        .copy("cal/a.ics", &format!("{base}cal/b.ics"), true)
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let raw = String::from_utf8_lossy(&guard);
    assert!(
        raw.starts_with("COPY "),
        "expected COPY method in request: {raw}"
    );
    let lower = raw.to_ascii_lowercase();
    assert!(
        lower.contains("destination: "),
        "expected Destination header: {raw}"
    );
    assert!(
        lower.contains("overwrite: t"),
        "expected 'Overwrite: T' in request: {raw}"
    );
}

#[tokio::test]
async fn delegate_move_sends_move_with_destination() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client
        .r#move("cal/a.ics", &format!("{base}cal/b.ics"), false)
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let raw = String::from_utf8_lossy(&guard);
    let lower = raw.to_ascii_lowercase();
    assert!(
        raw.starts_with("MOVE "),
        "expected MOVE method in request: {raw}"
    );
    assert!(
        lower.contains("destination: "),
        "expected Destination header: {raw}"
    );
}

#[tokio::test]
async fn delegate_request_compression_mode_getter_roundtrips() {
    let client = CalDavClient::new("http://127.0.0.1:1/", None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    assert_eq!(
        client.request_compression_mode(),
        RequestCompressionMode::Disabled
    );

    client.set_request_compression_mode(RequestCompressionMode::Force(
        fast_dav_rs::ContentEncoding::Gzip,
    ));
    assert_eq!(
        client.request_compression_mode(),
        RequestCompressionMode::Force(fast_dav_rs::ContentEncoding::Gzip)
    );
}

// ---------------------------------------------------------------------------
// calendar_multiget_many
// ---------------------------------------------------------------------------

/// One `calendar-multiget` REPORT response listing `hrefs` (optionally with
/// `calendar-data`), as a `(head, body)` pair for the wire helpers.
fn multiget_report_response(hrefs: &[&str], with_data: bool) -> (String, Vec<u8>) {
    let mut responses = String::new();
    for (i, href) in hrefs.iter().enumerate() {
        let data = if with_data {
            format!("<C:calendar-data>BEGIN:VCALENDAR\r\nEND:VCALENDAR-{i}</C:calendar-data>")
        } else {
            String::new()
        };
        responses.push_str(&format!(
            "<D:response><D:href>{href}</D:href><D:propstat><D:prop>\
             <D:getetag>\"etag-{i}\"</D:getetag>{data}</D:prop>\
             <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>"
        ));
    }
    let body = format!(
        "<?xml version=\"1.0\"?>\
         <D:multistatus xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\
         {responses}</D:multistatus>"
    );
    (
        crate::common::http_helpers::response_head("", body.len()),
        body.into_bytes(),
    )
}

#[tokio::test]
async fn multiget_many_chunks_hrefs_and_orders_results() {
    let hrefs: Vec<String> = (0..5).map(|i| format!("/cal/e{i}.ics")).collect();
    let responses = vec![
        multiget_report_response(&["/cal/e0.ics", "/cal/e1.ics"], true),
        multiget_report_response(&["/cal/e2.ics", "/cal/e3.ics"], true),
        multiget_report_response(&["/cal/e4.ics"], true),
    ];
    let (base, captured) = crate::common::http_helpers::serve_sequence(responses).await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let items = client
        .calendar_multiget_many("cal/", &hrefs, true, None, 2, 1)
        .await
        .unwrap();

    // 5 hrefs / batch 2 -> exactly 3 REPORTs, each carrying only its chunk.
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 3, "expected one REPORT per chunk");
    let req1 = String::from_utf8_lossy(&requests[0]);
    assert!(
        req1.contains("calendar-multiget"),
        "expected calendar-multiget report root: {req1}"
    );
    assert!(req1.contains("<D:href>/cal/e0.ics</D:href>"), "{req1}");
    assert!(req1.contains("<D:href>/cal/e1.ics</D:href>"), "{req1}");
    assert!(
        !req1.contains("/cal/e2.ics"),
        "chunk 1 leaked hrefs: {req1}"
    );
    let req2 = String::from_utf8_lossy(&requests[1]);
    assert!(
        req2.contains("/cal/e2.ics") && req2.contains("/cal/e3.ics"),
        "{req2}"
    );
    assert!(
        !req2.contains("/cal/e0.ics"),
        "chunk 2 leaked hrefs: {req2}"
    );
    let req3 = String::from_utf8_lossy(&requests[2]);
    assert!(req3.contains("/cal/e4.ics"), "{req3}");
    drop(requests);

    // Deterministic ordering: chunk index first, then server order in-chunk.
    assert_eq!(items.len(), 5);
    for item in &items {
        assert_eq!(item.pub_path, "cal/");
        assert!(
            item.result.is_ok(),
            "expected Ok item: {:?}",
            item.result.as_ref().err()
        );
    }
    let got: Vec<String> = items
        .iter()
        .map(|i| i.result.as_ref().unwrap().href.clone())
        .collect();
    assert_eq!(
        got,
        vec![
            "/cal/e0.ics",
            "/cal/e1.ics",
            "/cal/e2.ics",
            "/cal/e3.ics",
            "/cal/e4.ics"
        ]
    );
    assert!(items[0].result.as_ref().unwrap().calendar_data.is_some());
}

#[tokio::test]
async fn multiget_many_propagates_expand() {
    let hrefs = vec!["/cal/a.ics".to_string(), "/cal/b.ics".to_string()];
    let (_, body) = multiget_report_response(&["/cal/a.ics", "/cal/b.ics"], false);
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let expand = TimeRange::new("20240101T000000Z").with_end("20240201T000000Z");
    client
        .calendar_multiget_many("cal/", &hrefs, true, Some(expand), 10, 4)
        .await
        .unwrap();

    let guard = captured.lock().unwrap();
    let raw = String::from_utf8_lossy(&guard);
    assert!(
        raw.contains(
            "<C:calendar-data><C:expand start=\"20240101T000000Z\" end=\"20240201T000000Z\"/></C:calendar-data>"
        ),
        "expected expand element in request body: {raw}"
    );
}

#[tokio::test]
async fn multiget_many_partial_failure_isolated() {
    let hrefs: Vec<String> = (0..3).map(|i| format!("/cal/e{i}.ics")).collect();
    let ok500 =
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string();
    let responses = vec![
        multiget_report_response(&["/cal/e0.ics"], true),
        (ok500, Vec::new()),
        multiget_report_response(&["/cal/e2.ics"], true),
    ];
    let (base, _captured) = crate::common::http_helpers::serve_sequence(responses).await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let items = client
        .calendar_multiget_many("cal/", &hrefs, true, None, 1, 1)
        .await
        .unwrap();

    assert_eq!(items.len(), 3, "one BatchItem per chunk result");
    assert!(items[0].result.is_ok());
    assert!(
        matches!(
            &items[1].result,
            Err(Error::UnexpectedStatus {
                operation,
                status,
                ..
            })
                if *operation == fast_dav_rs::Operation::ReportCalendarMultiget
                    && status.as_u16() == 500
        ),
        "expected UnexpectedStatus(500) for the failed chunk, got: {:?}",
        items[1].result
    );
    assert!(items[2].result.is_ok(), "sibling chunk must be unaffected");
}

#[tokio::test]
async fn multiget_many_unparsable_chunk_is_one_batch_error() {
    let hrefs = vec!["/cal/a.ics".to_string(), "/cal/b.ics".to_string()];
    let (head, body) = multiget_report_response(&["/cal/a.ics"], true);
    let garbage = b"<D:multistatus><D:response>".to_vec();
    let responses = vec![
        (head, body),
        (
            crate::common::http_helpers::response_head("", garbage.len()),
            garbage,
        ),
    ];
    let (base, _captured) = crate::common::http_helpers::serve_sequence(responses).await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let items = client
        .calendar_multiget_many("cal/", &hrefs, true, None, 1, 1)
        .await
        .unwrap();

    assert_eq!(items.len(), 2);
    assert!(items[0].result.is_ok());
    assert!(
        items[1].result.is_err(),
        "unparsable chunk must be an error"
    );
}

#[tokio::test]
async fn multiget_many_runs_batches_concurrently() {
    let hrefs = vec!["/cal/a.ics".to_string(), "/cal/b.ics".to_string()];
    // Chunk 1 is delayed; chunk 2 answers immediately. With real concurrency
    // (2 permits) both requests must be in flight before any response is
    // written; a serialized client would answer chunk 1 first.
    let (head1, body1) = multiget_report_response(&["/cal/a.ics"], true);
    let (head2, body2) = multiget_report_response(&["/cal/b.ics"], true);
    let responses = vec![(head1, body1, 300), (head2, body2, 0)];
    let (base, _captured, events) = crate::common::http_helpers::serve_parallel(responses).await;
    let client = CalDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let items = client
        .calendar_multiget_many("cal/", &hrefs, true, None, 1, 2)
        .await
        .unwrap();

    let log = events.lock().unwrap();
    let first_resp = log
        .iter()
        .position(|e| e == "resp")
        .expect("server wrote at least one response");
    assert_eq!(
        log[..first_resp].iter().filter(|e| *e == "req").count(),
        2,
        "both REPORTs must be in flight before the first response: {log:?}"
    );
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.result.is_ok()));
}

#[tokio::test]
async fn multiget_many_rejects_zero_batch_size_before_io() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let Err(err) = client
        .calendar_multiget_many("cal/", &["/cal/a.ics".to_string()], true, None, 0, 4)
        .await
    else {
        panic!("expected batch_size=0 to fail before any network I/O");
    };

    assert!(
        matches!(err, Error::InvalidConfig(ref msg) if msg.contains("batch_size")),
        "expected InvalidConfig for batch_size=0, got: {err:?}"
    );
}

#[tokio::test]
async fn multiget_many_empty_hrefs_returns_empty_without_io() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let items = client
        .calendar_multiget_many("cal/", &[], true, None, 10, 4)
        .await
        .unwrap();

    assert!(items.is_empty());
}

#[tokio::test]
async fn multiget_many_rejects_invalid_expand_before_io() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let Err(err) = client
        .calendar_multiget_many(
            "cal/",
            &["/cal/a.ics".to_string()],
            true,
            Some(TimeRange::new("nope")),
            10,
            4,
        )
        .await
    else {
        panic!("expected invalid expand to fail before any network I/O");
    };

    assert!(
        matches!(err, Error::InvalidDateTime { ref context, .. }
            if context.contains("calendar-multiget expand")),
        "expected InvalidDateTime for expand, got: {err:?}"
    );
}

#[tokio::test]
async fn calendar_query_rejects_exclusive_prop_filters_before_io() {
    // RFC 4791 §9.7.2: `is-not-defined | ((time-range | text-match)?, param-filter*)`.
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let both = CalendarQueryFilter::new("VEVENT").with_prop_filters(vec![
        PropFilter::new("DTSTART", TextMatch::new("x"))
            .with_time_range(TimeRange::new("20240101T000000Z")),
    ]);
    let err = client
        .calendar_query("cal/", &both, true)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref msg) if msg.contains("text-match and time-range are mutually exclusive")),
        "expected InvalidInput for text-match + time-range, got: {err:?}"
    );

    let mut not_defined = PropFilter::not_defined("LOCATION");
    not_defined.param_filters = vec![ParamFilter::not_defined("TYPE")];
    let absent = CalendarQueryFilter::new("VEVENT").with_prop_filters(vec![not_defined]);
    let err = client
        .calendar_query("cal/", &absent, true)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref msg) if msg.contains("is-not-defined excludes")),
        "expected InvalidInput for is-not-defined with children, got: {err:?}"
    );
}
