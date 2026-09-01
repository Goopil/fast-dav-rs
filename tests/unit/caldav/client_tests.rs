use fast_dav_rs::{CalDavClient, Depth, Error, RequestCompressionMode};
use hyper::http::HeaderMap;

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
fn test_build_uri_with_query() {
    let client = CalDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("calendar/?param=value")
        .expect("Failed to build URI");
    assert_eq!(
        uri.to_string(),
        "https://example.com/dav/user/calendar/?param=value"
    );
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
    );
    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("name=\"VEVENT\""));
    assert!(body.contains("start=\"20240101T000000Z\""));
    assert!(body.contains("end=\"20240201T000000Z\""));
}

#[test]
fn test_build_calendar_query_body_no_time_range() {
    let body = fast_dav_rs::caldav::client::build_calendar_query_body("VTODO", None, None, false);
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
    )
    .expect("Should create body");

    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("/calendars/user/event1.ics"));
    assert!(body.contains("event&amp;special.ics")); // Escaped ampersand
}

#[test]
fn test_build_calendar_multiget_empty() {
    let body =
        fast_dav_rs::caldav::client::build_calendar_multiget_body(Vec::<String>::new(), true);
    assert!(body.is_none());
}

#[test]
fn test_build_sync_collection_body() {
    let body = fast_dav_rs::caldav::client::build_sync_collection_body(
        Some("http://example.com/sync-token-123"),
        Some(50),
        true,
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
        .calendar_query_timerange("calendar/", "VEVENT\"><evil/>", None, None, false)
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
        .calendar_query_timerange("calendar/", "", None, None, false)
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

    let body =
        fast_dav_rs::caldav::client::build_sync_collection_body(Some(&normalized), None, true);
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
        .sync_collection("cal/", None, None, true)
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
