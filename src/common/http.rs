use anyhow::{Result, anyhow};
use bytes::Bytes;
use http_body_util::Full;
use hyper::Uri;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, RootCertStore};
use rustls_native_certs::load_native_certs;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tower_service::Service;

/// Type alias for the Hyper client used across CalDAV/CardDAV modules.
///
/// The `MaybeProxied` connector is `pub(crate)` since it is an implementation
/// detail of the connector layer; external code consumes it through this
/// alias and the `build_hyper_client*` constructors.
#[allow(private_interfaces)]
pub type HyperClient = Client<hyper_rustls::HttpsConnector<MaybeProxied>, Full<Bytes>>;

/// Default maximum number of idle pooled connections kept alive per host.
///
/// Lowered from the previous hardcoded `128`: typical CalDAV/CardDAV
/// deployments cap per-client connections well below that (often 10–50),
/// and keeping 128 idle sockets around needlessly exhausts client file
/// descriptors and server connection limits.
pub const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 32;

/// Connector that is either direct or proxied via HTTP CONNECT tunnel.
///
/// Implements `tower_service::Service<Uri>` by delegating to the inner
/// connector. The future is boxed since `HttpConnector` and
/// `Tunnel<HttpConnector>` produce different future types.
#[derive(Clone)]
pub(crate) enum MaybeProxied {
    Direct(HttpConnector),
    Tunneled(Tunnel<HttpConnector>),
}

impl Service<Uri> for MaybeProxied {
    type Response = <HttpConnector as Service<Uri>>::Response;
    type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            MaybeProxied::Direct(c) => c.poll_ready(cx).map_err(|e| Box::new(e) as Self::Error),
            MaybeProxied::Tunneled(c) => c.poll_ready(cx).map_err(|e| Box::new(e) as Self::Error),
        }
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        match self {
            MaybeProxied::Direct(c) => {
                let fut = c.call(dst);
                Box::pin(async move { fut.await.map_err(|e| Box::new(e) as Self::Error) })
            }
            MaybeProxied::Tunneled(c) => {
                let fut = c.call(dst);
                Box::pin(async move { fut.await.map_err(|e| Box::new(e) as Self::Error) })
            }
        }
    }
}

/// Build a Hyper client with the default idle-connection pool limit.
///
/// Thin wrapper around [`build_hyper_client_with_pool`].
#[allow(private_interfaces)]
pub fn build_hyper_client() -> Result<HyperClient> {
    build_hyper_client_with_pool(DEFAULT_POOL_MAX_IDLE_PER_HOST)
}

/// Build a Hyper client configured with HTTP/2, connection pooling capped at
/// `pool_max_idle_per_host` idle connections per host, and a TLS connector
/// that prefers native roots but falls back to the bundled WebPKI store.
#[allow(private_interfaces)]
pub fn build_hyper_client_with_pool(pool_max_idle_per_host: usize) -> Result<HyperClient> {
    let https_builder = HttpsConnectorBuilder::new()
        .with_native_roots()
        .unwrap_or_else(|err| {
            #[cfg(debug_assertions)]
            eprintln!(
                "fast-dav-rs: falling back to webpki roots (native roots unavailable: {err})"
            );
            HttpsConnectorBuilder::new().with_webpki_roots()
        });

    let http = HttpConnector::new();
    let inner = MaybeProxied::Direct(http);

    let https = https_builder
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .wrap_connector(inner);

    Ok(Client::builder(TokioExecutor::new())
        .http2_adaptive_window(true)
        .pool_max_idle_per_host(pool_max_idle_per_host)
        .build::<_, Full<Bytes>>(https))
}

/// A certificate verifier that accepts any server certificate.
///
/// # Warning
///
/// This completely disables TLS certificate verification. Only use in
/// testing/debug scenarios — never in production.
#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Build a rustls `ClientConfig` with native roots (fallback webpki),
/// optional extra PEM trust roots, and optional danger mode.
fn build_rustls_config(
    extra_root_certs_pem: &[Vec<u8>],
    danger_accept_invalid_certs: bool,
) -> Result<ClientConfig> {
    if danger_accept_invalid_certs {
        #[cfg(debug_assertions)]
        eprintln!(
            "fast-dav-rs: WARNING — danger_accept_invalid_certs is enabled, \
             TLS certificate verification is disabled"
        );

        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify))
            .with_no_client_auth();
        return Ok(config);
    }

    let mut roots = RootCertStore::empty();

    match load_native_certs() {
        result if !result.certs.is_empty() => {
            for cert in result.certs {
                let _ = roots.add(cert);
            }
        }
        result => {
            if !result.errors.is_empty() {
                #[cfg(debug_assertions)]
                eprintln!(
                    "fast-dav-rs: falling back to webpki roots (native roots errors: {:?})",
                    result.errors
                );
            }
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
    }

    for pem in extra_root_certs_pem {
        for cert in rustls_pemfile::certs(&mut pem.as_slice()) {
            let cert = cert.map_err(|e| anyhow!("failed to parse PEM certificate: {e}"))?;
            let _ = roots.add(cert);
        }
    }

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

/// Build a fully configured Hyper client.
///
/// This is the internal function called by `WebDavClientBuilder::build`.
/// It constructs the connector (with optional proxy tunnel), the TLS
/// config (with optional extra roots / danger mode), and the Hyper client
/// with pool settings.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_hyper_client_full(
    pool_max_idle_per_host: usize,
    pool_idle_timeout: Option<Duration>,
    force_http1: bool,
    connect_timeout: Option<Duration>,
    proxy: Option<Uri>,
    proxy_basic_user: Option<String>,
    proxy_basic_pass: Option<String>,
    extra_root_certs_pem: &[Vec<u8>],
    danger_accept_invalid_certs: bool,
) -> Result<HyperClient> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as B64;

    let mut http = HttpConnector::new();
    http.enforce_http(false);
    if let Some(t) = connect_timeout {
        http.set_connect_timeout(Some(t));
    }

    let inner = match proxy {
        Some(proxy_uri) => {
            let mut tunnel = Tunnel::new(proxy_uri, http);
            if let (Some(user), Some(pass)) = (proxy_basic_user, proxy_basic_pass) {
                let basic = B64.encode(format!("{user}:{pass}"));
                tunnel = tunnel.with_auth(
                    format!("Basic {basic}")
                        .parse()
                        .map_err(|e| anyhow!("invalid proxy auth header: {e}"))?,
                );
            }
            MaybeProxied::Tunneled(tunnel)
        }
        None => MaybeProxied::Direct(http),
    };

    let tls = build_rustls_config(extra_root_certs_pem, danger_accept_invalid_certs)?;

    let https_builder = HttpsConnectorBuilder::new()
        .with_tls_config(tls)
        .https_or_http()
        .enable_http1();

    let https = if force_http1 {
        https_builder.wrap_connector(inner)
    } else {
        https_builder.enable_http2().wrap_connector(inner)
    };

    let mut builder = Client::builder(TokioExecutor::new());
    if !force_http1 {
        builder.http2_adaptive_window(true);
    }
    builder.pool_max_idle_per_host(pool_max_idle_per_host);
    if let Some(t) = pool_idle_timeout {
        builder.pool_idle_timeout(t);
    }

    Ok(builder.build::<_, Full<Bytes>>(https))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pool_max_idle_is_32() {
        assert_eq!(DEFAULT_POOL_MAX_IDLE_PER_HOST, 32);
    }

    #[test]
    fn maybe_proxied_direct_construction() {
        let _ = MaybeProxied::Direct(HttpConnector::new());
    }
}
