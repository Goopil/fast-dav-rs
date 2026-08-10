//! Builder for [`CalDavClient`] — a thin wrapper over
//! [`WebDavClientBuilder`] with the same option set.

use std::time::Duration;

use hyper::Uri;

use crate::caldav::client::CalDavClient;
use crate::webdav::builder::WebDavClientBuilder;
use crate::webdav::client::RequestCompressionMode;

/// Builder for [`CalDavClient`].
///
/// Created with [`CalDavClient::builder`]. Delegates every option to
/// [`WebDavClientBuilder`]; only the base URL is required.
///
/// # Example
///
/// ```no_run
/// use fast_dav_rs::CalDavClient;
/// use fast_dav_rs::webdav::RequestCompressionMode;
/// use std::time::Duration;
///
/// let client = CalDavClient::builder("https://cal.example.com/dav/user01/")
///     .basic_auth("user01", "secret")
///     .timeout(Duration::from_secs(10))
///     .pool_max_idle_per_host(8)
///     .request_compression(RequestCompressionMode::Auto)
///     .build()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug)]
pub struct CalDavClientBuilder {
    inner: WebDavClientBuilder,
}

impl CalDavClientBuilder {
    pub(crate) fn new(base_url: impl Into<String>) -> Self {
        Self {
            inner: WebDavClientBuilder::new(base_url),
        }
    }

    /// Send **Basic** credentials with every request. Default: no auth.
    pub fn basic_auth(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.inner = self.inner.basic_auth(user, pass);
        self
    }

    /// Send a **Bearer** token with every request (OAuth 2.0). Default: no auth.
    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
        self.inner = self.inner.bearer_token(token);
        self
    }

    /// Set the default per-request timeout. Default: **20 seconds**.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Set the TCP connect timeout applied to the connector. Default: **none**.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.connect_timeout(timeout);
        self
    }

    /// Set the `User-Agent` header sent on every request. Default: **none**.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.inner = self.inner.user_agent(ua);
        self
    }

    /// Force HTTP/1.1 only (disable HTTP/2 negotiation). Default: **false**.
    pub fn force_http1(mut self, force: bool) -> Self {
        self.inner = self.inner.force_http1(force);
        self
    }

    /// Cap the number of idle pooled connections kept alive per host.
    /// Default: **32**.
    pub fn pool_max_idle_per_host(mut self, max_idle: usize) -> Self {
        self.inner = self.inner.pool_max_idle_per_host(max_idle);
        self
    }

    /// Set the idle connection timeout for the pool. Default: **unbounded**.
    pub fn pool_idle_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.pool_idle_timeout(timeout);
        self
    }

    /// Choose the request-body compression strategy. Default:
    /// [`RequestCompressionMode::Auto`].
    pub fn request_compression(mut self, mode: RequestCompressionMode) -> Self {
        self.inner = self.inner.request_compression(mode);
        self
    }

    /// Route all requests through this proxy (e.g. `http://127.0.0.1:9090`).
    /// HTTPS is tunneled via HTTP CONNECT. Default: **no proxy**.
    pub fn proxy(mut self, proxy: impl Into<Uri>) -> Self {
        self.inner = self.inner.proxy(proxy);
        self
    }

    /// Set Basic credentials for the proxy. Default: **no proxy auth**.
    pub fn proxy_basic_auth(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.inner = self.inner.proxy_basic_auth(user, pass);
        self
    }

    /// Add additional PEM-encoded trust roots on top of the native/webpki
    /// roots — e.g. a Proxyman/Charles/mitmproxy root CA. Default: **empty**.
    pub fn extra_root_certs_pem(mut self, certs: Vec<Vec<u8>>) -> Self {
        self.inner = self.inner.extra_root_certs_pem(certs);
        self
    }

    /// Accept any server certificate without verification. **Testing/debug
    /// only — never enable in production.** Default: **false**.
    pub fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.inner = self.inner.danger_accept_invalid_certs(accept);
        self
    }

    /// Validate the configuration and construct the [`CalDavClient`].
    ///
    /// # Errors
    ///
    /// Returns an error if the base URL is not a valid URI, if credentials
    /// are provided but cannot be encoded, if the proxy URI is invalid,
    /// if `timeout` or `pool_max_idle_per_host` is zero, or if PEM
    /// certificates cannot be parsed.
    pub fn build(self) -> anyhow::Result<CalDavClient> {
        Ok(CalDavClient::from_webdav(self.inner.build()?))
    }
}
