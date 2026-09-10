use bytes::Bytes;
use fast_dav_rs::Error;
use fast_dav_rs::webdav::streaming::{
    DavStreamEvent, decode_text, multistatus_events, parse_multistatus_bytes,
    parse_multistatus_stream_visit, parse_multistatus_stream_visit_with_timeout,
};
use fast_dav_rs::{
    ContentEncoding, Depth, Operation, RequestCompressionMode, WebDavClient, compress_payload,
};
use futures::StreamExt;
use hyper::{HeaderMap, Method};
use std::sync::Arc;
use std::time::Duration;

use crate::common::http_helpers::{response_head, serve_once, serve_stalled};

/// XML containing a self-closing element (Empty event), CDATA text and a sync token.
const RICH_MULTISTATUS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:CS="http://calendarserver.org/ns/">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/></D:resourcetype>
        <D:displayname><![CDATA[My & Calendar]]></D:displayname>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>https://example.com/sync/42</D:sync-token>
</D:multistatus>"#;

#[tokio::test]
async fn visit_parses_stream_with_empty_events_cdata_and_sync_token() {
    let base = serve_once(
        response_head("", RICH_MULTISTATUS.len()),
        RICH_MULTISTATUS.as_bytes().to_vec(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();

    let mut items = Vec::new();
    let sync_token = parse_multistatus_stream_visit(resp.into_body(), &[], |item| {
        items.push(item);
        Ok(())
    })
    .await
    .unwrap();

    assert_eq!(sync_token.as_deref(), Some("https://example.com/sync/42"));
    assert_eq!(items.len(), 1);
    assert!(items[0].is_collection);
    assert_eq!(items[0].displayname.as_deref(), Some("My & Calendar"));
}

#[tokio::test]
async fn visit_with_custom_timeout_parses_stream() {
    let base = serve_once(
        response_head("", RICH_MULTISTATUS.len()),
        RICH_MULTISTATUS.as_bytes().to_vec(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    let sync_token = parse_multistatus_stream_visit_with_timeout(
        resp.into_body(),
        &[],
        Duration::from_secs(30),
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(sync_token.as_deref(), Some("https://example.com/sync/42"));
}

#[tokio::test]
async fn visit_malformed_xml_returns_error() {
    let base = serve_once(
        response_head("", 34),
        b"<D:multistatus><D:response><D:prop".to_vec(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    let result = parse_multistatus_stream_visit(resp.into_body(), &[], |_| Ok(())).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn send_decompresses_gzip_and_normalizes_headers() {
    let raw = RICH_MULTISTATUS.as_bytes();
    let compressed = compress_payload(Bytes::from(raw.to_vec()), ContentEncoding::Gzip)
        .await
        .unwrap();
    let base = serve_once(
        response_head("Content-Encoding: gzip\r\n", compressed.len()),
        compressed.to_vec(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();

    assert!(
        resp.headers()
            .get(hyper::header::CONTENT_ENCODING)
            .is_none()
    );
    assert_eq!(
        resp.headers().get(hyper::header::CONTENT_LENGTH).unwrap(),
        &raw.len().to_string()
    );
    assert_eq!(resp.body().as_ref(), raw);
}

#[tokio::test]
async fn discover_current_user_principal_skips_empty_href() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href></D:href></D:current-user-principal>
        <D:displayname></D:displayname>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/other/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href>/principals/user/</D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let base = serve_once(response_head("", xml.len()), xml.as_bytes().to_vec()).await;
    let client = WebDavClient::builder(&base)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let principal = client.discover_current_user_principal().await.unwrap();
    assert_eq!(principal.as_deref(), Some("/principals/user/"));
}

#[test]
fn bytes_parse_reads_supported_address_data_attrs() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:A="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/books/</D:href>
    <D:propstat>
      <D:prop>
        <A:supported-address-data>
          <A:address-data-type content-type="text/vcard" version="4.0"/>
        </A:supported-address-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert!(
        result.items[0]
            .supported_address_data
            .iter()
            .any(|v| v.contains("text/vcard") && v.contains("version=4.0"))
    );
}

#[test]
fn bytes_parse_reads_managed_ids() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/event.ics</D:href>
    <D:propstat>
      <D:prop>
        <C:managed-ids>
          <C:managed-id>abc</C:managed-id>
          <C:managed-id>def-42</C:managed-id>
        </C:managed-ids>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].managed_ids, vec!["abc", "def-42"]);
}

#[tokio::test]
async fn visit_propagates_sink_error() {
    let base = serve_once(
        response_head("", RICH_MULTISTATUS.len()),
        RICH_MULTISTATUS.as_bytes().to_vec(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    let result =
        parse_multistatus_stream_visit(resp.into_body(), &[], |_| Err(Error::other("sink failed")))
            .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn unreachable_server_returns_errors_from_all_verbs() {
    let client = WebDavClient::new("http://127.0.0.1:1/", None, None).unwrap();

    assert!(client.head("").await.is_err());
    assert!(client.get("").await.is_err());
    assert!(
        client
            .send_stream(Method::GET, "", HeaderMap::new(), None, None)
            .await
            .is_err()
    );
    assert!(
        client
            .copy("", "http://127.0.0.1:1/dest", false)
            .await
            .is_err()
    );
    assert!(
        client
            .r#move("", "http://127.0.0.1:1/dest", true)
            .await
            .is_err()
    );

    let body = Arc::new(Bytes::from("<propfind/>"));
    let results = client
        .propfind_many(vec!["a".into(), "b".into()], Depth::One, body.clone(), 2)
        .await;
    assert!(results.iter().all(|b| b.result.is_err()));
    assert_eq!(results[0].pub_path, "a");
    assert_eq!(results[0].hrefs, vec!["a".to_string()]);
    assert_eq!(results[1].hrefs, vec!["b".to_string()]);

    let results = client
        .report_many(vec!["a".into()], Depth::One, body, 1)
        .await;
    assert!(results.iter().all(|b| b.result.is_err()));
    assert_eq!(results[0].hrefs, vec!["a".to_string()]);
}

#[tokio::test]
async fn send_returns_timeout_when_response_body_stalls() {
    let head = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n";
    let base = crate::common::http_helpers::serve_stalled(head.to_string(), b"partial").await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let err = client
        .send(
            Method::GET,
            "",
            HeaderMap::new(),
            None,
            Some(Duration::from_millis(200)),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::Timeout { .. }),
        "expected Timeout, got: {err:?}"
    );
}

#[test]
fn decode_text_unknown_entity_passes_through_literally() {
    // Servers emit entities outside the predefined XML set (e.g. `&nbsp;`);
    // they must surface as literal text instead of aborting the parse.
    assert_eq!(decode_text(b"Cal&nbsp;1").unwrap(), "Cal&nbsp;1");
    assert_eq!(decode_text(b"&foo;").unwrap(), "&foo;");
}

#[test]
fn decode_text_mixed_unknown_and_known_entities() {
    assert_eq!(
        decode_text(b"Cal&nbsp;1 &amp; x &lt;y&gt;").unwrap(),
        "Cal&nbsp;1 & x <y>"
    );
}

#[test]
fn decode_text_numeric_entities_still_resolve() {
    assert_eq!(decode_text(b"&#65;&#x42;").unwrap(), "AB");
}

#[test]
fn decode_text_malformed_numeric_entity_still_errors() {
    assert!(decode_text(b"&#xZZ;").is_err());
    assert!(decode_text(b"&#999999999999;").is_err());
}

#[test]
fn bytes_parse_displayname_with_unknown_entity() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop><D:displayname>Cal&nbsp;1</D:displayname></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].displayname.as_deref(), Some("Cal&nbsp;1"));
}

/// A foreign-namespace element whose local name collides with a DAV element
/// must not be interpreted as a DAV element.
#[test]
fn foreign_namespace_elements_are_not_dav_elements() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/real.ics</D:href>
    <x:href xmlns:x="urn:mal">/injected/pwned</x:href>
    <D:propstat>
      <D:prop><D:getetag>"1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].href, "/cal/real.ics");
}

#[test]
fn foreign_namespace_status_is_not_dav_status() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop><D:getetag>"1"</D:getetag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
      <x:status xmlns:x="urn:mal">HTTP/1.1 500 Server Error</x:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].status.as_deref(), Some("HTTP/1.1 200 OK"));
}

#[test]
fn default_dav_namespace_elements_are_parsed() {
    let xml = br#"<?xml version="1.0"?>
<multistatus xmlns="DAV:">
  <response>
    <href>/cal/a.ics</href>
    <propstat>
      <prop><getetag>"1"</getetag></prop>
      <status>HTTP/1.1 200 OK</status>
    </propstat>
  </response>
</multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].href, "/cal/a.ics");
}

#[test]
fn caldav_namespace_calendar_data_is_parsed() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-data>BEGIN:VCALENDAR
END:VCALENDAR
        </C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert!(
        result.items[0]
            .calendar_data
            .as_deref()
            .unwrap_or_default()
            .contains("BEGIN:VCALENDAR")
    );
}

/// Element matching is ASCII-case-insensitive by design (documented
/// tolerance for non-canonical server element-name casing).
#[test]
fn uppercase_element_names_are_matched() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:RESPONSE>
    <D:HREF>/cal/a.ics</D:HREF>
    <D:PROPSTAT>
      <D:PROP><D:GETETAG>"1"</D:GETETAG></D:PROP>
      <D:STATUS>HTTP/1.1 200 OK</D:STATUS>
    </D:PROPSTAT>
  </D:RESPONSE>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].href, "/cal/a.ics");
    assert_eq!(result.items[0].etag.as_deref(), Some("1"));
}

/// The streaming event engine yields the sync token and each completed
/// `<D:response>` as separate events, in document order (RFC 6578 servers
/// commonly emit the sync token first).
#[tokio::test]
async fn events_stream_items_in_order_with_leading_sync_token() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:sync-token>https://example.com/sync/42</D:sync-token>
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:response><D:href>/cal/b.ics</D:href><D:propstat><D:prop><D:getetag>"b"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
</D:multistatus>"#;
    let base = serve_once(response_head("", xml.len()), xml.as_bytes().to_vec()).await;
    let client = WebDavClient::new(&base, None, None).unwrap();
    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();

    let stream = multistatus_events(resp.into_body(), &[]);
    futures::pin_mut!(stream);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    assert_eq!(events.len(), 3, "token + 2 items expected: {events:?}");
    assert!(
        matches!(&events[0], DavStreamEvent::SyncToken(t) if t == "https://example.com/sync/42"),
        "sync token must come first, got: {:?}",
        events[0]
    );
    assert!(
        matches!(&events[1], DavStreamEvent::Item(i) if i.href == "/cal/a.ics" && i.etag.as_deref() == Some("a")),
        "first item must be a.ics, got: {:?}",
        events[1]
    );
    assert!(
        matches!(&events[2], DavStreamEvent::Item(i) if i.href == "/cal/b.ics"),
        "second item must be b.ics, got: {:?}",
        events[2]
    );
}

/// A trailing sync token (RFC 6578 §3.6 example layout) is emitted as its own
/// event, after the items.
#[tokio::test]
async fn events_stream_sync_token_emitted_when_trailing() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:sync-token>https://example.com/sync/7</D:sync-token>
</D:multistatus>"#;
    let base = serve_once(response_head("", xml.len()), xml.as_bytes().to_vec()).await;
    let client = WebDavClient::new(&base, None, None).unwrap();
    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();

    let stream = multistatus_events(resp.into_body(), &[]);
    futures::pin_mut!(stream);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    assert_eq!(events.len(), 2, "item + token expected: {events:?}");
    assert!(
        matches!(&events[0], DavStreamEvent::Item(i) if i.href == "/cal/a.ics"),
        "item must come first, got: {:?}",
        events[0]
    );
    assert!(
        matches!(&events[1], DavStreamEvent::SyncToken(t) if t == "https://example.com/sync/7"),
        "trailing sync token must be last, got: {:?}",
        events[1]
    );
}

// ---------------------------------------------------------------------------
// Client-level item streams (WebDavClient)
// ---------------------------------------------------------------------------

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:getetag/></D:prop></D:propfind>"#;

/// `propfind_items_stream` streams each `<D:response>` as soon as it is
/// complete, in document order, without aggregating the body.
#[tokio::test]
async fn propfind_items_stream_yields_items_in_order() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:response><D:href>/cal/b.ics</D:href><D:propstat><D:prop><D:getetag>"b"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
</D:multistatus>"#;
    let base = serve_once(response_head("", xml.len()), xml.as_bytes().to_vec()).await;
    let client = WebDavClient::builder(&base)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let stream = client
        .propfind_items_stream("/cal/", Depth::One, PROPFIND_BODY)
        .await
        .unwrap();
    futures::pin_mut!(stream);
    let mut hrefs = Vec::new();
    while let Some(event) = stream.next().await {
        match event.unwrap() {
            DavStreamEvent::Item(item) => hrefs.push(item.href),
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert_eq!(hrefs, ["/cal/a.ics", "/cal/b.ics"]);
}

/// `report_items_stream_with_timeout` streams REPORT results under a custom
/// idle timeout; a trailing sync token arrives as its own event.
#[tokio::test]
async fn report_items_stream_with_timeout_yields_items_and_trailing_token() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:sync-token>https://example.com/sync/7</D:sync-token>
</D:multistatus>"#;
    let base = serve_once(response_head("", xml.len()), xml.as_bytes().to_vec()).await;
    let client = WebDavClient::builder(&base)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let stream = client
        .report_items_stream_with_timeout(
            "/cal/",
            Depth::One,
            PROPFIND_BODY,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    futures::pin_mut!(stream);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    assert_eq!(events.len(), 2, "item + token expected: {events:?}");
    assert!(
        matches!(&events[0], DavStreamEvent::Item(i) if i.href == "/cal/a.ics"),
        "item must come first, got: {:?}",
        events[0]
    );
    assert!(
        matches!(&events[1], DavStreamEvent::SyncToken(t) if t == "https://example.com/sync/7"),
        "trailing sync token must be last, got: {:?}",
        events[1]
    );
}

/// A non-success status is rejected eagerly by the call itself — before any
/// stream item is produced — as [`Error::UnexpectedStatus`].
#[tokio::test]
async fn propfind_items_stream_rejects_error_status_eagerly() {
    let base = serve_once(
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 0\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;
    let client = WebDavClient::builder(&base)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let err = match client
        .propfind_items_stream("/cal/", Depth::One, PROPFIND_BODY)
        .await
    {
        Err(err) => err,
        Ok(_) => panic!("expected non-success status to be rejected eagerly"),
    };
    assert!(
        matches!(
            err,
            Error::UnexpectedStatus {
                operation: Operation::Propfind,
                ..
            }
        ),
        "expected UnexpectedStatus(Propfind), got: {err:?}"
    );
}

/// A gzip-encoded response is decompressed **on the fly**: the items stream
/// without ever aggregating the compressed or decompressed body.
#[tokio::test]
async fn propfind_items_stream_decodes_gzip_on_the_fly() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:response><D:href>/cal/b.ics</D:href><D:propstat><D:prop><D:getetag>"b"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
</D:multistatus>"#;
    let compressed = compress_payload(xml.as_bytes().to_vec().into(), ContentEncoding::Gzip)
        .await
        .unwrap();
    let base = serve_once(
        response_head("Content-Encoding: gzip\r\n", compressed.len()),
        compressed.to_vec(),
    )
    .await;
    let client = WebDavClient::builder(&base)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let stream = client
        .propfind_items_stream("/cal/", Depth::One, PROPFIND_BODY)
        .await
        .unwrap();
    futures::pin_mut!(stream);
    let mut hrefs = Vec::new();
    while let Some(event) = stream.next().await {
        match event.unwrap() {
            DavStreamEvent::Item(item) => hrefs.push(item.href),
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert_eq!(hrefs, ["/cal/a.ics", "/cal/b.ics"]);
}

/// A response that never completes (stalled connection) still yields the
/// items already parsed; dropping the stream aborts the download instead of
/// waiting for the rest of the body.
#[tokio::test]
async fn propfind_items_stream_drop_before_eof_aborts_download() {
    let partial = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:response><D:href>/cal/b.ics</D:href><D:propstat><D:prop><D:getetag>"b"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK"#;
    let head = format!(
        "HTTP/1.1 207 Multi-Status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        partial.len() + 4096
    );
    let base = serve_stalled(head, partial).await;
    let client = WebDavClient::builder(&base)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let stream = client
        .propfind_items_stream("/cal/", Depth::One, PROPFIND_BODY)
        .await
        .unwrap();
    futures::pin_mut!(stream);
    let first = stream.next().await.unwrap().unwrap();
    match first {
        DavStreamEvent::Item(item) => assert_eq!(item.href, "/cal/a.ics"),
        other => panic!("unexpected event: {other:?}"),
    }
    // No further polling: dropping the stream (scope end) aborts the
    // download instead of waiting for the remaining bytes — the test would
    // otherwise hang on the stalled connection.
}

/// A truncated body (connection closed before `Content-Length` bytes) yields
/// exactly one transport error and then ends the stream.
#[tokio::test]
async fn events_stream_yields_error_on_truncated_body() {
    let full = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
</D:multistatus>"#;
    let truncated = &full[..full.len() / 2];
    let base = serve_once(
        format!(
            "HTTP/1.1 207 Multi-Status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            full.len()
        ),
        truncated.to_vec(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();
    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();

    let stream = multistatus_events(resp.into_body(), &[]);
    futures::pin_mut!(stream);
    let err = stream.next().await.unwrap().unwrap_err();
    assert!(
        matches!(err, Error::Xml(_)),
        "expected a transport/XML error, got: {err:?}"
    );
    assert!(
        stream.next().await.is_none(),
        "stream must end after the error"
    );
}

/// An idle gap longer than the configured timeout yields a
/// [`Error::Timeout`] once and ends the stream.
#[tokio::test]
async fn propfind_items_stream_with_timeout_reports_idle_timeout() {
    let partial = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/cal/a.ics</D:href><D:propstat><D:prop><D:getetag>"a"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:response><D:href>/cal/b.ics</D:href><D:propstat><D:prop><D:getetag>"b"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK"#;
    let head = format!(
        "HTTP/1.1 207 Multi-Status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        partial.len() + 4096
    );
    let base = serve_stalled(head, partial).await;
    let client = WebDavClient::builder(&base)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let stream = client
        .propfind_items_stream_with_timeout(
            "/cal/",
            Depth::One,
            PROPFIND_BODY,
            Duration::from_millis(100),
        )
        .await
        .unwrap();
    futures::pin_mut!(stream);
    let first = stream.next().await.unwrap().unwrap();
    assert!(
        matches!(first, DavStreamEvent::Item(ref i) if i.href == "/cal/a.ics"),
        "first event must be the complete item, got: {first:?}"
    );
    let err = stream.next().await.unwrap().unwrap_err();
    assert!(
        matches!(err, Error::Timeout { .. }),
        "expected idle timeout, got: {err:?}"
    );
    assert!(
        stream.next().await.is_none(),
        "stream must end after the timeout"
    );
}
