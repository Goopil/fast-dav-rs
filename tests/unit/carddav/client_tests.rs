use fast_dav_rs::CardDavClient;
use fast_dav_rs::RequestCompressionMode;
use fast_dav_rs::SyncLevel;
use fast_dav_rs::carddav::Depth;
use hyper::http::HeaderMap;

#[test]
fn test_client_creation() {
    let client = CardDavClient::new("https://example.com/dav/", Some("user"), Some("pass"));
    assert!(client.is_ok());
}

#[test]
fn test_client_without_auth() {
    let client = CardDavClient::new("https://example.com/dav/", None, None);
    assert!(client.is_ok());
}

#[test]
fn test_build_uri_relative() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("addressbook/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/addressbook/");
}

#[test]
fn test_build_uri_absolute() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
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
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("addressbook/?param=value")
        .expect("Failed to build URI");
    assert_eq!(
        uri.to_string(),
        "https://example.com/dav/user/addressbook/%3Fparam=value"
    );
    assert!(uri.query().is_none());
}

#[test]
fn test_build_uri_empty_path() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client.build_uri("").expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/");
}

#[test]
fn test_build_uri_root_path_only() {
    let client =
        CardDavClient::new("https://example.com/", None, None).expect("Failed to create client");

    let uri = client
        .build_uri("addressbook/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/addressbook/");
}

#[test]
fn test_build_uri_with_special_characters() {
    let client = CardDavClient::new("https://example.com/dav/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("my-addressbook_123/")
        .expect("Failed to build URI");
    assert_eq!(
        uri.to_string(),
        "https://example.com/dav/my-addressbook_123/"
    );
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
        fast_dav_rs::carddav::client::escape_xml("Hello & World"),
        "Hello &amp; World"
    );
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("Test <tag>"),
        "Test &lt;tag&gt;"
    );
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("\"quotes\""),
        "&quot;quotes&quot;"
    );
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("'apos'"),
        "&apos;apos&apos;"
    );
}

#[test]
fn test_escape_xml_complex() {
    let input = "Mix & match <tag attr=\"value\"> with 'quotes'";
    let expected = "Mix &amp; match &lt;tag attr=&quot;value&quot;&gt; with &apos;quotes&apos;";
    assert_eq!(fast_dav_rs::carddav::client::escape_xml(input), expected);
}

#[test]
fn test_escape_xml_empty() {
    assert_eq!(fast_dav_rs::carddav::client::escape_xml(""), "");
}

#[test]
fn test_escape_xml_no_special_chars() {
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("normal text"),
        "normal text"
    );
}

#[test]
fn test_escape_xml_multiple_same_char() {
    assert_eq!(
        fast_dav_rs::carddav::client::escape_xml("&&&&"),
        "&amp;&amp;&amp;&amp;"
    );
}

#[test]
fn test_build_addressbook_query_body() {
    let filter = fast_dav_rs::carddav::client::build_addressbook_query_filter_uid("user-123");
    let body = fast_dav_rs::carddav::client::build_addressbook_query_body(&filter, true);
    assert!(body.contains("<C:addressbook-query"));
    assert!(body.contains("<C:address-data/>"));
    assert!(body.contains("prop-filter name=\"UID\""));
    assert!(body.contains("user-123"));
}

#[test]
fn test_build_addressbook_query_filters() {
    let email_filter =
        fast_dav_rs::carddav::client::build_addressbook_query_filter_email("user@example.com");
    assert!(email_filter.contains("prop-filter name=\"EMAIL\""));
    assert!(email_filter.contains("user@example.com"));

    let fn_filter = fast_dav_rs::carddav::client::build_addressbook_query_filter_fn("Ada Lovelace");
    assert!(fn_filter.contains("prop-filter name=\"FN\""));
    assert!(fn_filter.contains("Ada Lovelace"));
}

#[test]
fn test_build_addressbook_multiget_and_escapes() {
    let body = fast_dav_rs::carddav::client::build_addressbook_multiget_body(
        vec![
            "/addressbooks/user/contact1.vcf",
            "/addressbooks/user/contact&special.vcf",
        ],
        true,
    )
    .expect("Should create body");

    assert!(body.contains("<C:address-data/>"));
    assert!(body.contains("/addressbooks/user/contact1.vcf"));
    assert!(body.contains("contact&amp;special.vcf")); // Escaped ampersand
}

#[test]
fn test_build_addressbook_multiget_empty() {
    let body =
        fast_dav_rs::carddav::client::build_addressbook_multiget_body(Vec::<String>::new(), true);
    assert!(body.is_none());
}

#[test]
fn test_build_sync_collection_body() {
    let body = fast_dav_rs::carddav::client::build_sync_collection_body(
        Some("http://example.com/sync-token-123"),
        Some(50),
        true,
    );

    assert!(body.contains("<D:sync-token>http://example.com/sync-token-123</D:sync-token>"));
    assert!(body.contains("<C:address-data/>"));
    assert!(body.contains("<D:nresults>50</D:nresults>"));
}

#[test]
fn test_map_addressbook_list_filters_addressbooks() {
    let mut item = fast_dav_rs::carddav::types::DavItem::new();
    item.href = "/addressbooks/user/personal/".to_string();
    item.displayname = Some("Personal".to_string());
    item.is_addressbook = true;

    let mut collection_item = fast_dav_rs::carddav::types::DavItem::new();
    collection_item.href = "/addressbooks/user/collection/".to_string();
    collection_item.displayname = Some("Collection".to_string());
    collection_item.is_collection = true;

    let items = vec![item.clone(), collection_item.clone()];
    let books = fast_dav_rs::carddav::client::map_addressbook_list(items);

    assert_eq!(books.len(), 1);
    assert_eq!(books[0].href, "/addressbooks/user/personal/");
    assert_eq!(books[0].displayname, Some("Personal".to_string()));
}

#[test]
fn test_map_address_objects() {
    let mut item1 = fast_dav_rs::carddav::types::DavItem::new();
    item1.href = "/addressbooks/user/contact1.vcf".to_string();
    item1.etag = Some("abc123".to_string());
    item1.address_data = Some("BEGIN:VCARD...END:VCARD".to_string());

    let mut item2 = fast_dav_rs::carddav::types::DavItem::new();
    item2.href = "/addressbooks/user/contact2.vcf".to_string();
    item2.etag = Some("def456".to_string());
    item2.status = Some("HTTP/1.1 404 Not Found".to_string());

    let items = vec![item1.clone(), item2.clone()];
    let objects = fast_dav_rs::carddav::client::map_address_objects(items);

    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].href, "/addressbooks/user/contact1.vcf");
    assert_eq!(objects[0].etag, Some("abc123".to_string()));
    assert_eq!(
        objects[0].address_data,
        Some("BEGIN:VCARD...END:VCARD".to_string())
    );
    assert_eq!(objects[1].href, "/addressbooks/user/contact2.vcf");
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

    let mut item1 = fast_dav_rs::carddav::types::DavItem::new();
    item1.href = "/addressbooks/user/contact1.vcf".to_string();
    item1.etag = Some("abc123".to_string());
    item1.address_data = Some("BEGIN:VCARD...END:VCARD".to_string());

    let mut item2 = fast_dav_rs::carddav::types::DavItem::new();
    item2.href = "/addressbooks/user/contact2.vcf".to_string();
    item2.status = Some("HTTP/1.1 404 Not Found".to_string());

    let mut collection_item = fast_dav_rs::carddav::types::DavItem::new();
    collection_item.href = "/addressbooks/user/subbook/".to_string();
    collection_item.sync_token = Some("http://example.com/sync-token-789".to_string());
    collection_item.is_collection = true;

    let items = vec![item1, item2, collection_item];
    let response = fast_dav_rs::carddav::client::map_sync_response(&headers, items, None);

    assert_eq!(
        response.sync_token,
        Some("http://example.com/sync-token-456".to_string())
    );
    assert_eq!(response.items.len(), 2); // Collection item should be filtered out

    // Check the first item (regular item with data)
    assert_eq!(response.items[0].href, "/addressbooks/user/contact1.vcf");
    assert_eq!(response.items[0].etag, Some("abc123".to_string()));
    assert!(!response.items[0].is_deleted); // Should not be deleted

    // Check second item (deleted item)
    assert_eq!(response.items[1].href, "/addressbooks/user/contact2.vcf");
    assert_eq!(
        response.items[1].status,
        Some("HTTP/1.1 404 Not Found".to_string())
    );
    assert!(response.items[1].is_deleted); // Should be marked as deleted
}

#[test]
fn builder_propagates_options() {
    use fast_dav_rs::RequestCompressionMode;
    use std::time::Duration;

    let client = CardDavClient::builder("https://card.example.com/dav/")
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
    let result = CardDavClient::builder("not a valid url").build();
    assert!(result.is_err());
}

#[test]
fn builder_bearer_auth() {
    let client = CardDavClient::builder("https://card.example.com/dav/")
        .bearer_token("test-token")
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn clone_shares_compression_mode() {
    use fast_dav_rs::RequestCompressionMode;

    let client_a = CardDavClient::builder("https://card.example.com/dav/")
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
    let sync = fast_dav_rs::carddav::client::map_sync_response(&headers, Vec::new(), None);
    let normalized = sync.sync_token.expect("sync token present");
    assert_eq!(normalized, "http://example.com/sync/99");

    let body =
        fast_dav_rs::carddav::client::build_sync_collection_body(Some(&normalized), None, true);
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
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection("contacts/", None, None, true)
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
    <D:href>/contacts/a.vcf</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/contacts/</D:href>
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
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection("contacts/", None, None, false)
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
        .find(|i| i.href == "/contacts/")
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
    <D:href>/contacts/a.vcf</D:href>
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
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection("contacts/", None, None, false)
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
async fn mkaddressbook_sends_depth_zero() {
    let body = b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"></D:multistatus>".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .mkaddressbook(
            "newab/",
            r#"<C:mkaddressbook xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav"><D:set><D:prop><D:displayname>New</D:displayname></D:prop></D:set></C:mkaddressbook>"#,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("MKADDRESSBOOK"),
        "expected MKADDRESSBOOK method in request: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected explicit 'Depth: 0' on MKADDRESSBOOK: {req}"
    );
}

#[tokio::test]
async fn list_addressbooks_requests_apple_color_not_carddav_color() {
    let body = b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"></D:multistatus>".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let addressbooks = client.list_addressbooks("home/").await.unwrap();
    assert!(addressbooks.is_empty());

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("<A:addressbook-color/>"),
        "the Apple addressbook-color property must be requested: {req}"
    );
    assert!(
        !req.contains("<C:addressbook-color/>"),
        "the non-existent CardDAV addressbook-color property must not be requested: {req}"
    );
}

#[tokio::test]
async fn carddav_follow_redirects_false_propagates() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/never/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;

    let client = CardDavClient::builder(&base)
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
    <D:href>/contacts/a.vcf</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/contacts/b.vcf</D:href>
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
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection_resilient(
            "contacts/",
            Some("http://example.com/sync/stale"),
            None,
            true,
        )
        .await
        .unwrap();

    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://example.com/sync/2")
    );
    assert_eq!(sync.items.len(), 2);
    assert_eq!(sync.items[0].href, "/contacts/a.vcf");
    assert_eq!(sync.items[0].etag.as_deref(), Some("etag-a"));
    assert!(!sync.items[0].is_deleted);
    assert!(sync.resynced, "410-triggered initial sync must be flagged");

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
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let sync = client
        .sync_collection_with_level("contacts/", None, None, false, SyncLevel::Infinite)
        .await
        .unwrap();
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://example.com/sync/2")
    );
    assert_eq!(sync.items.len(), 2);
    assert!(!sync.resynced, "an incremental sync is never a resync");

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw);
    assert!(
        req.contains("<D:sync-level>infinite</D:sync-level>"),
        "expected the configured sync-level on the wire: {req}"
    );
}

#[tokio::test]
async fn carddav_put_never_validates_body_as_icalendar() {
    use bytes::Bytes;

    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    // Neither a valid vCard nor a valid iCalendar — CardDAV bodies are never
    // iCalendar-validated, so the request must go out as-is.
    let resp = client
        .put("contact.vcf", Bytes::from_static(b"total garbage \xFF"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(req.starts_with("PUT "), "request must be sent: {req}");
}

#[tokio::test]
async fn addressbook_multiget_sends_depth_zero() {
    let body = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/contacts/a.vcf</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body.as_bytes().to_vec(),
    )
    .await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let contacts = client
        .addressbook_multiget("contacts/", ["/contacts/a.vcf"], true)
        .await
        .unwrap();
    assert_eq!(contacts.len(), 1);

    let raw = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&raw).to_ascii_lowercase();
    assert!(
        req.contains("depth: 0") && !req.contains("depth: 1"),
        "addressbook-multiget must use Depth: 0 (RFC 6352 §8.7): {req}"
    );
}

/// One `addressbook-multiget` REPORT response listing `hrefs` (optionally
/// with `address-data`), as a `(head, body)` pair for the wire helpers.
fn addressbook_multiget_report_response(hrefs: &[&str], with_data: bool) -> (String, Vec<u8>) {
    let mut responses = String::new();
    for (i, href) in hrefs.iter().enumerate() {
        let data = if with_data {
            format!("<C:address-data>BEGIN:VCARD\r\nEND:VCARD-{i}</C:address-data>")
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
         <D:multistatus xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:carddav\">\
         {responses}</D:multistatus>"
    );
    (
        crate::common::http_helpers::response_head("", body.len()),
        body.into_bytes(),
    )
}

#[tokio::test]
async fn addressbook_multiget_many_chunks_hrefs_and_orders_results() {
    let hrefs: Vec<String> = (0..5).map(|i| format!("/contacts/c{i}.vcf")).collect();
    let responses = vec![
        addressbook_multiget_report_response(&["/contacts/c0.vcf", "/contacts/c1.vcf"], true),
        addressbook_multiget_report_response(&["/contacts/c2.vcf", "/contacts/c3.vcf"], true),
        addressbook_multiget_report_response(&["/contacts/c4.vcf"], true),
    ];
    let (base, captured) = crate::common::http_helpers::serve_sequence(responses).await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let items = client
        .addressbook_multiget_many("contacts/", &hrefs, true, 2, 1)
        .await
        .unwrap();

    // 5 hrefs / batch 2 -> exactly 3 REPORTs, each carrying only its chunk.
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 3, "expected one REPORT per chunk");
    let req1 = String::from_utf8_lossy(&requests[0]);
    assert!(
        req1.contains("addressbook-multiget"),
        "expected addressbook-multiget report root: {req1}"
    );
    assert!(
        req1.to_ascii_lowercase().contains("depth: 0"),
        "multiget REPORTs must use Depth: 0 (RFC 6352 §8.7): {req1}"
    );
    assert!(
        req1.contains("<D:href>/contacts/c0.vcf</D:href>")
            && req1.contains("<D:href>/contacts/c1.vcf</D:href>"),
        "{req1}"
    );
    assert!(
        !req1.contains("/contacts/c2.vcf"),
        "chunk 1 leaked hrefs: {req1}"
    );
    let req2 = String::from_utf8_lossy(&requests[1]);
    assert!(
        req2.contains("/contacts/c2.vcf") && req2.contains("/contacts/c3.vcf"),
        "{req2}"
    );
    let req3 = String::from_utf8_lossy(&requests[2]);
    assert!(req3.contains("/contacts/c4.vcf"), "{req3}");
    drop(requests);

    // Deterministic ordering: chunk index first, then server order in-chunk.
    assert_eq!(items.len(), 5);
    for item in &items {
        assert_eq!(item.pub_path, "contacts/");
        assert!(
            item.result.is_ok(),
            "expected Ok item: {:?}",
            item.result.as_ref().err()
        );
    }
    // Each BatchItem carries the hrefs its chunk's REPORT requested.
    assert_eq!(
        items[0].hrefs,
        vec![
            "/contacts/c0.vcf".to_string(),
            "/contacts/c1.vcf".to_string()
        ]
    );
    assert_eq!(
        items[2].hrefs,
        vec![
            "/contacts/c2.vcf".to_string(),
            "/contacts/c3.vcf".to_string()
        ]
    );
    assert_eq!(items[4].hrefs, vec!["/contacts/c4.vcf".to_string()]);
    // A compliant server echoes every requested href: no missing hrefs.
    assert!(items.iter().all(|i| i.missing_hrefs.is_empty()));
    let got: Vec<String> = items
        .iter()
        .map(|i| i.result.as_ref().unwrap().href.clone())
        .collect();
    assert_eq!(
        got,
        vec![
            "/contacts/c0.vcf",
            "/contacts/c1.vcf",
            "/contacts/c2.vcf",
            "/contacts/c3.vcf",
            "/contacts/c4.vcf"
        ]
    );
    assert!(
        items[0].result.as_ref().unwrap().address_data.is_some(),
        "include_data = true must return address-data"
    );
}

#[tokio::test]
async fn addressbook_multiget_many_partial_failure_isolated() {
    let hrefs: Vec<String> = (0..3).map(|i| format!("/contacts/c{i}.vcf")).collect();
    let ok500 =
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string();
    let responses = vec![
        addressbook_multiget_report_response(&["/contacts/c0.vcf"], true),
        (ok500, Vec::new()),
        addressbook_multiget_report_response(&["/contacts/c2.vcf"], true),
    ];
    let (base, _captured) = crate::common::http_helpers::serve_sequence(responses).await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let items = client
        .addressbook_multiget_many("contacts/", &hrefs, true, 1, 1)
        .await
        .unwrap();

    assert_eq!(items.len(), 3, "one BatchItem per chunk result");
    assert!(items[0].result.is_ok());
    assert!(
        matches!(
            &items[1].result,
            Err(fast_dav_rs::Error::UnexpectedStatus {
                operation,
                status,
                ..
            })
                if *operation == fast_dav_rs::Operation::ReportAddressbookMultiget
                    && status.as_u16() == 500
        ),
        "expected UnexpectedStatus(500) for the failed chunk, got: {:?}",
        items[1].result
    );
    assert!(
        items[1].hrefs == vec!["/contacts/c1.vcf".to_string()],
        "a failed chunk must be attributable to its requested hrefs, got: {:?}",
        items[1].hrefs
    );
    assert!(items[2].result.is_ok(), "sibling chunk must be unaffected");
}

#[tokio::test]
async fn addressbook_multiget_many_reports_missing_hrefs() {
    // The mock answers only 2 of the 3 requested hrefs; the absent one must
    // be surfaced via `BatchItem::missing_hrefs` (exact href string
    // comparison) while the answered objects are still delivered.
    let hrefs = vec![
        "/contacts/a.vcf".to_string(),
        "/contacts/b.vcf".to_string(),
        "/contacts/missing.vcf".to_string(),
    ];
    let (head, body) =
        addressbook_multiget_report_response(&["/contacts/a.vcf", "/contacts/b.vcf"], true);
    let base = crate::common::http_helpers::serve_once(head, body).await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let items = client
        .addressbook_multiget_many("contacts/", &hrefs, true, 3, 2)
        .await
        .unwrap();

    let mut missing: Vec<String> = items
        .iter()
        .flat_map(|b| b.missing_hrefs.iter().cloned())
        .collect();
    missing.sort();
    missing.dedup();
    assert_eq!(
        missing,
        vec!["/contacts/missing.vcf".to_string()],
        "the href the server omitted must be reported in missing_hrefs"
    );
    let got: Vec<String> = items
        .iter()
        .map(|i| {
            i.result
                .as_ref()
                .expect("answered objects must be Ok")
                .href
                .clone()
        })
        .collect();
    assert_eq!(
        got,
        vec!["/contacts/a.vcf".to_string(), "/contacts/b.vcf".to_string()]
    );
}

#[tokio::test]
async fn addressbook_multiget_many_rejects_zero_batch_size_before_io() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CardDavClient::new(&base, None, None).unwrap();

    let Err(err) = client
        .addressbook_multiget_many("contacts/", &["/contacts/a.vcf".to_string()], true, 0, 4)
        .await
    else {
        panic!("expected batch_size=0 to fail before any network I/O");
    };

    assert!(
        matches!(err, fast_dav_rs::Error::InvalidConfig(ref msg) if msg.contains("batch_size")),
        "expected InvalidConfig for batch_size=0, got: {err:?}"
    );
}

#[tokio::test]
async fn addressbook_multiget_many_empty_hrefs_returns_empty_without_io() {
    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CardDavClient::new(&base, None, None).unwrap();

    assert!(
        client
            .addressbook_multiget_many("contacts/", &[], true, 10, 4)
            .await
            .unwrap()
            .is_empty(),
        "empty input must produce no batches"
    );
    // Empty hrefs are filtered before chunking: an all-empty input produces
    // no batches either.
    assert!(
        client
            .addressbook_multiget_many("contacts/", &["".to_string(), String::new()], true, 10, 4)
            .await
            .unwrap()
            .is_empty(),
        "all-empty input must produce no batches"
    );
}

#[tokio::test]
async fn carddav_put_derives_content_type_version_from_body() {
    use bytes::Bytes;

    let responses = vec![
        (
            crate::common::http_helpers::response_head("", 0),
            Vec::new(),
        );
        3
    ];
    let (base, captured) = crate::common::http_helpers::serve_sequence(responses).await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client
        .put(
            "a.vcf",
            Bytes::from_static(b"BEGIN:VCARD\r\nVERSION:3.0\r\nFN:A\r\nEND:VCARD\r\n"),
        )
        .await
        .unwrap();
    client
        .put(
            "b.vcf",
            Bytes::from_static(b"BEGIN:VCARD\r\nversion:4.0\r\nFN:B\r\nEND:VCARD\r\n"),
        )
        .await
        .unwrap();
    // No VERSION property: falls back to the default version=4.0.
    client
        .put(
            "c.vcf",
            Bytes::from_static(b"BEGIN:VCARD\r\nFN:C\r\nEND:VCARD\r\n"),
        )
        .await
        .unwrap();

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "one PUT per body");
    let req3 = String::from_utf8_lossy(&reqs[0]).to_ascii_lowercase();
    assert!(
        req3.contains("content-type: text/vcard; charset=utf-8; version=3.0"),
        "a vCard 3.0 body must carry version=3.0: {req3}"
    );
    let req4 = String::from_utf8_lossy(&reqs[1]).to_ascii_lowercase();
    assert!(
        req4.contains("content-type: text/vcard; charset=utf-8; version=4.0"),
        "a vCard 4.0 body must carry version=4.0: {req4}"
    );
    let reqd = String::from_utf8_lossy(&reqs[2]).to_ascii_lowercase();
    assert!(
        reqd.contains("content-type: text/vcard; charset=utf-8; version=4.0"),
        "a body without VERSION must default to version=4.0: {reqd}"
    );
}

#[tokio::test]
async fn carddav_put_if_match_derives_content_type_version_from_body() {
    use bytes::Bytes;

    let responses = vec![(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )];
    let (base, captured) = crate::common::http_helpers::serve_sequence(responses).await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    client
        .put_if_match(
            "a.vcf",
            Bytes::from_static(b"BEGIN:VCARD\r\nVERSION:3.0\r\nFN:A\r\nEND:VCARD\r\n"),
            "\"etag\"",
        )
        .await
        .unwrap();

    let reqs = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&reqs[0]).to_ascii_lowercase();
    assert!(
        req.contains("content-type: text/vcard; charset=utf-8; version=3.0"),
        "a conditional PUT of a vCard 3.0 body must carry version=3.0: {req}"
    );
}

#[tokio::test]
async fn addressbook_query_filter_rejects_param_filter_exclusivity_before_io() {
    // RFC 6352 §10.5.2: `param-filter (is-not-defined | text-match?)` — a
    // param-filter `is-not-defined` excludes a nested text-match. Must be
    // rejected before any network I/O.
    use fast_dav_rs::carddav::types::{CardDavFilter, ParamFilter, TextMatch};

    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CardDavClient::new(&base, None, None).unwrap();

    let mut param = ParamFilter::new("TYPE", TextMatch::new("work"));
    param.is_not_defined = true;
    let filter = CardDavFilter::new("TEL", "ignored").with_param_filters(vec![param]);

    let Err(err) = client
        .addressbook_query_filter("contacts/", &filter, true)
        .await
    else {
        panic!("param-filter is-not-defined + text-match must be rejected before any network I/O");
    };
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(ref msg)
            if msg.contains("param-filter") && msg.contains("§10.5.2")),
        "param-filter is-not-defined + text-match must be rejected, got: {err:?}"
    );
}

#[tokio::test]
async fn addressbook_query_filter_rejects_prop_filter_exclusivity_before_io() {
    // RFC 6352 §10.5.1: `prop-filter (is-not-defined | (text-match*,
    // param-filter*))` — a prop-filter `is-not-defined` excludes both the
    // text-match child and any param-filter children. Must be rejected
    // before any network I/O.
    use fast_dav_rs::carddav::types::{CardDavFilter, ParamFilter, TextMatch};

    let base = crate::common::http_helpers::unreachable_base().await;
    let client = CardDavClient::new(&base, None, None).unwrap();

    // is-not-defined + a nested param-filter → rejected (§10.5.1).
    let filter = CardDavFilter::new("EMAIL", "")
        .with_is_not_defined(true)
        .with_param_filters(vec![ParamFilter::new("TYPE", TextMatch::new("work"))]);
    let Err(err) = client
        .addressbook_query_filter("contacts/", &filter, true)
        .await
    else {
        panic!("prop-filter is-not-defined + param-filter must be rejected before any network I/O");
    };
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(ref msg)
            if msg.contains("prop-filter") && msg.contains("§10.5.1")),
        "prop-filter is-not-defined + param-filter must be rejected, got: {err:?}"
    );

    // is-not-defined + a text-match value → also rejected (§10.5.1).
    let filter = CardDavFilter::new("EMAIL", "alice@example.com").with_is_not_defined(true);
    let Err(err) = client
        .addressbook_query_filter("contacts/", &filter, true)
        .await
    else {
        panic!("prop-filter is-not-defined + text-match must be rejected before any network I/O");
    };
    assert!(
        matches!(err, fast_dav_rs::Error::InvalidInput(ref msg)
            if msg.contains("prop-filter") && msg.contains("§10.5.1")),
        "prop-filter is-not-defined + text-match must be rejected, got: {err:?}"
    );

    // Bare `is-not-defined` (no text-match value, no param-filters) is
    // DTD-valid: it must pass validation and proceed to I/O (fails here
    // only because the base is unreachable).
    let filter = CardDavFilter::new("EMAIL", "").with_is_not_defined(true);
    let Err(err) = client
        .addressbook_query_filter("contacts/", &filter, true)
        .await
    else {
        panic!("bare prop-filter is-not-defined is DTD-valid");
    };
    assert!(
        !matches!(err, fast_dav_rs::Error::InvalidInput(_)),
        "bare prop-filter is-not-defined must not be rejected as invalid input, got: {err:?}"
    );
}

#[tokio::test]
async fn addressbook_query_filter_sends_structured_filter_report() {
    use fast_dav_rs::carddav::types::{CardDavFilter, ParamFilter, TextMatch};

    let body = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/contacts/a.vcf</D:href>
    <D:propstat>
      <D:prop><D:getetag>"etag-a"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body.as_bytes().to_vec(),
    )
    .await;
    let client = CardDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let filter = CardDavFilter::new("EMAIL", "ada@example.com")
        .with_param_filters(vec![ParamFilter::new("TYPE", TextMatch::new("work"))]);
    let contacts = client
        .addressbook_query_filter("contacts/", &filter, true)
        .await
        .unwrap();
    assert_eq!(contacts.len(), 1);

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("addressbook-query"),
        "expected addressbook-query REPORT: {req}"
    );
    assert!(
        req.contains("prop-filter name=\"EMAIL\""),
        "expected the structured prop-filter on the wire: {req}"
    );
    assert!(
        req.contains("param-filter name=\"TYPE\""),
        "expected the nested param-filter on the wire: {req}"
    );
}
