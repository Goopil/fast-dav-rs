# Proposal: `ClientConfig` for `CalDavClient` / `CardDavClient` / `WebDavClient`

## Context / motivation

Today the client is built with a fixed, internal configuration (`common/http::build_hyper_client`)
and the constructors only take `base_url` + optional Basic credentials. Several things are hardcoded:

- `default_timeout = 20s` and a `5s` probe timeout (`webdav/client.rs`)
- no `User-Agent` header is ever sent
- HTTP is always `enable_http1() + enable_http2() + http2_adaptive_window(true)`
- `pool_max_idle_per_host(128)`, no idle timeout
- TLS trust is `with_native_roots()` (fallback `webpki_roots`), with no way to add extra roots
- no proxy support at all (neither env vars nor system proxy are honored by the plain `HttpConnector`)

The two practical needs on our side (Infomaniak Calendar, Android + Apple via a UniFFI/KMP bridge):

1. **General knobs** we'd like in prod: custom `User-Agent`, tunable timeouts / pool, and the option to
   force HTTP/1.1 for servers/proxies that misbehave with h2.
2. **Debug interception**: route traffic through a debugging proxy (Proxyman/Charles/mitmproxy) and trust
   its MITM CA. This is impossible today because (a) there's no proxy hook and (b) `with_native_roots()`
   on Android does **not** read user-installed CAs, only the system store — so we can't just install the
   proxy CA on the device.

Everything below is **fully opt-in via `Default`**: an empty `ClientConfig` must reproduce today's exact
behavior, so this is a non-breaking, additive change.

## Proposed public API

```rust
use std::time::Duration;

#[derive(Default, Clone)]
pub struct ClientConfig {
    // ---- General (prod-worthy) ----
    /// Value for the `User-Agent` header on every request. `None` = current behavior (none).
    pub user_agent: Option<String>,
    /// Per-request timeout. `None` keeps the current 20s default.
    pub default_timeout: Option<Duration>,
    /// TCP connect timeout applied to the connector. `None` = no explicit connect timeout.
    pub connect_timeout: Option<Duration>,
    /// Force HTTP/1.1 only (disable h2). `false` keeps current h1+h2 negotiation.
    pub force_http1: bool,
    /// Override `pool_max_idle_per_host`. `None` keeps current 128.
    pub pool_max_idle_per_host: Option<usize>,
    /// Idle connection timeout for the pool. `None` = current default (unbounded).
    pub pool_idle_timeout: Option<Duration>,

    // ---- Debug / interception ----
    /// Route all requests through this proxy, e.g. `http://127.0.0.1:9090`.
    /// HTTPS is tunneled via `CONNECT`.
    pub proxy: Option<http::Uri>,
    /// Optional Basic credentials for the proxy (username, password).
    pub proxy_auth: Option<(String, String)>,
    /// Additional trust anchors (PEM, possibly multiple concatenated certs per entry) added on top of
    /// the native/webpki roots — e.g. a Proxyman/Charles/mitmproxy root CA.
    pub extra_root_certs_pem: Vec<Vec<u8>>,
    /// DANGER: accept any server certificate. Testing/debug only — never enable in production.
    pub danger_accept_invalid_certs: bool,
}
```

### Constructors (additive, keep the existing ones)

```rust
impl CalDavClient {
    pub fn with_config(
        base_url: &str,
        basic_user: Option<&str>,
        basic_pass: Option<&str>,
        config: ClientConfig,
    ) -> anyhow::Result<Self> { /* ... */ }
}

// existing new(...) stays and simply delegates:
impl CalDavClient {
    pub fn new(base_url: &str, u: Option<&str>, p: Option<&str>) -> anyhow::Result<Self> {
        Self::with_config(base_url, u, p, ClientConfig::default())
    }
}
```

Same pattern for `CardDavClient` and `WebDavClient` (the config is threaded down into `WebDavClient`,
which owns the `HyperClient`, `default_timeout`, and would own the `user_agent`).

## Implementation sketch (`common/http.rs`)

The meaty part is the TLS config (native roots + extra roots / danger) and the optional proxy tunnel.

> **Type-alias note:** wrapping the connector in a `Tunnel` changes its type, which breaks the fixed
> `HyperClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>` alias. Two clean options:
> (a) box the inner HTTP connector so the alias stays a single type, or
> (b) a tiny `enum MaybeProxied<C>` connector that is either `Direct(C)` or `Tunnel<C>` and forwards
> `tower::Service`. Sketch below uses (a) via `hyper_util`'s boxed connector to keep one alias.

```rust
use std::time::Duration;
use anyhow::Result;
use bytes::Bytes;
use http::Uri;
use http_body_util::Full;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::rt::TokioExecutor;
use rustls::{ClientConfig as RustlsConfig, RootCertStore};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

use crate::ClientConfig; // the struct above

/// Build the rustls client config: native roots (fallback webpki) + any extra PEM roots,
/// or a no-op verifier when `danger_accept_invalid_certs` is set.
fn build_rustls_config(cfg: &ClientConfig) -> Result<RustlsConfig> {
    let mut roots = RootCertStore::empty();

    // native roots, fallback to webpki (mirrors today's with_native_roots behavior)
    match rustls_native_certs::load_native_certs() {
        result if !result.certs.is_empty() => {
            for c in result.certs { let _ = roots.add(c); }
        }
        _ => {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
    }

    // extra roots (e.g. Proxyman CA)
    for pem in &cfg.extra_root_certs_pem {
        for der in rustls_pemfile::certs(&mut pem.as_slice()) {
            if let Ok(der) = der { let _ = roots.add(der); }
        }
    }

    let mut tls = RustlsConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    if cfg.danger_accept_invalid_certs {
        tls.dangerous().set_certificate_verifier(std::sync::Arc::new(NoVerify));
    }
    Ok(tls)
}

#[derive(Debug)]
struct NoVerify;
impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self, _e: &CertificateDer, _i: &[CertificateDer], _s: &ServerName,
        _o: &[u8], _n: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, _m: &[u8], _c: &CertificateDer, _d: &rustls::DigitallySignedStruct)
        -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(&self, _m: &[u8], _c: &CertificateDer, _d: &rustls::DigitallySignedStruct)
        -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms
            .supported_schemes()
    }
}

pub type HyperClient = Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::BoxedConnector>,
    Full<Bytes>,
>;

pub fn build_hyper_client(cfg: &ClientConfig) -> Result<HyperClient> {
    // base TCP connector
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    if let Some(t) = cfg.connect_timeout { http.set_connect_timeout(Some(t)); }

    // optional proxy tunnel (CONNECT), boxed so the connector type stays stable
    let inner: BoxedConnector = match &cfg.proxy {
        Some(uri) => {
            let mut tunnel = Tunnel::new(uri.clone(), http);
            if let Some((u, p)) = &cfg.proxy_auth {
                let basic = base64::encode(format!("{u}:{p}"));
                tunnel = tunnel.with_auth(format!("Basic {basic}").parse()?);
            }
            box_connector(tunnel)
        }
        None => box_connector(http),
    };

    let tls = build_rustls_config(cfg)?;

    let mut https = HttpsConnectorBuilder::new()
        .with_tls_config(tls)
        .https_or_http()
        .enable_http1();
    let https = if cfg.force_http1 { https.build() } else { https.enable_http2().build() };

    let mut builder = Client::builder(TokioExecutor::new());
    if !cfg.force_http1 { builder.http2_adaptive_window(true); }
    builder.pool_max_idle_per_host(cfg.pool_max_idle_per_host.unwrap_or(128));
    if let Some(t) = cfg.pool_idle_timeout { builder.pool_idle_timeout(t); }

    Ok(builder.build::<_, Full<Bytes>>(https))
}
```

Notes for the maintainer:
- `user_agent` and `default_timeout` are applied in `WebDavClient` (header injection on each request /
  the existing `default_timeout` field), not in `build_hyper_client`.
- `BoxedConnector` / `box_connector` is shorthand for whatever boxing helper you prefer; the enum
  approach (option b) works equally well and avoids a boxed type.
- New deps needed: `rustls`, `rustls-pemfile`, `rustls-native-certs`, `webpki-roots`, `base64` (some are
  already transitive via `hyper-rustls`). Proxy tunneling needs `hyper-util` ≥ the version exposing
  `connect::proxy::Tunnel` (0.1.11+).

## Usage example (consumer side)

```rust
use fast_dav_rs::{CalDavClient, ClientConfig};

let cfg = ClientConfig {
    user_agent: Some("Infomaniak-Calendar-Android/1.2.3".into()),
    proxy: Some("http://127.0.0.1:9090".parse()?),        // Proxyman
    extra_root_certs_pem: vec![std::fs::read("/path/proxyman-ca.pem")?],
    ..Default::default()
};

let client = CalDavClient::with_config(
    "https://cal.example.com/dav/user/",
    Some("user"), Some("secret"),
    cfg,
)?;
```

With just `proxy` + `extra_root_certs_pem` (the Proxyman CA), interception works on **Android non-rooted
and iOS/macOS alike**, without touching the OS system trust store. `danger_accept_invalid_certs` is only
a convenience fallback for local testing.

## Summary of the ask

- Add `ClientConfig` (`Default` = current behavior) and `*_with_config` constructors on
  `CalDavClient` / `CardDavClient` / `WebDavClient`.
- Thread it into `build_hyper_client` (proxy tunnel + rustls roots/danger) and `WebDavClient`
  (user-agent + timeout).
- Non-breaking, additive; the general knobs are broadly useful, the proxy/CA knobs unblock
  debugging DAV traffic through an intercepting proxy.
