use bytes::Bytes;
use fast_dav_rs::Error;
use fast_dav_rs::webdav::streaming::{
    parse_multistatus_bytes, parse_multistatus_stream_visit,
    parse_multistatus_stream_visit_with_timeout,
};
use fast_dav_rs::{ContentEncoding, Depth, RequestCompressionMode, WebDavClient, compress_payload};
use hyper::{HeaderMap, Method};
use std::sync::Arc;
use std::time::Duration;

use crate::common::http_helpers::{response_head, serve_once};

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

    let results = client
        .report_many(vec!["a".into()], Depth::One, body, 1)
        .await;
    assert!(results.iter().all(|b| b.result.is_err()));
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
