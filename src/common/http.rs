use anyhow::Result;
use bytes::Bytes;
use http_body_util::Full;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;

/// Type alias for the Hyper client used across CalDAV/CardDAV modules.
pub type HyperClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;

/// Build a Hyper client configured with HTTP/2, connection pooling, and a TLS connector
/// that prefers native roots but falls back to the bundled WebPKI store.
pub fn build_hyper_client() -> Result<HyperClient> {
    let https_builder = HttpsConnectorBuilder::new()
        .with_native_roots()
        .unwrap_or_else(|err| {
            #[cfg(debug_assertions)]
            eprintln!(
                "fast-dav-rs: falling back to webpki roots (native roots unavailable: {err})"
            );
            HttpsConnectorBuilder::new().with_webpki_roots()
        });

    let https = https_builder
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    Ok(Client::builder(TokioExecutor::new())
        .http2_adaptive_window(true)
        .pool_max_idle_per_host(128)
        .build::<_, Full<Bytes>>(https))
}
