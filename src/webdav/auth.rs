//! Pluggable authentication: the [`TokenProvider`] trait and a generic
//! OAuth2 refresh-grant implementation ([`OAuth2RefreshProvider`], RFC 6749
//! §6).
//!
//! This is the **third** auth mode next to
//! [`basic_auth`](crate::WebDavClientBuilder::basic_auth) and
//! [`bearer_token`](crate::WebDavClientBuilder::bearer_token): the three are
//! mutually exclusive and the last one configured wins.
//!
//! The crate stays provider-agnostic and pure HTTP: browser/OIDC flows are
//! the caller's job. A [`TokenProvider`] hands the client the bearer token to
//! attach to the next request; [`OAuth2RefreshProvider`] is a provided impl
//! that obtains and renews tokens from an RFC 6749 token endpoint.
//!
//! # Security
//!
//! Tokens never appear in error messages, `Debug` output, or tracing events
//! from this module.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::header;
use hyper::{Method, Request, Uri};
use percent_encoding::utf8_percent_encode;
use zeroize::Zeroize;

use crate::common::http::HyperClient;
use crate::{Error, Result, TokenRefreshReason};

/// RFC 3986 unreserved characters: the safe set for
/// `application/x-www-form-urlencoded` token-request values (RFC 6749 §4.1.3).
const FORM_UNRESERVED: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Upper bound for a token-endpoint response body.
const MAX_TOKEN_RESPONSE_BYTES: usize = 1024 * 1024;

/// Supplies the bearer token attached to outgoing DAV requests.
///
/// This is the pluggable **third** auth mode next to
/// [`basic_auth`](crate::WebDavClientBuilder::basic_auth) and
/// [`bearer_token`](crate::WebDavClientBuilder::bearer_token); the three are
/// mutually exclusive and the last one set on the builder wins.
///
/// # Contract
///
/// - `token` is called **before each outgoing request** is built, and once
///   more when the DAV server rejects a provider-resolved token with `401`
///   (see below). Providers that fetch tokens over the network SHOULD cache;
///   the client does not.
/// - **401 renewal**: when a request carrying a provider-resolved token is
///   answered `401 Unauthorized`, the client calls
///   [`invalidate`](Self::invalidate) and retries the request **once** with a
///   token from a fresh `token` call. If the retry is also rejected, the `401`
///   response is returned as-is.
/// - The returned token must be usable as an HTTP header value (visible ASCII,
///   no controls): otherwise the request fails with
///   [`Error::InvalidInput`](crate::Error::InvalidInput).
/// - Implementations must never expose tokens through `Debug`, logs, or error
///   messages.
///
/// # Example
///
/// ```no_run
/// use std::future::Future;
/// use std::pin::Pin;
/// use std::sync::Arc;
///
/// use fast_dav_rs::webdav::{TokenProvider, WebDavClient};
/// use fast_dav_rs::Result;
///
/// /// Pulls the token from an ambient token cache owned by the application.
/// struct AmbientTokens;
///
/// impl TokenProvider for AmbientTokens {
///     fn token(&self) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
///         Box::pin(async {
///             // e.g. await the current token from your auth middleware here
///             Ok("token-from-ambient-cache".to_owned())
///         })
///     }
/// }
///
/// let client = WebDavClient::builder("https://dav.example.com/")
///     .token_provider(Arc::new(AmbientTokens))
///     .build()?;
/// # Ok::<(), fast_dav_rs::Error>(())
/// ```
pub trait TokenProvider: Send + Sync + 'static {
    /// Return the access token for the next outgoing request.
    ///
    /// Called before each request is built (and again for the single `401`
    /// retry — see the trait docs). The returned future borrows `self`, so
    /// providers can await shared state without cloning.
    ///
    /// # Errors
    ///
    /// A failing provider fails the whole DAV request with the returned error.
    fn token(&self) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>;

    /// Notify the provider that the most recently attached token was rejected
    /// with `401 Unauthorized`.
    ///
    /// Called by the client exactly once per request, before the single
    /// retry. The next [`token`](Self::token) call is expected to return a
    /// token usable despite the rejection (e.g. force a refresh). Default:
    /// no-op.
    fn invalidate(&self) {}
}

/// Configuration of [`OAuth2RefreshProvider`].
#[derive(Clone)]
struct RefreshConfig {
    token_endpoint: Uri,
    client_id: String,
    client_secret: String,
}

impl std::fmt::Debug for RefreshConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshConfig")
            .field("token_endpoint", &self.token_endpoint)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

impl Drop for RefreshConfig {
    fn drop(&mut self) {
        self.client_secret.zeroize();
    }
}

/// Cached access token with its expiry, plus the (possibly rotated) refresh
/// token. Secrets are zeroized on drop.
struct CachedToken {
    access_token: String,
    /// When the token expires; `None` when the server sent no `expires_in`
    /// (then only a `401` triggers renewal).
    expires_at: Option<Instant>,
    refresh_token: String,
}

impl Drop for CachedToken {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}

/// Shared state of [`OAuth2RefreshProvider`].
struct Inner {
    config: RefreshConfig,
    /// Guards the cached token **and** the refresh HTTP call: holding it
    /// across the POST makes renewal single-flight — concurrent callers
    /// serialize, and everyone after the first finds a fresh cache instead of
    /// stampeding the token endpoint.
    cached: tokio::sync::Mutex<CachedToken>,
    /// Set by [`TokenProvider::invalidate`]; the next `token()` call forces a
    /// refresh even if the cache has not expired yet.
    invalidated: AtomicBool,
    http: HyperClient,
    timeout: Duration,
}

/// A [`TokenProvider`] implementing the **OAuth2 refresh-token grant**
/// (RFC 6749 §6): it exchanges the configured refresh token for access tokens
/// at a token endpoint, transparently renewing on expiry or `401`.
///
/// The grant is POSTed as `application/x-www-form-urlencoded`
/// (`grant_type=refresh_token`, RFC 6749 §4.1.3/§6) with `client_id` and
/// `client_secret` as form parameters; the JSON response's `access_token`,
/// `expires_in` (if present) and `refresh_token` (rotation, if present) are
/// consumed per §5.1. No browser, no provider presets — pure HTTP.
///
/// # Renewal semantics
///
/// - Tokens are cached and reused until they expire (`expires_in`) or are
///   rejected: the DAV client retries a `401` **once** after forcing a
///   refresh, so a mid-flight expiry is absorbed invisibly.
/// - Refreshes are single-flight: concurrent requests share one in-flight
///   refresh instead of stampeding the token endpoint.
/// - With no `expires_in` from the server, only a `401` triggers renewal.
///
/// # Errors
///
/// Failures surface as [`Error::TokenRefresh`](crate::Error::TokenRefresh)
/// with a [`TokenRefreshReason`]: `Rejected` for non-success HTTP statuses,
/// `MalformedResponse` for unparsable §5.1 responses, `Transport` for
/// connection/timeout failures. Neither the response body nor any token is
/// included in the error (RFC 6749 §10.4).
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
///
/// use fast_dav_rs::webdav::{OAuth2RefreshProvider, WebDavClient};
///
/// # fn main() -> fast_dav_rs::Result<()> {
/// let provider = OAuth2RefreshProvider::new(
///     "https://auth.example.com/oauth2/token",
///     "my-client-id",
///     "my-client-secret",
///     "the-long-lived-refresh-token",
/// )?;
///
/// let client = WebDavClient::builder("https://dav.example.com/")
///     .token_provider(Arc::new(provider))
///     .build()?;
/// # Ok(())
/// # }
/// ```
pub struct OAuth2RefreshProvider {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for OAuth2RefreshProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuth2RefreshProvider")
            .field("config", &self.inner.config)
            .field("cached", &"<redacted>")
            .finish()
    }
}

impl OAuth2RefreshProvider {
    /// Create a refresh-token provider.
    ///
    /// The `token_endpoint` URL is validated eagerly. Credentials must be
    /// non-empty.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidUrl`](crate::Error::InvalidUrl) when
    /// `token_endpoint` is not a valid URI, [`Error::InvalidConfig`] when the
    /// token HTTP client cannot be constructed, and
    /// [`Error::InvalidInput`](crate::Error::InvalidInput) when any
    /// credential is empty.
    pub fn new(
        token_endpoint: impl AsRef<str>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Result<Self> {
        let token_endpoint = token_endpoint.as_ref();
        let endpoint: Uri = token_endpoint
            .parse()
            .map_err(|source| Error::invalid_url(token_endpoint, source))?;
        let client_id = client_id.into();
        let client_secret = client_secret.into();
        let refresh_token = refresh_token.into();
        if client_id.is_empty() || client_secret.is_empty() || refresh_token.is_empty() {
            return Err(Error::InvalidInput(
                "OAuth2RefreshProvider requires non-empty client_id, client_secret, \
                 and refresh_token"
                    .to_owned(),
            ));
        }
        Ok(Self {
            inner: Arc::new(Inner {
                config: RefreshConfig {
                    token_endpoint: endpoint,
                    client_id,
                    client_secret,
                },
                cached: tokio::sync::Mutex::new(CachedToken {
                    access_token: String::new(),
                    expires_at: None,
                    refresh_token,
                }),
                invalidated: AtomicBool::new(false),
                http: crate::webdav::builder::default_hyper_client()?,
                timeout: Duration::from_secs(10),
            }),
        })
    }

    /// Set the timeout for each token-endpoint call. Default: **10 seconds**.
    /// Applies before the provider is shared; a no-op on an already-cloned
    /// provider.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.timeout = timeout;
        }
        self
    }
}

impl Inner {
    /// POST the refresh grant (RFC 6749 §6) and parse the §5.1 response.
    async fn request_refresh(
        &self,
        refresh_token: &str,
    ) -> Result<(String, Option<u64>, Option<String>)> {
        let mut form = String::new();
        for (k, v) in [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
        ] {
            if !form.is_empty() {
                form.push('&');
            }
            form.push_str(&utf8_percent_encode(k, FORM_UNRESERVED).to_string());
            form.push('=');
            form.push_str(&utf8_percent_encode(v, FORM_UNRESERVED).to_string());
        }

        let transport =
            |source: Option<Box<dyn std::error::Error + Send + Sync>>| Error::TokenRefresh {
                reason: TokenRefreshReason::Transport,
                status: None,
                source,
            };
        let malformed = || Error::TokenRefresh {
            reason: TokenRefreshReason::MalformedResponse,
            status: None,
            source: None,
        };

        let req = Request::builder()
            .method(Method::POST)
            .uri(self.config.token_endpoint.clone())
            .header(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .header(header::CONTENT_LENGTH, form.len())
            .body(Full::new(Bytes::from(form)))
            .map_err(|e| transport(Some(Box::new(e))))?;

        let limit = self.timeout;
        let resp = match tokio::time::timeout(limit, self.http.request(req)).await {
            Ok(Ok(resp)) if resp.status().is_success() => resp,
            Ok(Ok(resp)) => {
                return Err(Error::TokenRefresh {
                    reason: TokenRefreshReason::Rejected,
                    status: Some(resp.status()),
                    source: None,
                });
            }
            Ok(Err(e)) => return Err(transport(Some(Box::new(e)))),
            Err(_) => return Err(transport(None)),
        };

        let body = match tokio::time::timeout(
            limit,
            Limited::new(resp.into_body(), MAX_TOKEN_RESPONSE_BYTES).collect(),
        )
        .await
        {
            Ok(Ok(collected)) => collected.to_bytes(),
            Ok(Err(_)) => return Err(malformed()),
            Err(_) => return Err(malformed()),
        };

        parse_token_response(&body)
    }
}

/// Parse an RFC 6749 §5.1 token response: `access_token` (required,
/// non-empty), optional `expires_in` (seconds), optional rotating
/// `refresh_token`. The body is never included in errors.
fn parse_token_response(body: &[u8]) -> Result<(String, Option<u64>, Option<String>)> {
    let malformed = || Error::TokenRefresh {
        reason: TokenRefreshReason::MalformedResponse,
        status: None,
        source: None,
    };
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| malformed())?;
    let access_token = value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or_else(malformed)?
        .to_owned();
    let expires_in = value.get("expires_in").and_then(serde_json::Value::as_u64);
    let refresh_token = value
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok((access_token, expires_in, refresh_token))
}

impl Inner {
    async fn token(&self) -> Result<String> {
        let mut cached = self.cached.lock().await;
        let invalidated = self.invalidated.swap(false, Ordering::Relaxed);
        let now = Instant::now();
        let usable = !invalidated && !cached.access_token.is_empty() && !cached.expired(now);
        if usable {
            return Ok(cached.access_token.clone());
        }

        let (access_token, expires_in, refresh_token) =
            self.request_refresh(&cached.refresh_token).await?;
        let expires_at = expires_in.map(|s| now + Duration::from_secs(s));
        // Rotate the refresh token when the server sent a new one; keep the
        // old otherwise. `CachedToken::drop` zeroizes replaced strings.
        if let Some(mut rotated) = refresh_token {
            if !rotated.is_empty() {
                cached.refresh_token = rotated;
            } else {
                rotated.zeroize();
            }
        }
        cached.access_token = access_token;
        cached.expires_at = expires_at;
        Ok(cached.access_token.clone())
    }
}

impl CachedToken {
    fn expired(&self, now: Instant) -> bool {
        self.expires_at.is_some_and(|at| at <= now)
    }
}

impl TokenProvider for OAuth2RefreshProvider {
    fn token(&self) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move { inner.token().await })
    }

    fn invalidate(&self) {
        self.inner.invalidated.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refresh_error(body: &[u8]) -> Error {
        parse_token_response(body).unwrap_err()
    }

    #[test]
    fn parse_token_response_full() {
        let (token, expires, refresh) = parse_token_response(
            br#"{"access_token":"abc","expires_in":3600,"refresh_token":"r2","token_type":"Bearer"}"#,
        )
        .unwrap();
        assert_eq!(token, "abc");
        assert_eq!(expires, Some(3600));
        assert_eq!(refresh.as_deref(), Some("r2"));
    }

    #[test]
    fn parse_token_response_without_optional_fields() {
        let (token, expires, refresh) =
            parse_token_response(br#"{"access_token":"abc","token_type":"Bearer"}"#).unwrap();
        assert_eq!(token, "abc");
        assert_eq!(expires, None);
        assert_eq!(refresh, None);
    }

    #[test]
    fn parse_token_response_missing_or_empty_access_token() {
        for body in [
            br#"{"token_type":"Bearer"}"#.as_slice(),
            br#"{"access_token":""}"#,
        ] {
            assert!(matches!(
                refresh_error(body),
                Error::TokenRefresh {
                    reason: TokenRefreshReason::MalformedResponse,
                    ..
                }
            ));
        }
        assert!(matches!(
            refresh_error(b"not json"),
            Error::TokenRefresh {
                reason: TokenRefreshReason::MalformedResponse,
                ..
            }
        ));
    }

    #[test]
    fn parse_token_response_errors_never_contain_body() {
        let err = refresh_error(br#"{"weird_field":"leaked-value"}"#);
        assert!(!err.to_string().contains("leaked-value"), "{err}");
    }

    #[test]
    fn form_encoding_escapes_reserved_characters() {
        let encoded = utf8_percent_encode("a&b=c d~e-f_g.h", FORM_UNRESERVED).to_string();
        assert_eq!(encoded, "a%26b%3Dc%20d~e-f_g.h");
    }

    #[test]
    fn debug_impls_redact_secrets() {
        let provider = OAuth2RefreshProvider::new(
            "https://auth.example.com/token",
            "cid",
            "super-secret",
            "refresh-secret",
        )
        .unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains("super-secret"), "{debug}");
        assert!(!debug.contains("refresh-secret"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }

    #[test]
    fn empty_credentials_rejected() {
        let err =
            OAuth2RefreshProvider::new("https://auth.example.com/token", "", "s", "r").unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err}");
    }
}
