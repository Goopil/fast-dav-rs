use fast_dav_rs::RequestCompressionMode;
use fast_dav_rs::webdav::SyncCapability;
use fast_dav_rs::webdav::WebDavClient;

use crate::common::http_helpers::{response_head, serve_always, serve_once, unreachable_base};

const SYNC_SUPPORTED_MULTISTATUS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:supported-report-set>
          <D:supported-report>
            <D:report><D:sync-collection/></D:report>
          </D:supported-report>
        </D:supported-report-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

const PLAIN_MULTISTATUS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
  </D:response>
</D:multistatus>"#;

fn client_without_compression_probe(base: &str) -> WebDavClient {
    let client = WebDavClient::new(base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

#[tokio::test]
async fn sync_capability_supported_when_propfind_advertises_sync_collection() {
    let base = serve_once(
        response_head("", SYNC_SUPPORTED_MULTISTATUS.len()),
        SYNC_SUPPORTED_MULTISTATUS.as_bytes().to_vec(),
    )
    .await;
    let client = client_without_compression_probe(&base);

    assert_eq!(
        client.supports_webdav_sync().await.unwrap(),
        SyncCapability::Supported
    );
}

#[tokio::test]
async fn sync_capability_supported_via_report_fallback() {
    // The PROPFIND succeeds but does not advertise sync-collection, so the
    // client falls back to a sync-collection REPORT; a 207 answer proves
    // support. The same response is served to both sequential requests.
    let base = serve_always(
        format!(
            "HTTP/1.1 207 Multi-Status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            PLAIN_MULTISTATUS.len()
        ),
        PLAIN_MULTISTATUS.as_bytes().to_vec(),
    )
    .await;
    let client = client_without_compression_probe(&base);

    assert_eq!(
        client.supports_webdav_sync().await.unwrap(),
        SyncCapability::Supported
    );
}

#[tokio::test]
async fn sync_capability_unsupported_when_probe_rejected() {
    let base = serve_always(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
        Vec::new(),
    )
    .await;
    let client = client_without_compression_probe(&base);

    assert_eq!(
        client.supports_webdav_sync().await.unwrap(),
        SyncCapability::Unsupported
    );
}

#[tokio::test]
async fn sync_capability_unknown_when_server_unreachable() {
    let base = unreachable_base().await;
    let client = client_without_compression_probe(&base);

    assert_eq!(
        client.supports_webdav_sync().await.unwrap(),
        SyncCapability::Unknown
    );
}
