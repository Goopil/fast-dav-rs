use fast_dav_rs::Depth;
use fast_dav_rs::common::http::MaybeProxied;
use fast_dav_rs::webdav::{HyperClient, WebDavClient};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;

/// AUDIT-017 injection fidelity smoke: a caller-built Hyper stack (here a
/// deliberately http1-only connector wrapped in `MaybeProxied::direct`) is
/// injected via `with_hyper_client` and drives a real PROPFIND against the
/// live server — proving the injected-transport path works end-to-end on a
/// real server, not just in wire-level unit mocks.
#[tokio::test]
async fn test_injected_hyper_client_http1_propfind() {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    // http1-only: no `.enable_http2()` — the caller owns the transport.
    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .wrap_connector(MaybeProxied::direct(http));
    let hyper_client: HyperClient = Client::builder(TokioExecutor::new()).build(connector);

    let client = WebDavClient::builder(SABREDAV_URL)
        .with_hyper_client(hyper_client)
        .basic_auth(TEST_USER, TEST_PASS)
        .build()
        .expect("Failed to build client with injected hyper stack");

    let resp = client
        .propfind("principals/test/", Depth::Zero, PROPFIND_BODY)
        .await
        .expect("PROPFIND must succeed through the injected stack");
    assert!(
        resp.status().is_success(),
        "Expected successful PROPFIND through the injected http1 stack, got {}",
        resp.status()
    );
    let body = resp.into_body();
    assert!(
        !body.is_empty(),
        "Expected a non-empty multistatus body through the injected stack"
    );
    assert!(
        String::from_utf8_lossy(&body).contains("principals/test"),
        "Expected the principal href in the multistatus body, got: {:?}",
        String::from_utf8_lossy(&body)
    );
}
