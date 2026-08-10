//! Builder for [`WebDavClient`] — configure auth, timeout, connection pool,
//! TLS, proxy, and request compression before the client is constructed.

use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use bytes::Bytes;
use http_body_util::Full;
use hyper::{Uri, header};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, RootCertStore};
use rustls_native_certs::load_native_certs;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroize;

use crate::common::http::{HyperClient, MaybeProxied};
use crate::webdav::client::{RequestCompressionMode, WebDavClient};

/// Default request timeout applied when [`WebDavClientBuilder::timeout`] is
/// not called (and by the `new()` convenience constructors).
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Default maximum number of idle pooled connections kept alive per host.
///
/// Lowered from the previous hardcoded `128`: typical CalDAV/CardDAV
/// deployments cap per-client connections well below that (often 10–50),
/// and keeping 128 idle sockets around needlessly exhausts client file
/// descriptors and server connection limits.
pub(crate) const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 32;

/// Builder for [`WebDavClient`].
///
/// Created with [`WebDavClient::builder`]. Every option is optional and
/// documented with its default; only the base URL is required.
///
/// # Example
///
/// ```no_run
/// use fast_dav_rs::webdav::{RequestCompressionMode, WebDavClient};
/// use std::time::Duration;
///
/// let client = WebDavClient::builder("https://dav.example.com/user01/")
///     .basic_auth("user01", "secret")
///     .timeout(Duration::from_secs(10))
///     .pool_max_idle_per_host(8)
///     .request_compression(RequestCompressionMode::Auto)
///     .build()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub struct WebDavClientBuilder {
    base_url: String,
    basic_user: Option<String>,
    basic_pass: Option<String>,
    bearer_token: Option<String>,
    timeout: Duration,
    connect_timeout: Option<Duration>,
    user_agent: Option<String>,
    force_http1: bool,
    pool_max_idle_per_host: usize,
    pool_idle_timeout: Option<Duration>,
    request_compression: RequestCompressionMode,
    proxy: Option<Uri>,
    proxy_basic_user: Option<String>,
    proxy_basic_pass: Option<String>,
    extra_root_certs_pem: Vec<Vec<u8>>,
    danger_accept_invalid_certs: bool,
}

/// Manual implementation so held Basic/Bearer credentials are never printed.
impl std::fmt::Debug for WebDavClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("WebDavClientBuilder");
        d.field("base_url", &self.base_url);
        if self.basic_user.is_some() {
            d.field("basic_auth", &"<redacted>");
        }
        if self.bearer_token.is_some() {
            d.field("bearer_token", &"<redacted>");
        }
        d.field("timeout", &self.timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("user_agent", &self.user_agent)
            .field("force_http1", &self.force_http1)
            .field("pool_max_idle_per_host", &self.pool_max_idle_per_host)
            .field("pool_idle_timeout", &self.pool_idle_timeout)
            .field("request_compression", &self.request_compression)
            .field("proxy", &self.proxy);
        if self.proxy_basic_user.is_some() {
            d.field("proxy_basic_auth", &"<redacted>");
        }
        d.field(
            "extra_root_certs_pem_count",
            &self.extra_root_certs_pem.len(),
        )
        .field(
            "danger_accept_invalid_certs",
            &self.danger_accept_invalid_certs,
        )
        .finish()
    }
}

impl WebDavClientBuilder {
    /// Start a builder for the given **base URL** (collection/home-set).
    pub(crate) fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            basic_user: None,
            basic_pass: None,
            bearer_token: None,
            timeout: DEFAULT_TIMEOUT,
            connect_timeout: None,
            user_agent: None,
            force_http1: false,
            pool_max_idle_per_host: DEFAULT_POOL_MAX_IDLE_PER_HOST,
            pool_idle_timeout: None,
            request_compression: RequestCompressionMode::Auto,
            proxy: None,
            proxy_basic_user: None,
            proxy_basic_pass: None,
            extra_root_certs_pem: Vec::new(),
            danger_accept_invalid_certs: false,
        }
    }

    /// Send **Basic** credentials with every request. Default: no auth.
    ///
    /// Calling this after [`bearer_token`](Self::bearer_token) clears the
    /// bearer token — the last auth method called wins.
    ///
    /// # Security
    ///
    /// Basic credentials are sent as an `Authorization: Basic` header on
    /// **every** request. Base64 is an encoding, not encryption: over plain
    /// `http://` the credentials travel effectively in cleartext and can be
    /// read by anyone on the network path. Always use `https://` outside
    /// isolated test environments (e.g. a local Docker test server).
    pub fn basic_auth(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.basic_user = Some(user.into());
        self.basic_pass = Some(pass.into());
        self.bearer_token = None;
        self
    }

    /// Send a **Bearer** token with every request (OAuth 2.0). Default: no auth.
    ///
    /// Calling this after [`basic_auth`](Self::basic_auth) clears the Basic
    /// credentials — the last auth method called wins.
    ///
    /// # Security
    ///
    /// The bearer token is sent as an `Authorization: Bearer` header on
    /// **every** request. Over plain `http://` the token travels in
    /// cleartext. Always use `https://` outside isolated test environments.
    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self.basic_user = None;
        self.basic_pass = None;
        self
    }

    /// Set the default per-request timeout. Default: **20 seconds**.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the TCP connect timeout applied to the connector. Default: **none**.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Set the `User-Agent` header sent on every request. Default: **none**.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Force HTTP/1.1 only (disable HTTP/2 negotiation). Default: **false**.
    ///
    /// Useful for servers or proxies that misbehave with HTTP/2.
    pub fn force_http1(mut self, force: bool) -> Self {
        self.force_http1 = force;
        self
    }

    /// Cap the number of idle pooled connections kept alive per host.
    /// Default: **32**.
    pub fn pool_max_idle_per_host(mut self, max_idle: usize) -> Self {
        self.pool_max_idle_per_host = max_idle;
        self
    }

    /// Set the idle connection timeout for the pool. Default: **unbounded**.
    pub fn pool_idle_timeout(mut self, timeout: Duration) -> Self {
        self.pool_idle_timeout = Some(timeout);
        self
    }

    /// Choose the request-body compression strategy. Default:
    /// [`RequestCompressionMode::Auto`].
    pub fn request_compression(mut self, mode: RequestCompressionMode) -> Self {
        self.request_compression = mode;
        self
    }

    /// Route all requests through this proxy (e.g. `http://127.0.0.1:9090`).
    /// HTTPS is tunneled via HTTP CONNECT. Default: **no proxy**.
    pub fn proxy(mut self, proxy: impl Into<Uri>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    /// Set Basic credentials for the proxy. Default: **no proxy auth**.
    pub fn proxy_basic_auth(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.proxy_basic_user = Some(user.into());
        self.proxy_basic_pass = Some(pass.into());
        self
    }

    /// Add additional PEM-encoded trust roots on top of the native/webpki
    /// roots — e.g. a Proxyman/Charles/mitmproxy root CA. Default: **empty**.
    pub fn extra_root_certs_pem(mut self, certs: Vec<Vec<u8>>) -> Self {
        self.extra_root_certs_pem = certs;
        self
    }

    /// # Danger
    ///
    /// Accept any server certificate without verification. **Testing/debug
    /// only — never enable in production.** Default: **false**.
    ///
    /// This bypasses all TLS certificate validation. Any HTTPS connection
    /// will succeed regardless of the server's certificate, including
    /// self-signed, expired, or mismatched certificates. Use
    /// [`extra_root_certs_pem`](Self::extra_root_certs_pem) instead when
    /// you need to trust a specific custom CA.
    pub fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.danger_accept_invalid_certs = accept;
        self
    }

    /// Validate the configuration and construct the [`WebDavClient`].
    ///
    /// # Errors
    ///
    /// Returns an error if the base URL is not a valid URI, if credentials
    /// are provided but cannot be encoded, if the proxy URI is invalid,
    /// if `timeout` or `pool_max_idle_per_host` is zero, or if PEM
    /// certificates cannot be parsed.
    pub fn build(mut self) -> Result<WebDavClient> {
        if self.timeout.is_zero() {
            return Err(anyhow!("timeout must be > 0"));
        }
        if self.pool_max_idle_per_host == 0 {
            return Err(anyhow!("pool_max_idle_per_host must be > 0"));
        }
        if let Some(token) = &self.bearer_token
            && token.is_empty()
        {
            return Err(anyhow!("bearer_token must not be empty"));
        }
        if let Some(token) = &self.bearer_token
            && !token.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(b, b'-' | b'.' | b'_' | b'~' | b'+' | b'/' | b'=')
            })
        {
            return Err(anyhow!(
                "bearer_token contains invalid characters (allowed: A-Z a-z 0-9 - . _ ~ + / =)"
            ));
        }
        if let (Some(user), Some(pass)) = (&self.basic_user, &self.basic_pass)
            && (user.is_empty() || pass.is_empty())
        {
            return Err(anyhow!(
                "basic_auth requires both user and pass to be non-empty"
            ));
        }
        if self.proxy.is_none()
            && (self.proxy_basic_user.is_some() || self.proxy_basic_pass.is_some())
        {
            return Err(anyhow!(
                "proxy_basic_auth requires a proxy to be set via .proxy()"
            ));
        }
        if let (Some(user), Some(pass)) = (&self.proxy_basic_user, &self.proxy_basic_pass)
            && (user.is_empty() || pass.is_empty())
        {
            return Err(anyhow!(
                "proxy_basic_auth requires both user and pass to be non-empty"
            ));
        }

        let base: Uri = self
            .base_url
            .parse()
            .map_err(|e: hyper::http::uri::InvalidUri| {
                anyhow!("invalid URL {:?}: {e}", self.base_url)
            })?;

        let auth_header = build_auth_header(
            self.basic_user.take(),
            self.basic_pass.take(),
            self.bearer_token.take(),
        )?;

        let user_agent = match self.user_agent.take() {
            Some(ua) => Some(
                header::HeaderValue::from_str(&ua)
                    .map_err(|e| anyhow!("invalid User-Agent header value: {e}"))?,
            ),
            None => None,
        };

        let hyper_client = build_hyper_client(
            self.pool_max_idle_per_host,
            self.pool_idle_timeout,
            self.force_http1,
            self.connect_timeout,
            self.proxy.take(),
            self.proxy_basic_user.take(),
            self.proxy_basic_pass.take(),
            &self.extra_root_certs_pem,
            self.danger_accept_invalid_certs,
        )?;

        Ok(WebDavClient::from_parts(
            base,
            hyper_client,
            auth_header,
            user_agent,
            self.timeout,
            self.request_compression,
        ))
    }
}

impl Drop for WebDavClientBuilder {
    fn drop(&mut self) {
        self.basic_user.zeroize();
        self.basic_pass.zeroize();
        self.bearer_token.zeroize();
        self.proxy_basic_user.zeroize();
        self.proxy_basic_pass.zeroize();
    }
}

/// Build the `Authorization` header from whichever auth method was set.
/// Zeroizes intermediate credential strings.
fn build_auth_header(
    basic_user: Option<String>,
    basic_pass: Option<String>,
    bearer_token: Option<String>,
) -> Result<Option<header::HeaderValue>> {
    if let Some(mut token) = bearer_token {
        let header_value = header::HeaderValue::from_str(&format!("Bearer {token}"));
        token.zeroize();
        return Ok(Some(header_value?));
    }
    if let (Some(mut user), Some(mut pass)) = (basic_user, basic_pass) {
        let header_value = build_basic_auth_header(&user, &pass);
        user.zeroize();
        pass.zeroize();
        return Ok(Some(header_value?));
    }
    Ok(None)
}

/// Build the `Authorization: Basic …` header value, zeroizing the
/// intermediate strings so plaintext credentials do not linger in freed
/// heap memory.
fn build_basic_auth_header(user: &str, pass: &str) -> Result<header::HeaderValue> {
    let mut token = format!("{}:{}", user, pass);
    let mut val = format!("Basic {}", B64.encode(&token));
    let header_value = header::HeaderValue::from_str(&val);
    token.zeroize();
    val.zeroize();
    Ok(header_value?)
}

// ---------------------------------------------------------------------------
// TLS configuration
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Hyper client construction
// ---------------------------------------------------------------------------

/// Build a fully configured Hyper client.
///
/// Constructs the connector (with optional proxy tunnel), the TLS config
/// (with optional extra roots / danger mode), and the Hyper client with
/// pool settings. Called by [`WebDavClientBuilder::build`].
#[allow(clippy::too_many_arguments)]
fn build_hyper_client(
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
    use crate::common::compression::ContentEncoding;

    const BASE: &str = "https://dav.example.com/user01/";

    #[test]
    fn defaults_match_documented_values() {
        let builder = WebDavClient::builder(BASE);
        assert_eq!(builder.timeout, DEFAULT_TIMEOUT);
        assert_eq!(builder.timeout, Duration::from_secs(20));
        assert_eq!(builder.pool_max_idle_per_host, 32);
        assert_eq!(builder.request_compression, RequestCompressionMode::Auto);
        assert!(builder.basic_user.is_none());
        assert!(builder.basic_pass.is_none());
        assert!(builder.bearer_token.is_none());
        assert!(!builder.force_http1);
        assert!(!builder.danger_accept_invalid_certs);
    }

    #[test]
    fn basic_auth_header_correct() {
        let client = WebDavClient::builder(BASE)
            .basic_auth("user", "pass")
            .build()
            .unwrap();
        let header = client.basic_auth_header().expect("auth header present");
        assert_eq!(header.to_str().unwrap(), "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn bearer_auth_header_correct() {
        let client = WebDavClient::builder(BASE)
            .bearer_token("ya29.token")
            .build()
            .unwrap();
        let header = client.basic_auth_header().expect("auth header present");
        assert_eq!(header.to_str().unwrap(), "Bearer ya29.token");
    }

    #[test]
    fn auth_mutual_exclusivity_last_wins() {
        let client = WebDavClient::builder(BASE)
            .basic_auth("user", "pass")
            .bearer_token("token123")
            .build()
            .unwrap();
        let header = client.basic_auth_header().expect("auth header present");
        assert_eq!(header.to_str().unwrap(), "Bearer token123");
    }

    #[test]
    fn no_auth_means_no_header() {
        let client = WebDavClient::builder(BASE).build().unwrap();
        assert!(client.basic_auth_header().is_none());
    }

    #[test]
    fn debug_redacts_credentials() {
        let builder = WebDavClient::builder(BASE).basic_auth("user", "hunter2");
        let debug = format!("{builder:?}");
        assert!(debug.contains("<redacted>"), "debug output: {debug}");
        assert!(!debug.contains("hunter2"), "debug output: {debug}");
    }

    #[test]
    fn invalid_url_errors() {
        let result = WebDavClient::builder("not a valid url").build();
        assert!(result.is_err());
    }

    #[test]
    fn clone_shares_compression_mode() {
        let client_a = WebDavClient::builder(BASE)
            .request_compression(RequestCompressionMode::Force(ContentEncoding::Zstd))
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
    fn timeout_zero_errors() {
        let result = WebDavClient::builder(BASE).timeout(Duration::ZERO).build();
        assert!(result.is_err());
    }

    #[test]
    fn pool_zero_errors() {
        let result = WebDavClient::builder(BASE)
            .pool_max_idle_per_host(0)
            .build();
        assert!(result.is_err());
    }
}
