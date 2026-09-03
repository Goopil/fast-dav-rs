use bytes::Bytes;
use futures::{StreamExt, stream::FuturesOrdered};
use http_body_util::Full;
use hyper::body::{Body as _, Incoming};
use hyper::{HeaderMap, Method, Request, Response, StatusCode, Uri, header};
use parking_lot::RwLock;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::time::{Duration, sleep, timeout};
use zeroize::Zeroize;

use crate::common::compression::{
    ContentEncoding, add_accept_encoding, add_content_encoding, compress_payload, decompress_body,
    detect_encodings, detect_request_compression_preference,
};
use crate::common::http::HyperClient;
use crate::common::{dav_debug, dav_trace};
// Only referenced inside `dav_debug!` arguments, which compile out without
// the `tracing` feature.
#[cfg(feature = "tracing")]
use crate::common::redact_userinfo;
use crate::error::EtagReason;
use crate::webdav::auth::TokenProvider;
use crate::webdav::builder::WebDavClientBuilder;
use crate::webdav::retry::{is_idempotent_method, is_retryable_status, retry_delay};
use crate::webdav::types::{
    BatchItem, DavCapabilities, DavItem, Depth, LockInfo, LockScope, Prefer, SyncCapability,
    SyncLevel,
};
use crate::{Error, Operation, Result};

/// Strategy for compressing outgoing request bodies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequestCompressionMode {
    /// Negotiate automatically: attempt gzip on first use and cache only what
    /// the probe proved — gzip when the server's advertised `Accept-Encoding`
    /// preference names it, `Identity` otherwise (including when the probe
    /// meets a redirect); fall back to identity on 415/501. A transient probe
    /// failure (transport error, timeout, other non-success status) is not
    /// cached — the next body-carrying request re-probes, while the current
    /// request proceeds uncompressed.
    #[default]
    Auto,
    Disabled,
    Force(ContentEncoding),
}

impl RequestCompressionMode {
    fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

const AUTO_DEFAULT_ENCODING: ContentEncoding = ContentEncoding::Gzip;
/// Canonical bootstrap PROPFIND body: requests `DAV:current-user-principal`
/// (RFC 6764 §6 step 5) — shared by the compression probe and the
/// `.well-known` service discovery.
pub(crate) const PROBE_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:current-user-principal/>
  </D:prop>
</D:propfind>"#;

/// Build the `If-Match` header value for a conditional request.
///
/// Accepts `*`, quoted/bare strong entity-tags (bare values are quoted), and
/// rejects empty and malformed tags. Weak entity-tags (`W/"abc"`) are
/// **rejected**: RFC 9110 §13.1.1 mandates strong comparison for `If-Match`,
/// so a weak validator would never match and the operation would be a
/// guaranteed `412` on a compliant server. Weak tags remain accepted on the
/// informational paths ([`normalize_etag`], [`etag_from_headers`]).
pub(crate) fn if_match_header_value(etag: &str) -> Result<header::HeaderValue> {
    let etag = etag.trim();
    if etag.is_empty() {
        return Err(Error::InvalidEtag {
            reason: EtagReason::Empty,
            source: None,
        });
    }

    if etag == "*" || is_valid_entity_tag(etag) {
        if etag.starts_with("W/") {
            // RFC 9110 §13.1.1: `If-Match` uses strong comparison; a weak
            // validator never matches, so the write would be a guaranteed 412.
            return Err(Error::InvalidEtag {
                reason: EtagReason::Weak,
                source: None,
            });
        }
        return header::HeaderValue::from_str(etag).map_err(|err| Error::InvalidEtag {
            reason: EtagReason::InvalidHeaderValue,
            source: Some(Box::new(err)),
        });
    }

    if let Some(opaque) = etag.strip_prefix("W/") {
        validate_opaque_tag(opaque)?;
        return Err(Error::InvalidEtag {
            reason: EtagReason::Weak,
            source: None,
        });
    }

    validate_opaque_tag(etag)?;
    let value = format!("\"{etag}\"");
    header::HeaderValue::from_str(&value).map_err(|err| Error::InvalidEtag {
        reason: EtagReason::InvalidHeaderValue,
        source: Some(Box::new(err)),
    })
}

fn validate_opaque_tag(opaque: &str) -> Result<()> {
    if opaque.is_empty() || opaque.contains('"') {
        return Err(Error::InvalidEtag {
            reason: EtagReason::InvalidFormat,
            source: None,
        });
    }
    if !opaque.bytes().all(is_etag_character) {
        return Err(Error::InvalidEtag {
            reason: EtagReason::InvalidCharacters,
            source: None,
        });
    }
    Ok(())
}

fn is_valid_entity_tag(etag: &str) -> bool {
    let opaque_tag = etag.strip_prefix("W/").unwrap_or(etag);
    opaque_tag
        .strip_prefix('"')
        .and_then(|tag| tag.strip_suffix('"'))
        .is_some_and(|tag| tag.bytes().all(is_etag_character))
}

fn is_etag_character(byte: u8) -> bool {
    byte == b'!' || (b'#'..=b'~').contains(&byte) || byte >= 0x80
}

pub fn normalize_etag(etag: &str) -> String {
    let etag = etag.trim();
    if etag.is_empty() {
        return String::new();
    }
    let (prefix, rest) = if let Some(s) = etag.strip_prefix("W/") {
        ("W/", s)
    } else {
        ("", etag)
    };
    let rest = rest.trim_matches('"');
    format!("{prefix}{rest}")
}

pub fn normalize_sync_token(token: &str) -> String {
    token.trim().trim_matches('"').to_string()
}

/// Split a successful sync-collection response into its raw parts
/// (headers, multistatus items, sync token).
fn parse_sync_response(resp: Response<Bytes>) -> Result<(HeaderMap, Vec<DavItem>, Option<String>)> {
    let headers = resp.headers().clone();
    let body = resp.into_body();
    let parsed = crate::webdav::streaming::parse_multistatus_bytes(&body)?;
    Ok((headers, parsed.items, parsed.sync_token))
}

/// Extract the `ETag` from a response header map, if present.
///
/// The returned value is **normalized**: surrounding double quotes are
/// stripped, so `"abc"` becomes `abc` and `W/"abc"` becomes `W/abc`.
pub fn etag_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(normalize_etag)
        .filter(|s| !s.is_empty())
}

/// Extract the `Preference-Applied` response header (RFC 7240 §3) and map it
/// to a [`Prefer`] preference the client supports.
///
/// Parsing is **lenient**: an absent, malformed, or unrecognized value
/// yields `None` — never an error. Pair with
/// `put_if_match_prefer` to check whether the server actually honored
/// `Prefer: return=representation`; servers are free to ignore preferences.
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::{Prefer, preference_applied_from_headers};
/// use hyper::HeaderMap;
///
/// let mut headers = HeaderMap::new();
/// headers.insert("Preference-Applied", "return=representation".parse().unwrap());
/// assert_eq!(
///     preference_applied_from_headers(&headers),
///     Some(Prefer::Representation)
/// );
///
/// assert_eq!(preference_applied_from_headers(&HeaderMap::new()), None);
/// ```
pub fn preference_applied_from_headers(headers: &HeaderMap) -> Option<Prefer> {
    headers
        .get("Preference-Applied")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .and_then(|v| {
            if v.eq_ignore_ascii_case("return=minimal") {
                Some(Prefer::Minimal)
            } else if v.eq_ignore_ascii_case("return=representation") {
                Some(Prefer::Representation)
            } else {
                None
            }
        })
}

/// Bytes percent-encoded in each URI path segment. RFC 3986 reserves `/`
/// (segment separator, preserved by encoding per segment) and rejects
/// controls, space, and the "unsafe" punctuation; existing valid `%XX`
/// escapes are preserved before this set is applied.
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'\\')
    .add(b'^')
    .add(b'|')
    .add(b'[')
    .add(b']');

/// Percent-encode every path segment of `path` individually.
///
/// `/` separators are preserved, already-valid `%XX` sequences are kept
/// verbatim, bare `%` becomes `%25`, and everything in
/// [`PATH_SEGMENT_ENCODE_SET`] (plus non-ASCII bytes) is percent-encoded.
/// In particular `?` and `#` are encoded, so a resource name can never
/// leak into the query or fragment position.
///
/// # `%XX` escapes are kept verbatim — know what you pass
///
/// Already-valid escapes are **never decoded or re-encoded**: the caller is
/// expected to pass either raw text or a correctly pre-encoded path. A
/// literal `%41` in the input therefore still addresses `A` on the server —
/// this crate does not rewrite existing escapes, so passing pre-encoded
/// input addresses exactly the resource named by the encoded form.
///
/// ```
/// # use fast_dav_rs::webdav::client::encode_path_segments;
/// assert_eq!(encode_path_segments("a%41b.txt"), "a%41b.txt");
/// assert_eq!(encode_path_segments("report?q.ics"), "report%3Fq.ics");
/// ```
#[doc(hidden)]
pub fn encode_path_segments(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(pos) = rest.find('%') {
        let (head, tail) = rest.split_at(pos);
        out.push_str(&utf8_percent_encode(head, PATH_SEGMENT_ENCODE_SET).to_string());
        if tail.len() >= 3 && tail.as_bytes()[1..3].iter().all(u8::is_ascii_hexdigit) {
            out.push_str(&tail[..3]);
            rest = &tail[3..];
        } else {
            out.push_str("%25");
            rest = &tail[1..];
        }
    }
    out.push_str(&utf8_percent_encode(rest, PATH_SEGMENT_ENCODE_SET).to_string());
    out
}

/// True for the HTTP statuses whose `Location` must be followed
/// (301, 302, 303, 307, 308).
fn is_redirect_status(status: StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

/// Compare the origins (scheme + host + effective port) of two URIs.
///
/// The host comparison is ASCII-case-insensitive (RFC 3986 §3.2.2: host
/// names are case-insensitive; `Uri` hosts are already punycode/ASCII), so
/// a redirect that merely re-cases the host stays same-origin.
#[doc(hidden)]
pub fn same_origin(a: &Uri, b: &Uri) -> bool {
    let effective_port = |u: &Uri| {
        u.port_u16().unwrap_or(match u.scheme_str() {
            Some("https") => 443,
            Some("http") => 80,
            _ => 0,
        })
    };
    let host = |u: &Uri| u.host().map(str::to_ascii_lowercase);
    a.scheme_str() == b.scheme_str() && host(a) == host(b) && effective_port(a) == effective_port(b)
}

/// Resolve a `Location` header value against the URI that produced it.
///
/// Supports absolute URLs with case-insensitive schemes (RFC 3986 §3.1 —
/// `HTTPS://…` is absolute just like `https://…`), network-path references
/// (`//host/path`, RFC 3986 §4.2 — resolved against the current scheme),
/// root-relative paths, bare query references, and relative segment
/// references (merged against the current directory, RFC 3986 §5). The
/// merged path is normalized with the RFC 3986 §5.2.4 `remove_dot_segments`
/// algorithm, so the resolved URI never contains `.` or `..` segments
/// (`../caldav/` against `/.well-known/` yields `/caldav/`). Returns `None`
/// when the reference cannot be resolved.
#[doc(hidden)]
pub fn resolve_location(current: &Uri, location: &str) -> Option<Uri> {
    if let Some((scheme, rest)) = location.split_once(':') {
        if rest.starts_with("//") && is_uri_scheme(scheme) {
            // Absolute URL: the `Uri` parser canonicalizes `http`/`https`
            // schemes to lowercase, so case differences (RFC 3986 §3.1)
            // need no special handling here.
            return location.parse().ok();
        }
    }

    // Network-path reference (RFC 3986 §4.2): `//host/path` reuses the
    // current scheme.
    if location.starts_with("//") {
        let scheme = current.scheme_str()?;
        return format!("{scheme}:{location}").parse().ok();
    }

    let scheme = current.scheme_str()?;
    let authority = current.authority()?;

    let (path_q, _) = location.split_once('#').unwrap_or((location, ""));
    let (path, query) = match path_q.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path_q, None),
    };

    let merged = if path.is_empty() {
        current.path().to_owned()
    } else if path.starts_with('/') {
        path.to_owned()
    } else {
        let dir = current.path().rsplit_once('/').map_or("", |(d, _)| d);
        format!("{dir}/{path}")
    };

    // RFC 3986 §5.2.4: the resolution output must not contain dot-segments.
    let resolved = remove_dot_segments(&merged);

    let path_and_query = match query {
        Some(q) => format!("{resolved}?{q}"),
        None => resolved,
    };

    Uri::builder()
        .scheme(scheme)
        .authority(authority.clone())
        .path_and_query(path_and_query)
        .build()
        .ok()
}

/// True when `s` is a valid URI scheme (RFC 3986 §3.1): an ASCII letter
/// followed by ASCII letters, digits, `+`, `-`, or `.`.
fn is_uri_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// RFC 3986 §5.2.4 `remove_dot_segments`: collapse `.` and `..` segments.
///
/// Empty segments (`//` inside the path) are preserved verbatim — the
/// algorithm only removes dot-segments. A `..` climbing past the root is
/// ignored (the leading `/` of an absolute path is never consumed), and a
/// trailing `.`/`..` keeps the trailing slash (`/a/./` → `/a/`,
/// `/a/..` → `/`).
fn remove_dot_segments(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::with_capacity(8);
    let segs: Vec<&str> = path.split('/').collect();
    let last = segs.len() - 1;
    for (i, segment) in segs.iter().enumerate() {
        match *segment {
            "." => {
                if i == last {
                    segments.push("");
                }
            }
            ".." => {
                if segments.len() > usize::from(absolute) {
                    segments.pop();
                }
                if i == last {
                    segments.push("");
                }
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// True when following `next` would downgrade the connection from HTTPS to
/// plain HTTP. Such redirects are never followed (RFC 6764 §6 is
/// TLS-first); the redirect response is returned to the caller instead.
#[doc(hidden)]
pub fn is_https_to_http_downgrade(current: &Uri, next: &Uri) -> bool {
    next.scheme_str() == Some("http") && current.scheme_str() == Some("https")
}

fn normalize_decompressed_headers(
    headers: &mut HeaderMap,
    encodings: &[ContentEncoding],
    body_len: usize,
) {
    if encodings.is_empty() {
        return;
    }

    headers.remove(header::CONTENT_ENCODING);
    if let Ok(value) = header::HeaderValue::from_str(&body_len.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    } else {
        headers.remove(header::CONTENT_LENGTH);
    }
}

#[derive(Clone)]
pub struct WebDavClient {
    base: Uri,
    client: HyperClient,
    /// Pre-built `Authorization` header (Basic or Bearer) attached to every
    /// request, if credentials were provided.
    ///
    /// Residual limitation: the intermediate credential strings are zeroized
    /// in [`WebDavClientBuilder::build`], but this `HeaderValue` necessarily
    /// keeps a copy of the credentials in memory for the whole lifetime of
    /// the client (and its clones) and is **not** zeroized on drop. This is
    /// an accepted trade-off so the header can be attached cheaply to each
    /// request.
    auth_header: Option<header::HeaderValue>,
    /// Pluggable per-request auth: when set, the `Authorization: Bearer`
    /// header is resolved through [`TokenProvider::token`] before each
    /// request (third auth mode, mutually exclusive with `auth_header`).
    /// Clones share the provider, including its cache and single-flight
    /// refresh.
    token_provider: Option<Arc<dyn TokenProvider>>,
    default_timeout: Duration,
    request_compression_mode: Arc<RwLock<RequestCompressionMode>>,
    /// Pre-parsed `User-Agent` header injected on every request, if set.
    user_agent: Option<header::HeaderValue>,
    negotiated_request_compression: Arc<RwLock<Option<ContentEncoding>>>,
    request_compression_probe: Arc<Mutex<()>>,
    /// Whether HTTP redirects (301/302/303/307/308) are followed.
    follow_redirects: bool,
    /// Maximum number of redirects to follow before failing with
    /// [`Error::TooManyRedirects`](crate::Error::TooManyRedirects).
    max_redirects: u8,
    /// Client-wide `Prefer` header (RFC 7240) injected on every request
    /// unless the request already carries one, if set via the builder.
    prefer: Option<Prefer>,
    /// Maximum number of retries for transient failures (`429`, `503`,
    /// `504`); `0` disables retrying (each request is sent exactly once).
    max_retries: usize,
    /// When `false` (default), only idempotent methods are retried; when
    /// `true`, every method is retried on transient failures.
    retry_all: bool,
    /// Initial exponential-backoff delay (shrinkable via the test seam).
    retry_initial_backoff: Duration,
    /// Upper bound for the exponential-backoff delay (shrinkable via the
    /// test seam).
    retry_backoff_cap: Duration,
}

impl WebDavClient {
    /// Create a new client from a **base URL** (collection/home-set) and optional **Basic** credentials.
    ///
    /// The base may be `https://` **or** `http://` (both are supported by the connector).
    ///
    /// # Security
    ///
    /// Basic credentials are sent as an `Authorization: Basic` header on **every**
    /// request. Base64 is an encoding, not encryption: over plain `http://` the
    /// credentials travel effectively in cleartext and can be read by anyone on the
    /// network path. Always use `https://` outside isolated test environments
    /// (e.g. a local Docker test server).
    pub fn new(base_url: &str, basic_user: Option<&str>, basic_pass: Option<&str>) -> Result<Self> {
        let mut builder = Self::builder(base_url);
        if let (Some(u), Some(p)) = (basic_user, basic_pass) {
            builder = builder.basic_auth(u, p);
        }
        builder.build()
    }

    /// Create a builder for configuring the client before construction.
    ///
    /// Only the base URL is required; every other option has a sensible
    /// default documented on [`WebDavClientBuilder`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::webdav::WebDavClient;
    /// use std::time::Duration;
    ///
    /// let client = WebDavClient::builder("https://dav.example.com/")
    ///     .basic_auth("user", "pass")
    ///     .timeout(Duration::from_secs(10))
    ///     .build()?;
    /// # Ok::<(), fast_dav_rs::Error>(())
    /// ```
    pub fn builder(base_url: impl Into<String>) -> WebDavClientBuilder {
        WebDavClientBuilder::new(base_url)
    }

    /// Construct a client from pre-built parts.
    ///
    /// This is the internal constructor used by [`WebDavClient::new`] and
    /// [`WebDavClientBuilder::build`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        base: Uri,
        client: HyperClient,
        auth_header: Option<header::HeaderValue>,
        user_agent: Option<header::HeaderValue>,
        token_provider: Option<Arc<dyn TokenProvider>>,
        default_timeout: Duration,
        request_compression_mode: RequestCompressionMode,
        follow_redirects: bool,
        max_redirects: u8,
        prefer: Option<Prefer>,
        max_retries: usize,
        retry_all: bool,
        retry_initial_backoff: Duration,
        retry_backoff_cap: Duration,
    ) -> Self {
        Self {
            base,
            client,
            auth_header,
            token_provider,
            user_agent,
            default_timeout,
            request_compression_mode: Arc::new(RwLock::new(request_compression_mode)),
            negotiated_request_compression: Arc::new(RwLock::new(None)),
            request_compression_probe: Arc::new(Mutex::new(())),
            follow_redirects,
            max_redirects,
            prefer,
            max_retries,
            retry_all,
            retry_initial_backoff,
            retry_backoff_cap,
        }
    }

    /// Get the auth header value, if credentials were provided.
    #[cfg(test)]
    pub(crate) fn auth_header(&self) -> Option<&header::HeaderValue> {
        self.auth_header.as_ref()
    }

    /// Resolve the `Authorization` header for the next request attempt.
    ///
    /// The token provider wins when configured (third auth mode — the
    /// builder guarantees mutual exclusion with the static header).
    /// Provider errors propagate and fail the request.
    async fn resolve_auth_header(&self) -> Result<Option<header::HeaderValue>> {
        let Some(provider) = &self.token_provider else {
            return Ok(self.auth_header.clone());
        };
        let mut token = provider.token().await?;
        let value = header::HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
            Error::InvalidInput(
                "token provider returned a token that cannot be used as an HTTP \
                 header value (visible ASCII required)"
                    .to_owned(),
            )
        });
        token.zeroize();
        Ok(Some(value?))
    }

    /// Get the base URI this client was constructed with.
    pub(crate) fn base(&self) -> &Uri {
        &self.base
    }

    /// Configure the request compression strategy.
    ///
    /// Switching back to [`RequestCompressionMode::Auto`] clears the cached
    /// negotiation so the next body-carrying request re-probes the server;
    /// `Disabled` and `Force` pin their encoding immediately. This is also the
    /// manual recovery path after unusual server behavior.
    ///
    /// # Performance
    ///
    /// In `Auto` mode each client instance runs one extra compressed
    /// `PROPFIND` probe (until the server's answer is cached) before its first
    /// body-carrying request. Clones share the cache, but short-lived clients
    /// — e.g. one built per request in serverless setups — pay the probe every
    /// time; prefer reusing a client or pinning [`RequestCompressionMode::Disabled`]
    /// / [`RequestCompressionMode::Force`] to skip the probe.
    pub fn set_request_compression_mode(&self, mode: RequestCompressionMode) {
        *self.request_compression_mode.write() = mode;
        match mode {
            RequestCompressionMode::Auto => self.set_negotiated_encoding(None),
            RequestCompressionMode::Disabled => {
                self.set_negotiated_encoding(Some(ContentEncoding::Identity))
            }
            RequestCompressionMode::Force(enc) => self.set_negotiated_encoding(Some(enc)),
        }
    }

    /// Get the current request compression strategy.
    pub fn request_compression_mode(&self) -> RequestCompressionMode {
        *self.request_compression_mode.read()
    }

    /// Hidden test seam: shrink the exponential-backoff delays so unit tests
    /// do not sleep for real seconds. Not part of the stable API.
    #[doc(hidden)]
    pub fn set_retry_delays_for_testing(&mut self, initial: Duration, cap: Duration) {
        self.retry_initial_backoff = initial;
        self.retry_backoff_cap = cap;
    }

    /// Get the currently resolved request compression encoding.
    pub fn request_compression(&self) -> ContentEncoding {
        self.resolve_request_encoding()
    }

    /// Build the full request URI for `path` against the client's base URL.
    ///
    /// `path` may be empty, absolute (`/…`), or relative (merged into the
    /// base path's directory). The whole input is treated as a **path**:
    /// `?` and `#` inside a resource name are percent-encoded (`%3F` /
    /// `%23`) so they cannot change resource identity — a query string is
    /// not part of the path contract. Already-valid `%XX` escapes pass
    /// through verbatim (see [`encode_path_segments`]). An absolute URL
    /// (`http://`/`https://…`) is parsed as-is.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidUrl`](crate::Error::InvalidUrl) when the
    /// resulting URI is malformed.
    pub fn build_uri(&self, path: &str) -> Result<Uri> {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path
                .parse()
                .map_err(|source| Error::invalid_url(path, source));
        }

        let mut parts = self.base.clone().into_parts();
        let existing_path = parts
            .path_and_query
            .as_ref()
            .map(|pq| pq.path())
            .unwrap_or("/");

        let mut combined = if path.is_empty() {
            existing_path.to_string()
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            let mut base = existing_path.trim_end_matches('/').to_string();
            if base.is_empty() {
                base.push('/');
            }
            if !base.ends_with('/') {
                base.push('/');
            }
            base.push_str(path);
            base
        };

        if combined.is_empty() {
            combined.push('/');
        }

        // Percent-encode the path (per segment, preserving valid `%XX`).
        // `?` and `#` are encoded too, so a resource name can never be
        // mistaken for a query or fragment.
        let encoded = encode_path_segments(&combined);

        let path_and_query = encoded
            .parse::<hyper::http::uri::PathAndQuery>()
            .map_err(|source| Error::invalid_url(path, source))?;

        parts.path_and_query = Some(path_and_query);
        Uri::from_parts(parts).map_err(|source| Error::invalid_url(path, source))
    }

    fn resolve_request_encoding(&self) -> ContentEncoding {
        let mode = self.request_compression_mode.read();
        self.resolve_request_encoding_with_mode(&mode)
    }

    fn resolve_request_encoding_with_mode(&self, mode: &RequestCompressionMode) -> ContentEncoding {
        match *mode {
            RequestCompressionMode::Disabled => ContentEncoding::Identity,
            RequestCompressionMode::Force(enc) => enc,
            RequestCompressionMode::Auto => self
                .negotiated_request_compression
                .read()
                .unwrap_or(AUTO_DEFAULT_ENCODING),
        }
    }

    fn set_negotiated_encoding(&self, encoding: Option<ContentEncoding>) {
        *self.negotiated_request_compression.write() = encoding;
    }

    /// Probe whether the server accepts compressed request bodies.
    ///
    /// Sends a small gzip-compressed `PROPFIND` to the base URL. On success,
    /// the cached encoding keeps gzip — the only encoding the probe actually
    /// proved the server accepts — and only when the server's advertised
    /// `Accept-Encoding` preference names gzip; anything else (including
    /// `br`/`zstd` picks the header might suggest) is unproven and caches
    /// `Identity` instead, so later requests cannot fail with `415` on an
    /// encoding the server never agreed to.
    ///
    /// Returns `true` when a definitive answer was cached (including
    /// `Identity` when the server advertises no compression support, or
    /// answers the probe with a redirect — see below), and `false` when the
    /// probe failed: nothing is cached then, so the next body-carrying
    /// request re-probes. The caller sends the current request uncompressed
    /// in that case.
    ///
    /// The caller guarantees `Auto` mode and an uncached negotiation (checked
    /// under the probe lock) before invoking this.
    async fn probe_request_compression_support(&self) -> bool {
        let propfind = Method::from_bytes(b"PROPFIND").expect("static PROPFIND method is valid");

        let uri = self
            .build_uri("")
            .expect("base URL is validated at build time");

        let mut headers = HeaderMap::new();
        headers.insert("Depth", header::HeaderValue::from_static("0"));
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        // RFC 9110 §10.1.5: identify like every other request, so
        // User-Agent-aware servers (throttling, filtering) do not treat the
        // probe differently from the real pipeline.
        if let Some(ua) = &self.user_agent {
            headers.insert(header::USER_AGENT, ua.clone());
        }

        let mut req_builder = Request::builder().method(propfind).uri(uri);
        match self.resolve_auth_header().await {
            Ok(Some(auth)) => {
                req_builder = req_builder.header(header::AUTHORIZATION, auth);
            }
            Ok(None) => {}
            // A failing token provider is surfaced by the real request; the
            // probe just reports failure (no caching) as with any transport
            // error.
            Err(_) => return false,
        }

        add_accept_encoding(&mut headers);

        let probe_payload = Bytes::from_static(PROBE_BODY.as_bytes());
        let mut encoded_body = probe_payload.clone();
        if let Ok(compressed) = compress_payload(probe_payload.clone(), AUTO_DEFAULT_ENCODING).await
        {
            encoded_body = compressed;
            add_content_encoding(&mut headers, AUTO_DEFAULT_ENCODING);
        }

        for (k, v) in headers.iter() {
            req_builder = req_builder.header(k, v);
        }

        let req = req_builder
            .body(Full::new(encoded_body))
            .expect("valid static probe request parts");

        let fut = self.client.request(req);
        let result = timeout(self.default_timeout, fut).await;

        match result {
            Ok(Ok(resp)) if resp.status().is_success() => {
                // The gzip probe succeeded. Only cache what was proven:
                // keep gzip when the server advertises it as its preference,
                // `Identity` otherwise — an `Accept-Encoding` listing
                // `br`/`zstd` is not evidence the server accepts them for
                // request bodies (a later PUT could fail with `415`).
                let negotiated = match detect_request_compression_preference(resp.headers()) {
                    Some(ContentEncoding::Gzip) => ContentEncoding::Gzip,
                    _ => ContentEncoding::Identity,
                };
                self.set_negotiated_encoding(Some(negotiated));
                dav_debug!(
                    encoding = negotiated.as_str(),
                    "request compression probe succeeded"
                );
                true
            }
            // A 3xx is not transient: the probe deliberately bypasses the
            // redirect pipeline, and the base URL's redirect is a stable
            // property of the deployment — leaving the cache empty would
            // re-pay the same doomed probe before every body-carrying
            // request. Pin `Identity`, the same steady state requests reach
            // today (uncompressed), without the per-request tax.
            Ok(Ok(resp)) if is_redirect_status(resp.status()) => {
                self.set_negotiated_encoding(Some(ContentEncoding::Identity));
                dav_debug!(
                    status = %resp.status(),
                    "request compression probe hit a redirect; pinning identity"
                );
                true
            }
            // Transient failure: do not pin `Identity` — the next request
            // re-probes; the current one proceeds uncompressed.
            _ => {
                dav_debug!("request compression probe failed; re-probing on the next request");
                false
            }
        }
    }

    fn handle_request_compression_outcome(
        &self,
        attempted: Option<ContentEncoding>,
        status: StatusCode,
    ) -> bool {
        if !self.request_compression_mode.read().is_auto() {
            return false;
        }

        let Some(encoding) = attempted else {
            return false;
        };

        // Only a compression-specific rejection (`415`, or `501` for servers
        // that do not implement the request at all) may disable compression
        // for the client's lifetime: a `400` can come from an unrelated
        // malformed body and must not pin `Identity`.
        if matches!(
            status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE | StatusCode::NOT_IMPLEMENTED
        ) {
            self.set_negotiated_encoding(Some(ContentEncoding::Identity));
            dav_debug!(
                status = %status,
                "server rejected the compressed request body; pinning identity"
            );
            return true;
        }

        self.set_negotiated_encoding(Some(encoding));
        false
    }

    /// Prepare an outgoing body for sending.
    ///
    /// A caller-supplied `Content-Encoding` header means the body is already
    /// encoded: it is honored as-is — no automatic compression, no probe, and
    /// the header is forwarded untouched. Otherwise the negotiated request
    /// encoding is applied (probing first in `Auto` mode when nothing is
    /// cached yet).
    async fn prepare_request_body(
        &self,
        payload: Bytes,
        headers: &mut HeaderMap,
    ) -> (Bytes, Option<ContentEncoding>) {
        // Re-compressing an already-encoded body would silently corrupt the
        // payload (double encoding behind a 2xx), so a caller-supplied
        // `Content-Encoding` skips the automatic path entirely.
        if headers.contains_key(header::CONTENT_ENCODING) {
            return (payload, None);
        }

        let mode = *self.request_compression_mode.read();

        if mode.is_auto() {
            let negotiated = *self.negotiated_request_compression.read();
            if negotiated.is_none() {
                let _probe_guard = self.request_compression_probe.lock().await;
                let negotiated = *self.negotiated_request_compression.read();
                if negotiated.is_none() && !self.probe_request_compression_support().await {
                    // Probe failed: send this request uncompressed without
                    // caching anything, so the next request re-probes.
                    return (payload, None);
                }
            }
        }

        let encoding = self.resolve_request_encoding_with_mode(&mode);
        if encoding == ContentEncoding::Identity {
            return (payload, None);
        }

        match compress_payload(payload.clone(), encoding).await {
            Ok(compressed) => {
                add_content_encoding(headers, encoding);
                (compressed, Some(encoding))
            }
            Err(_) => (payload, None),
        }
    }

    // ----------- Aggregated send (Bytes) with automatic decompression -----------

    /// Shared request pipeline: builds the request (auth, UA, compression),
    /// sends it with the compression-retry loop, follows HTTP redirects,
    /// retries transient failures (`429`/`503`/`504`), and returns the
    /// **final request URI** (after redirect resolution) together with the
    /// raw (still-encoded) response.
    ///
    /// Redirect handling (301/302/303/307/308): the request is re-sent to the
    /// `Location` target up to `max_redirects` times. On 303 the method
    /// switches to `GET` and the body is dropped. When a hop crosses origins
    /// (scheme, host, or port change), `Authorization`, `Cookie`,
    /// `If-Match`, and `If-None-Match` headers are stripped for the
    /// remainder of the chain. An `https`→`http` downgrade is **never**
    /// followed (RFC 6764 §6 is TLS-first): the 3xx response is returned
    /// as-is so the caller can observe it, mirroring the
    /// unresolvable-`Location` behavior.
    ///
    /// Transient-failure retry (429/503/504): when `max_retries` > 0 and the
    /// method is retryable per the idempotency policy, the request is re-sent
    /// after a delay (`Retry-After` on 429, exponential backoff + jitter
    /// otherwise). The retry budget is shared across the whole redirect chain
    /// and counts every HTTP attempt; each attempt (retries included) runs
    /// under the same per-request timeout. When the budget is exhausted, the
    /// last response is returned as-is.
    ///
    /// Auth renewal (only when a [`TokenProvider`] is configured): a `401`
    /// response triggers exactly **one** refresh + resend per request —
    /// never a loop — and does not consume the transient-retry budget. The
    /// renewal is skipped once credentials were stripped for a cross-origin
    /// redirect. A still-unauthorized retry is returned as-is.
    pub(crate) async fn build_and_send(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body_bytes: Option<Bytes>,
        per_req_timeout: Option<Duration>,
    ) -> Result<(Uri, Response<Incoming>)> {
        let mut method = method;
        let mut body = body_bytes;
        let mut base_headers = headers;
        // RFC 7240: inject the client-wide preference unless the caller
        // already supplied a `Prefer` header for this request (per-request
        // headers win over the builder default).
        if let Some(prefer) = self.prefer {
            if !base_headers.contains_key("Prefer") {
                base_headers.insert("Prefer", header::HeaderValue::from_static(prefer.as_str()));
            }
        }
        let mut uri = self.build_uri(path)?;
        let mut redirects: u8 = 0;
        let mut strip_credentials = false;
        let mut attempt = 0;
        let mut retries: usize = 0;
        // Auth renewal on 401: at most one extra attempt per request (see
        // below), independent of the transient-retry budget.
        let mut auth_retried = false;

        loop {
            let mut headers = base_headers.clone();
            add_accept_encoding(&mut headers);

            let mut req_builder = Request::builder().method(method.clone()).uri(uri.clone());

            if !strip_credentials {
                let auth = self.resolve_auth_header().await?;
                if let Some(auth) = auth {
                    req_builder = req_builder.header(header::AUTHORIZATION, auth);
                }
            }

            if let Some(ua) = &self.user_agent {
                headers.insert(header::USER_AGENT, ua.clone());
            }

            let mut final_body: Option<Bytes> = None;
            let mut attempted_encoding: Option<ContentEncoding> = None;

            if let Some(body) = body.clone() {
                if !headers.contains_key(header::CONTENT_TYPE) {
                    req_builder = req_builder.header(
                        header::CONTENT_TYPE,
                        header::HeaderValue::from_static("application/xml; charset=utf-8"),
                    );
                }

                let (payload, encoding) = self.prepare_request_body(body, &mut headers).await;
                attempted_encoding = encoding;
                final_body = Some(payload);
            }

            for (k, v) in headers.iter() {
                req_builder = req_builder.header(k, v);
            }

            let req = match final_body {
                Some(b) => req_builder.body(Full::new(b))?,
                None => req_builder.body(Full::new(Bytes::new()))?,
            };

            let limit = per_req_timeout.unwrap_or(self.default_timeout);
            #[cfg(feature = "tracing")]
            let started = std::time::Instant::now();
            dav_debug!(
                method = %method,
                uri = %redact_userinfo(&uri),
                "dav request start"
            );
            let fut = self.client.request(req);
            let resp = timeout(limit, fut)
                .await
                .map_err(|_| {
                    dav_debug!(
                        method = %method,
                        uri = %redact_userinfo(&uri),
                        limit_ms = limit.as_millis() as u64,
                        "dav request timed out"
                    );
                    Error::Timeout { limit }
                })?
                .map_err(Error::from_client)?;
            dav_debug!(
                method = %method,
                uri = %redact_userinfo(&uri),
                status = %resp.status(),
                duration_us = started.elapsed().as_micros() as u64,
                "dav request finished"
            );

            // Auth renewal (token providers only): a 401 triggers exactly one
            // refresh+retry per request, never a loop. Skipped once
            // credentials were deliberately stripped cross-origin (the 401
            // came from a server that must not receive them again) and it
            // does not consume the transient-retry budget. A still-failing
            // 401 is returned as-is.
            if resp.status() == StatusCode::UNAUTHORIZED
                && self.token_provider.is_some()
                && !auth_retried
                && !strip_credentials
            {
                auth_retried = true;
                if let Some(provider) = &self.token_provider {
                    provider.invalidate();
                    dav_debug!(
                        method = %method,
                        uri = %redact_userinfo(&uri),
                        "401 with token provider; refreshing token and retrying once"
                    );
                    continue;
                }
            }

            let should_retry =
                self.handle_request_compression_outcome(attempted_encoding, resp.status());
            if should_retry && attempt == 0 && body.is_some() {
                attempt += 1;
                continue;
            }

            // Transient-failure retry (429/503/504): honor `Retry-After`
            // (clamped to the backoff cap), exponential backoff + jitter
            // otherwise. The budget is
            // shared across the whole redirect chain; exhausted retries fall
            // through and the last response is returned as-is.
            let retryable = is_retryable_status(resp.status())
                && (self.retry_all || is_idempotent_method(&method));
            if retryable && retries < self.max_retries {
                let delay = retry_delay(
                    resp.status(),
                    resp.headers(),
                    retries,
                    self.retry_initial_backoff,
                    self.retry_backoff_cap,
                );
                retries += 1;
                dav_debug!(
                    method = %method,
                    uri = %redact_userinfo(&uri),
                    status = %resp.status(),
                    delay_ms = delay.as_millis() as u64,
                    attempt = retries,
                    "retrying after transient failure"
                );
                sleep(delay).await;
                continue;
            }
            if retryable && retries > 0 {
                dav_debug!(
                    method = %method,
                    uri = %redact_userinfo(&uri),
                    status = %resp.status(),
                    attempts = retries,
                    "retry budget exhausted; returning last response as-is"
                );
            }

            if !self.follow_redirects || !is_redirect_status(resp.status()) {
                return Ok((uri, resp));
            }

            if redirects >= self.max_redirects {
                return Err(Error::TooManyRedirects {
                    limit: self.max_redirects,
                });
            }

            let next = resp
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|loc| resolve_location(&uri, loc));
            let Some(next) = next else {
                return Ok((uri, resp));
            };

            // Never follow an https→http downgrade (RFC 6764 §6 is
            // TLS-first): return the 3xx so the caller observes the redirect
            // instead of silently sending the request — body included —
            // over plaintext.
            if is_https_to_http_downgrade(&uri, &next) {
                return Ok((uri, resp));
            }

            if !strip_credentials && !same_origin(&uri, &next) {
                strip_credentials = true;
                base_headers.remove(header::AUTHORIZATION);
                base_headers.remove(header::COOKIE);
                // Conditional validators are bound to the origin's resource
                // (RFC 9110 §13.1.1) and must not leak to a new origin.
                base_headers.remove(header::IF_MATCH);
                base_headers.remove(header::IF_NONE_MATCH);
            }
            if resp.status() == StatusCode::SEE_OTHER {
                method = Method::GET;
                body = None;
                base_headers.remove(header::CONTENT_TYPE);
                base_headers.remove(header::CONTENT_LENGTH);
                base_headers.remove(header::CONTENT_ENCODING);
            }

            dav_debug!(
                from = %redact_userinfo(&uri),
                to = %redact_userinfo(&next),
                status = %resp.status(),
                hop = redirects as u64 + 1,
                "following redirect"
            );
            uri = next;
            redirects += 1;
        }
    }

    /// Generic **aggregated send** with automatic decompression (br/zstd/gzip).
    ///
    /// Follows HTTP redirects (301/302/303/307/308) up to the configured
    /// `max_redirects` when `follow_redirects` is enabled; on 303 the request
    /// is re-sent as `GET` without a body, and `Authorization`/`Cookie`/
    /// `If-Match`/`If-None-Match` headers are stripped when a hop crosses
    /// origins. An `https`→`http` downgrade is never followed — the 3xx
    /// response is returned as-is. Exceeding the limit
    /// fails with [`Error::TooManyRedirects`](crate::Error::TooManyRedirects).
    ///
    /// Transient failures (`429`/`503`/`504`) are retried up to
    /// `max_retries` times (default 0 = disabled) when the method is
    /// retryable per the idempotency policy; see
    /// [`WebDavClientBuilder::max_retries`](crate::WebDavClientBuilder::max_retries).
    ///
    /// # Request body compression
    ///
    /// A caller-supplied `Content-Encoding` header is honored as-is: the body
    /// is forwarded verbatim and automatic compression (and its probe) is
    /// skipped — the body is assumed to be already encoded. Otherwise the
    /// negotiated request encoding is applied (see
    /// [`RequestCompressionMode`](crate::RequestCompressionMode)).
    ///
    /// # Response body
    ///
    /// The body is decompressed (br/zstd/gzip) when the response carries a
    /// `Content-Encoding`; empty bodies (e.g. `HEAD`, `204`) are returned
    /// as-is with their headers untouched.
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body_bytes: Option<Bytes>,
        per_req_timeout: Option<Duration>,
    ) -> Result<Response<Bytes>> {
        let (_, resp) = self
            .build_and_send(method, path, headers, body_bytes, per_req_timeout)
            .await?;

        // RFC 9110 §9.3.2: a `HEAD` response may advertise `Content-Encoding`
        // while carrying an empty body — feeding that to a decoder fails, and
        // the header rewrite below would mask the server-reported
        // `Content-Length`. An empty body needs no decompression, so it is
        // returned as-is with its headers untouched (covers `HEAD`, `204`,
        // and `304`).
        if resp.body().is_end_stream() {
            let (parts, _) = resp.into_parts();
            return Ok(Response::from_parts(parts, Bytes::new()));
        }

        let encodings = detect_encodings(resp.headers());
        let (mut parts, body) = resp.into_parts();

        let limit = per_req_timeout.unwrap_or(self.default_timeout);
        let decompressed = timeout(limit, decompress_body(body, &encodings))
            .await
            .map_err(|_| Error::Timeout { limit })??;
        normalize_decompressed_headers(&mut parts.headers, &encodings, decompressed.len());
        dav_trace!(bytes = decompressed.len(), "decompressed response body");

        Ok(Response::from_parts(parts, decompressed))
    }

    // ----------- Streaming send (for parsing on the fly) -----------

    /// Generic **streaming send**. Returns a `Response<Incoming>` (not aggregated).
    /// The caller must enforce its own read deadline on the returned body; the
    /// per-request timeout covers headers only.
    ///
    /// # Response encoding
    ///
    /// The client advertises `Accept-Encoding` on every request
    /// (RFC 9110 §12.5.3) but does **not** decompress streaming bodies: when
    /// the server compresses, the returned `Incoming` is still encoded.
    /// Inspect `Content-Encoding` (e.g. with
    /// [`detect_encoding`](crate::detect_encoding)) and wrap the body before
    /// parsing — see the "Streaming Large Responses" example in the crate
    /// root docs, which passes the detected encoding to
    /// `parse_multistatus_stream`.
    ///
    /// # Request body compression
    ///
    /// As in [`send`](Self::send), a caller-supplied `Content-Encoding` is
    /// honored as-is: the body is forwarded verbatim without automatic
    /// compression.
    ///
    /// Redirects are followed exactly as in [`send`](Self::send) before the
    /// final response is returned.
    pub async fn send_stream(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body_bytes: Option<Bytes>,
        per_req_timeout: Option<Duration>,
    ) -> Result<Response<Incoming>> {
        let (_, resp) = self
            .build_and_send(method, path, headers, body_bytes, per_req_timeout)
            .await?;
        Ok(resp)
    }

    // ----------- HTTP/WebDAV Verbs -----------

    /// Send an `OPTIONS` request.
    pub async fn options(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::OPTIONS, path, HeaderMap::new(), None, None)
            .await
    }

    /// Query the server's DAV compliance classes and extensions via an
    /// `OPTIONS` request, parsing the `DAV` response header (RFC 4918 §10.1).
    ///
    /// The `DAV` header is a comma-separated list of compliance class tokens
    /// (`1`, `2`, `3`) and optional extension tokens (e.g. `calendar-access`,
    /// `addressbook`). When the server omits the `DAV` header, all flags are
    /// `false` and `extensions` is empty.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::WebDavClient;
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = WebDavClient::new("https://dav.example.com/", None, None)?;
    /// let caps = client.capabilities("/").await?;
    /// if caps.class2 {
    ///     println!("server supports locking");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn capabilities(&self, path: &str) -> Result<DavCapabilities> {
        let response = self.options(path).await?;
        match response.headers().get("dav") {
            Some(value) => {
                let s = value
                    .to_str()
                    .map_err(|e| Error::other(format!("invalid DAV header value: {e}")))?;
                crate::webdav::types::parse_dav_header(s)
            }
            None => Ok(DavCapabilities::default()),
        }
    }

    /// Send a `HEAD` request.
    pub async fn head(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::HEAD, path, HeaderMap::new(), None, None)
            .await
    }

    /// Send a `GET` request and return the fully aggregated (and decompressed) body.
    pub async fn get(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::GET, path, HeaderMap::new(), None, None)
            .await
    }

    /// Send a `DELETE` request.
    pub async fn delete(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::DELETE, path, HeaderMap::new(), None, None)
            .await
    }

    /// Conditional `DELETE` guarded by `If-Match`.
    ///
    /// Accepts entity-tags returned by DAV servers, including quoted strong
    /// ETags, as well as bare ETags returned by some servers. Bare ETags are
    /// quoted before sending. Weak entity-tags (`W/"abc"`) are rejected
    /// client-side: RFC 9110 strong comparison means a weak validator would
    /// never match, making the operation a guaranteed `412`.
    ///
    /// # Errors
    ///
    /// Returns an error if the ETag is empty, cannot form a valid HTTP
    /// entity-tag, or is a weak entity-tag
    /// ([`Error::InvalidEtag`](crate::Error::InvalidEtag) with
    /// [`EtagReason::Weak`](crate::EtagReason::Weak)).
    pub async fn delete_if_match(&self, path: &str, etag: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(header::IF_MATCH, if_match_header_value(etag)?);
        self.send(Method::DELETE, path, h, None, None).await
    }

    /// Shared implementation behind the domain clients' `put_if_match` /
    /// `put_if_match_prefer`: a conditional `PUT` guarded by `If-Match` with
    /// an optional `Prefer` preference (RFC 7240).
    pub(crate) async fn put_if_match_with(
        &self,
        path: &str,
        body: Bytes,
        content_type: header::HeaderValue,
        etag: &str,
        prefer: Option<Prefer>,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, content_type);
        h.insert(header::IF_MATCH, if_match_header_value(etag)?);
        if let Some(prefer) = prefer {
            h.insert("Prefer", header::HeaderValue::from_static(prefer.as_str()));
        }
        self.send(Method::PUT, path, h, Some(body), None).await
    }

    /// Send a WebDAV `COPY` from `src_path` to an absolute `Destination` URL.
    ///
    /// `dest_absolute_url` must be an **absolute URI with scheme and
    /// authority, already percent-encoded** (RFC 4918 §10.3 Simple-ref): the
    /// value is validated and sent verbatim as the `Destination` header —
    /// resource names containing spaces, non-ASCII characters, `?`, or `#`
    /// must be encoded by the caller beforehand (e.g. with
    /// [`encode_path_segments`]). It is **not** percent-encoded here. Any
    /// other value fails with [`Error::InvalidInput`](crate::Error::InvalidInput)
    /// before any network I/O.
    pub async fn copy(
        &self,
        src_path: &str,
        dest_absolute_url: &str,
        overwrite: bool,
    ) -> Result<Response<Bytes>> {
        self.copy_move(b"COPY", src_path, dest_absolute_url, overwrite)
            .await
    }

    /// Send a WebDAV `MOVE` from `src_path` to an absolute `Destination` URL.
    ///
    /// `dest_absolute_url` follows the same contract as
    /// [`copy`](Self::copy): an absolute, already percent-encoded URI with
    /// scheme and authority, validated before any network I/O
    /// ([`Error::InvalidInput`](crate::Error::InvalidInput) otherwise).
    pub async fn r#move(
        &self,
        src_path: &str,
        dest_absolute_url: &str,
        overwrite: bool,
    ) -> Result<Response<Bytes>> {
        self.copy_move(b"MOVE", src_path, dest_absolute_url, overwrite)
            .await
    }

    async fn copy_move(
        &self,
        method: &[u8],
        src_path: &str,
        dest_absolute_url: &str,
        overwrite: bool,
    ) -> Result<Response<Bytes>> {
        // RFC 4918 §10.3 Simple-ref: the Destination is an absolute URI.
        // It is sent verbatim (no percent-encoding here), so reject values
        // that are not absolute URIs up front. Never echo the raw value in
        // errors and reject userinfo (RFC 9110 §3.2 — senders MUST NOT
        // generate it): the Destination may come from user config or
        // discovery output and could carry credentials.
        let dest = dest_absolute_url.parse::<Uri>().map_err(|_| {
            Error::InvalidInput(format!(
                "Destination must be an absolute, already percent-encoded URI, got {:?}",
                crate::common::redact_userinfo(dest_absolute_url)
            ))
        })?;
        if dest.scheme_str().is_none() || dest.host().is_none() {
            return Err(Error::InvalidInput(format!(
                "Destination must be an absolute URI with scheme and authority, got {:?}",
                crate::common::redact_userinfo(dest_absolute_url)
            )));
        }
        if dest
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
        {
            return Err(Error::InvalidInput(
                "Destination must not carry userinfo (RFC 9110 §3.2): pass credentials via the client's auth options, not the URI"
                    .to_string(),
            ));
        }
        let mut h = HeaderMap::new();
        h.insert(
            "Destination",
            header::HeaderValue::from_str(dest_absolute_url)?,
        );
        h.insert(
            "Overwrite",
            header::HeaderValue::from_static(if overwrite { "T" } else { "F" }),
        );
        self.send(Method::from_bytes(method)?, src_path, h, None, None)
            .await
    }

    /// Send a WebDAV `PROPFIND` with a custom XML body and `Depth` header.
    pub async fn propfind(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send(
            Method::from_bytes(b"PROPFIND")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Send a WebDAV `PROPPATCH` with a custom XML body.
    ///
    /// Sent with an explicit `Depth: 0` (RFC 4918 §9.2: PROPPATCH applies to
    /// the resource only).
    pub async fn proppatch(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_static("0"));
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send(
            Method::from_bytes(b"PROPPATCH")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Send a WebDAV `REPORT` with a custom XML body and `Depth`.
    pub async fn report(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send(
            Method::from_bytes(b"REPORT")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Send a WebDAV `MKCOL` to create a generic collection. Some servers accept an optional XML body.
    pub async fn mkcol(&self, path: &str, xml_body: Option<&str>) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        let body = xml_body.map(|s| {
            h.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/xml; charset=utf-8"),
            );
            Bytes::from(s.to_owned())
        });
        self.send(Method::from_bytes(b"MKCOL")?, path, h, body, None)
            .await
    }

    // ----------- WebDAV locking (RFC 4918 class 2) -----------

    /// Map a non-success response to an error, surfacing a `<D:error>`
    /// precondition body (RFC 4918 §16, §14.12) when the body carries one.
    fn status_error(operation: Operation, resp: Response<Bytes>) -> Error {
        let status = resp.status();
        match crate::webdav::streaming::parse_error_body(&resp.into_body()) {
            Ok(dav) if dav.precondition_code.is_some() => Error::UnexpectedStatusWithDav {
                operation,
                status,
                dav,
            },
            _ => Error::UnexpectedStatus { operation, status },
        }
    }

    /// Reject lock tokens that cannot appear in a Coded-URL (RFC 4918
    /// §10.5): empty, `<`, `>`, `(`, `)`, or any non-visible-ASCII
    /// character.
    fn validate_lock_token(token: &str) -> Result<()> {
        if token.is_empty() {
            return Err(Error::InvalidInput(
                "lock token cannot be empty".to_string(),
            ));
        }
        if !token
            .chars()
            .all(|c| c.is_ascii_graphic() && !matches!(c, '<' | '>' | '(' | ')'))
        {
            return Err(Error::InvalidInput(format!(
                "lock token contains characters invalid in a Coded-URL (RFC 4918 §10.5): {token:?}"
            )));
        }
        Ok(())
    }

    /// `Timeout: Second-N` header value, clamped to `u32::MAX` seconds
    /// (RFC 4918 §10.7: the value MUST NOT exceed 2^32-1).
    fn timeout_header_value(secs: u64) -> Result<header::HeaderValue> {
        Ok(header::HeaderValue::from_str(&format!(
            "Second-{}",
            secs.min(u64::from(u32::MAX))
        ))?)
    }

    /// Shared LOCK pipeline behind [`lock`](Self::lock) and
    /// [`refresh_lock`](Self::refresh_lock): sends the request, maps a
    /// non-success status (including `423 Locked`) to
    /// [`Error::UnexpectedStatus`] — or [`Error::UnexpectedStatusWithDav`]
    /// when the body carries a `<D:error>` precondition — parses the
    /// `lockdiscovery`/`activelock` response, and fills `timeout_secs` from
    /// the `Timeout` response header when the body omits `<D:timeout>`.
    /// A successful response without a lock token falls back to
    /// `fallback_token` (refresh path, RFC 4918 §9.10.2) or fails with
    /// [`Error::InvalidInput`] when there is none to fall back to.
    async fn lock_request(
        &self,
        path: &str,
        headers: HeaderMap,
        body: Option<Bytes>,
        fallback_token: Option<&str>,
    ) -> Result<LockInfo> {
        let resp = self
            .send(Method::from_bytes(b"LOCK")?, path, headers, body, None)
            .await?;
        if !resp.status().is_success() {
            return Err(Self::status_error(Operation::Lock, resp));
        }
        let header_timeout = resp
            .headers()
            .get("Timeout")
            .and_then(|v| v.to_str().ok())
            .and_then(crate::webdav::streaming::parse_lock_timeout);
        let mut info = crate::webdav::streaming::parse_lock_discovery_bytes(&resp.into_body())?;
        if info.timeout_secs.is_none() {
            info.timeout_secs = header_timeout;
        }
        if info.token.is_empty() {
            match fallback_token {
                Some(token) if !token.is_empty() => info.token = token.to_string(),
                _ => {
                    return Err(Error::InvalidInput(
                        "server returned no lock token for the LOCK response".to_string(),
                    ));
                }
            }
        }
        Ok(info)
    }

    /// Acquire a WebDAV lock on a resource (RFC 4918 §9.10).
    ///
    /// Sends `LOCK` with a `Depth: 0` header (RFC 4918 §9.10.4 — without it
    /// a collection lock would default to `infinity` and silently lock the
    /// whole subtree), a `Timeout: Second-N` header (when `timeout_secs` is
    /// given; the value is clamped to `u32::MAX` seconds per RFC 4918
    /// §10.7), and a `<D:lockinfo>` body built from `scope` and
    /// `owner_xml`. `owner_xml` is a raw XML fragment inserted inside
    /// `<D:owner>` (e.g. `<D:href>https://example.com/alice</D:href>`) — it
    /// is **not** escaped; escape plain-text owners with
    /// [`escape_xml`](crate::webdav::escape_xml). Pass an empty string to
    /// omit the `<D:owner>` element. Collection locking with
    /// `Depth: infinity` is out of scope.
    ///
    /// Returns the parsed `<D:activelock>`: the server-assigned lock `token`
    /// (send it in an `If` header on subsequent conditional writes — this
    /// client keeps **no implicit lock state**), the granted `timeout_secs`,
    /// `scope`, `owner`, `lockroot`, and `depth`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnexpectedStatus`] (operation
    /// [`Operation::Lock`](crate::Operation::Lock)) when the server rejects
    /// the lock — notably `423 Locked` when an incompatible lock already
    /// exists — or [`Error::UnexpectedStatusWithDav`] when the error body
    /// carries a `<D:error>` precondition (e.g. `no-conflicting-lock`,
    /// RFC 4918 §16). A 2xx response without a lock token fails with
    /// [`Error::InvalidInput`] (RFC 4918 §9.10.9).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::webdav::{LockScope, WebDavClient};
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = WebDavClient::new("https://dav.example.com/", None, None)?;
    /// let lock = client
    ///     .lock(
    ///         "docs/plan.txt",
    ///         LockScope::Exclusive,
    ///         "<D:href>https://example.com/alice</D:href>",
    ///         Some(300),
    ///     )
    ///     .await?;
    /// println!("lock token: {}", lock.token);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn lock(
        &self,
        path: &str,
        scope: LockScope,
        owner_xml: &str,
        timeout_secs: Option<u64>,
    ) -> Result<LockInfo> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_static("0"));
        if let Some(secs) = timeout_secs {
            h.insert("Timeout", Self::timeout_header_value(secs)?);
        }
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        let owner = owner_xml.trim();
        let owner_el = if owner.is_empty() {
            String::new()
        } else {
            format!("\n  <D:owner>{owner}</D:owner>")
        };
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:{}/></D:lockscope>
  <D:locktype><D:write/></D:locktype>{}
</D:lockinfo>"#,
            scope.as_str(),
            owner_el,
        );
        self.lock_request(path, h, Some(Bytes::from(body)), None)
            .await
    }

    /// Refresh an existing WebDAV lock (RFC 4918 §7.7 / §9.10.7): the `LOCK`
    /// request is re-issued **without a body**, carrying the lock token in an
    /// `If` header and the requested `Timeout` (when `timeout_secs` is
    /// given; the value is clamped to `u32::MAX` seconds per RFC 4918
    /// §10.7). Returns the refreshed `<D:activelock>` — the server may grant
    /// a new timeout and may rotate the token, so use the returned `LockInfo`
    /// afterwards. A conforming server may omit `<D:locktoken>` on refresh
    /// (RFC 4918 §9.10.2), in which case the request token is returned
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnexpectedStatus`] (operation
    /// [`Operation::Lock`](crate::Operation::Lock)) on non-success statuses,
    /// [`Error::UnexpectedStatusWithDav`] when the error body carries a
    /// `<D:error>` precondition, `412 Precondition Failed` when the lock no
    /// longer exists, and [`Error::InvalidInput`] for an empty token or a
    /// token containing characters invalid in a Coded-URL (RFC 4918 §10.5).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::webdav::WebDavClient;
    ///
    /// # async fn run(client: &WebDavClient, token: &str) -> fast_dav_rs::Result<()> {
    /// let refreshed = client.refresh_lock("docs/plan.txt", token, Some(300)).await?;
    /// println!("refreshed, timeout: {:?}", refreshed.timeout_secs);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn refresh_lock(
        &self,
        path: &str,
        token: &str,
        timeout_secs: Option<u64>,
    ) -> Result<LockInfo> {
        let token = token.trim();
        Self::validate_lock_token(token)?;
        let mut h = HeaderMap::new();
        h.insert(
            "If",
            header::HeaderValue::from_str(&format!("(<{token}>)"))?,
        );
        if let Some(secs) = timeout_secs {
            h.insert("Timeout", Self::timeout_header_value(secs)?);
        }
        self.lock_request(path, h, None, Some(token)).await
    }

    /// Remove a WebDAV lock (RFC 4918 §9.11): sends `UNLOCK` with the lock
    /// token in a `Lock-Token` header. Succeeds on any 2xx status (typically
    /// `204 No Content`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnexpectedStatus`] (operation
    /// [`Operation::Unlock`](crate::Operation::Unlock)) on non-success
    /// statuses — e.g. `409 Conflict` when the lock token does not match an
    /// existing lock — [`Error::UnexpectedStatusWithDav`] when the error
    /// body carries a `<D:error>` precondition, and [`Error::InvalidInput`]
    /// for an empty token or a token containing characters invalid in a
    /// Coded-URL (RFC 4918 §10.5).
    pub async fn unlock(&self, path: &str, token: &str) -> Result<()> {
        let token = token.trim();
        Self::validate_lock_token(token)?;
        let mut h = HeaderMap::new();
        h.insert(
            "Lock-Token",
            header::HeaderValue::from_str(&format!("<{token}>"))?,
        );
        let resp = self
            .send(Method::from_bytes(b"UNLOCK")?, path, h, None, None)
            .await?;
        if !resp.status().is_success() {
            return Err(Self::status_error(Operation::Unlock, resp));
        }
        Ok(())
    }

    /// Run many `PROPFIND`s concurrently with a semaphore-bound concurrency limit.
    pub async fn propfind_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        let requests = paths.into_iter().map(move |p| (p, xml_body.clone()));
        self.many(
            // ponytail: static literal cannot fail; no-panic needs Result signatures (0.10 window)
            Method::from_bytes(b"PROPFIND").unwrap(),
            requests,
            depth,
            max_concurrency,
        )
        .await
    }

    /// Run many `REPORT`s concurrently with a semaphore-bound concurrency limit.
    pub async fn report_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        let requests = paths.into_iter().map(move |p| (p, xml_body.clone()));
        self.many(
            // ponytail: static literal cannot fail; no-panic needs Result signatures (0.10 window)
            Method::from_bytes(b"REPORT").unwrap(),
            requests,
            depth,
            max_concurrency,
        )
        .await
    }

    /// Run many `REPORT`s to the same collection with per-request bodies,
    /// concurrently bounded by a semaphore (same machinery as
    /// [`Self::report_many`], used by the CalDAV batched multiget). Sent with
    /// `Depth: 0` (multiget REPORTs SHOULD NOT use `Depth: 1`, RFC 4791 §7.9 /
    /// RFC 6352 §8.7).
    pub(crate) async fn report_many_bodies(
        &self,
        requests: impl IntoIterator<Item = (String, Arc<Bytes>)>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        self.many(
            // ponytail: static literal cannot fail; no-panic needs Result signatures (0.10 window)
            Method::from_bytes(b"REPORT").unwrap(),
            requests,
            Depth::Zero,
            max_concurrency,
        )
        .await
    }

    async fn many(
        &self,
        method: Method,
        requests: impl IntoIterator<Item = (String, Arc<Bytes>)>,
        depth: Depth,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        let sem = Arc::new(Semaphore::new(max_concurrency.max(1)));
        let mut tasks = FuturesOrdered::new();

        for (path, body) in requests {
            let sem_clone = sem.clone();
            let this = self.clone();
            let p = path.clone();
            let method = method.clone();
            tasks.push_back(async move {
                // ponytail: semaphore is private and never closed; expect cannot fire
                let _permit: OwnedSemaphorePermit =
                    sem_clone.acquire_owned().await.expect("semaphore closed");
                let mut h = HeaderMap::new();
                // ponytail: Depth::as_str is enum-controlled; header parse cannot fail
                h.insert(
                    "Depth",
                    header::HeaderValue::from_str(depth.as_str()).unwrap(),
                );
                h.insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static("application/xml; charset=utf-8"),
                );
                let res = this.send(method, &p, h, Some((*body).clone()), None).await;
                BatchItem {
                    pub_path: p.clone(),
                    hrefs: vec![p],
                    result: res,
                }
            });
        }

        let mut out = Vec::new();
        while let Some(item) = tasks.next().await {
            out.push(item);
        }
        out
    }

    /// Check whether the server supports WebDAV-Sync (RFC 6578) on the base collection.
    ///
    /// Detection strategy:
    /// 1. **`DAV:supported-report-set`** — a `PROPFIND` with `Depth: 0` asks the
    ///    collection which reports it supports; when the multistatus body
    ///    advertises the `sync-collection` report, support is confirmed.
    /// 2. **Probe REPORT fallback** — when the `PROPFIND` does not confirm
    ///    support, a minimal `sync-collection` REPORT is attempted. Only a 2xx
    ///    status (which includes `207 Multi-Status`) counts as
    ///    [`SyncCapability::Supported`].
    ///
    /// Returns [`SyncCapability::Unknown`] when the probe fails at the
    /// transport level (connection refused, timeout, …): the server's support
    /// could not be determined. Callers must not treat `Unknown` as
    /// "unsupported" — a client that falls back to full-list polling on a
    /// transient network error silently degrades every sync cycle.
    pub async fn supports_webdav_sync(&self) -> Result<SyncCapability> {
        // Primary: ask the collection which reports it supports (RFC 3253 §3.1.5).
        let supported_report_set = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:supported-report-set/>
  </D:prop>
</D:propfind>"#;

        if let Ok(response) = self.propfind("", Depth::Zero, supported_report_set).await {
            if response.status().is_success() {
                let body = String::from_utf8_lossy(response.body());
                if body.to_ascii_lowercase().contains("sync-collection") {
                    return Ok(SyncCapability::Supported);
                }
            }
        }

        // Fallback: attempt a minimal sync-collection REPORT; only a 2xx
        // answer proves support. Depth: 0 per RFC 6578; `<D:sync-level>`
        // scopes how deep the sync goes. A transport/timeout error leaves the
        // support undetermined (`Unknown`) instead of reporting "unsupported".
        let test_sync = r#"<D:sync-collection xmlns:D="DAV:">
            <D:sync-token/>
            <D:sync-level>1</D:sync-level>
            <D:prop>
                <D:getetag/>
            </D:prop>
        </D:sync-collection>"#;

        match self.report("", Depth::Zero, test_sync).await {
            Ok(response) if response.status().is_success() => Ok(SyncCapability::Supported),
            Ok(_) => Ok(SyncCapability::Unsupported),
            Err(_) => Ok(SyncCapability::Unknown),
        }
    }

    /// Send a `sync-collection` REPORT (RFC 6578) with a configurable
    /// `sync-level`, returning the raw parsed multistatus: the response
    /// headers, the response items, and the sync token (top-level element,
    /// then `Sync-Token` header, then the first per-item token).
    ///
    /// `namespace` and `data_element` select the CalDAV/CardDAV data
    /// property requested alongside `getetag` (e.g. `calendar-data`).
    ///
    /// # Truncation
    ///
    /// When the server truncates the result set (RFC 6578 §3.6), it reports
    /// `507 Insufficient Storage` inside the 207 multistatus — normally on
    /// the request-URI. That response element surfaces as an ordinary item
    /// with `status: Some("HTTP/1.1 507 Insufficient Storage")`; inspect
    /// `items` for a 507 status (or use the CalDAV/CardDAV `sync_collection`
    /// wrappers, which set `SyncResponse.truncated`) and use the returned
    /// sync token to fetch the next page.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnexpectedStatus`] (operation
    /// [`Operation::ReportSyncCollection`](crate::Operation::ReportSyncCollection))
    /// when the server answers with a non-success status.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::WebDavClient;
    /// use fast_dav_rs::webdav::SyncLevel;
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = WebDavClient::new("https://dav.example.com/cal/", None, None)?;
    /// let (headers, items, sync_token) = client
    ///     .sync_collection_with_level(
    ///         "calendars/user/work/",
    ///         None,
    ///         None,
    ///         true,
    ///         "urn:ietf:params:xml:ns:caldav",
    ///         "calendar-data",
    ///         SyncLevel::Infinite,
    ///     )
    ///     .await?;
    /// println!("token: {sync_token:?}, items: {}", items.len());
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub async fn sync_collection_with_level(
        &self,
        path: &str,
        sync_token: Option<&str>,
        limit: Option<u32>,
        include_data: bool,
        namespace: &str,
        data_element: &str,
        sync_level: SyncLevel,
    ) -> Result<(HeaderMap, Vec<DavItem>, Option<String>)> {
        let body = crate::webdav::xml::build_sync_collection_body(
            sync_token,
            limit,
            include_data,
            namespace,
            data_element,
            None,
            sync_level,
        );
        self.sync_collection_report(path, &body).await
    }

    /// 410-Gone-resilient variant of
    /// [`sync_collection_with_level`](Self::sync_collection_with_level)
    /// (RFC 6578 §3.11): when the server rejects the incremental request
    /// with `410 Gone` (stale sync token), or with `403 Forbidden` plus the
    /// `valid-sync-token` precondition (§3.2 alternative stale signal), the
    /// report is re-issued with an empty sync token (initial sync), the full
    /// result set with the new token is returned, and the last tuple element
    /// (`resynced`) is `true`. Uses [`SyncLevel::One`].
    ///
    /// A `resynced == true` result is an **initial sync**: per RFC 6578 §3.4
    /// it MUST NOT report deletions that predate the stale token, so callers
    /// must rebuild their caches from the returned items instead of applying
    /// them incrementally.
    ///
    /// Result-set truncation (RFC 6578 §3.6) surfaces as an item with a
    /// `HTTP/1.1 507 Insufficient Storage` status (see
    /// [`sync_collection_with_level`](Self::sync_collection_with_level)).
    ///
    /// Any other error propagates unchanged.
    ///
    /// # Errors
    ///
    /// Returns the error of the underlying report; a `410 Gone` (or a `403`
    /// with `valid-sync-token`) triggers one retry as an initial sync, and
    /// the second failure is returned as-is.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::WebDavClient;
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = WebDavClient::new("https://dav.example.com/cal/", None, None)?;
    /// let (headers, items, sync_token, resynced) = client
    ///     .sync_collection_resilient(
    ///         "calendars/user/work/",
    ///         Some("http://example.com/sync/stale"),
    ///         None,
    ///         true,
    ///         "urn:ietf:params:xml:ns:caldav",
    ///         "calendar-data",
    ///     )
    ///     .await?;
    /// if resynced {
    ///     println!("stale token: rebuild caches from {} items", items.len());
    /// } else {
    ///     println!("token: {sync_token:?}, items: {}", items.len());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn sync_collection_resilient(
        &self,
        path: &str,
        sync_token: Option<&str>,
        limit: Option<u32>,
        include_data: bool,
        namespace: &str,
        data_element: &str,
    ) -> Result<(HeaderMap, Vec<DavItem>, Option<String>, bool)> {
        self.sync_collection_resilient_report(path, sync_token, |token| {
            crate::webdav::xml::build_sync_collection_body(
                token,
                limit,
                include_data,
                namespace,
                data_element,
                None,
                SyncLevel::One,
            )
        })
        .await
    }

    /// Shared implementation behind the `sync-collection` methods: send the
    /// report with `Depth: 0`, map a non-success status to
    /// [`Error::UnexpectedStatus`], parse the multistatus, and return the
    /// raw parts for domain-specific mapping.
    pub(crate) async fn sync_collection_report(
        &self,
        path: &str,
        xml_body: &str,
    ) -> Result<(HeaderMap, Vec<DavItem>, Option<String>)> {
        let resp = self.report(path, Depth::Zero, xml_body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportSyncCollection,
                status: resp.status(),
            });
        }
        parse_sync_response(resp)
    }

    /// Shared implementation behind `sync_collection_resilient`: issues the
    /// report built by `build_body(sync_token)`; on a stale sync token
    /// (RFC 6578 §3.11: `410 Gone`; §3.2 alternative: `403 Forbidden` +
    /// `valid-sync-token` precondition) re-issues it once with an empty sync
    /// token (initial sync) and sets the `resynced` flag. Any other error
    /// propagates unchanged.
    pub(crate) async fn sync_collection_resilient_report(
        &self,
        path: &str,
        sync_token: Option<&str>,
        build_body: impl Fn(Option<&str>) -> String,
    ) -> Result<(HeaderMap, Vec<DavItem>, Option<String>, bool)> {
        let resp = self
            .report(path, Depth::Zero, &build_body(sync_token))
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let stale = status == StatusCode::GONE
                || (status == StatusCode::FORBIDDEN && {
                    let err =
                        crate::webdav::streaming::parse_error_body(resp.body()).unwrap_or_default();
                    err.precondition_code.as_deref() == Some("valid-sync-token")
                });
            if !stale {
                return Err(Error::UnexpectedStatus {
                    operation: Operation::ReportSyncCollection,
                    status,
                });
            }
            // Stale token: re-issue once as an initial sync (empty token);
            // the second failure propagates unchanged.
            let (headers, items, token) =
                self.sync_collection_report(path, &build_body(None)).await?;
            return Ok((headers, items, token, true));
        }
        parse_sync_response(resp).map(|(headers, items, token)| (headers, items, token, false))
    }

    /// Discover the current user's principal URL via `current-user-principal`.
    ///
    /// Returns `None` if the server omits the property.
    pub async fn discover_current_user_principal(&self) -> Result<Option<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:current-user-principal/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind("", Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindCurrentUserPrincipal,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        crate::webdav::streaming::parse_current_user_principal_bytes(&body)
    }

    /// Streaming variant of `PROPFIND`, returning the non-aggregated body.
    ///
    /// The body may still be compressed when the server honors the
    /// `Accept-Encoding` the client advertises: check `Content-Encoding` and
    /// decode before parsing (see [`send_stream`](Self::send_stream)).
    pub async fn propfind_stream(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Incoming>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send_stream(
            Method::from_bytes(b"PROPFIND")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Streaming variant of `REPORT`, returning the non-aggregated body.
    ///
    /// The body may still be compressed when the server honors the
    /// `Accept-Encoding` the client advertises: check `Content-Encoding` and
    /// decode before parsing (see [`send_stream`](Self::send_stream)).
    pub async fn report_stream(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Incoming>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send_stream(
            Method::from_bytes(b"REPORT")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }
}

/// Generates the shared delegate methods for the thin CalDAV/CardDAV client
/// wrappers over [`WebDavClient`].
///
/// The wrapper struct must own a `webdav: WebDavClient` field.
/// `$content_type` is the `Content-Type` used for conditional `PUT` bodies
/// (iCalendar vs. vCard); `$namespace`/`$data_element` select the sync-collection
/// data property; `$sync_response`/`$map_sync_response` are the domain sync
/// response type and its mapper (`map_sync_response`).
///
/// The optional trailing arguments (`$extra_field: $extra_ty` field threaded
/// through `from_webdav`, and the `$validate` method name) wire client-side
/// body validation into the conditional `PUT` methods: `$validate` names a
/// `fn(&Bytes) -> Result<HeaderValue>` method on `$client` that runs before
/// any network I/O and supplies the wire `Content-Type` (iCalendar
/// validation for CalDAV). Omit both for clients whose bodies need no
/// client-side validation (CardDAV/vCard).
#[macro_export]
macro_rules! impl_dav_client_delegates {
    (
        $client:ident,
        $content_type:expr,
        $namespace:expr,
        $data_element:expr,
        $sync_response:ty,
        $map_sync_response:path
        $(, validation_level: $extra_field:ident : $extra_ty:ty)?
        $(, validate: $validate:ident)?
    ) => {
        impl $client {
            /// Wrap a [`WebDavClient`] into this client type.
            pub(crate) fn from_webdav(
                webdav: $crate::webdav::WebDavClient
                $(, $extra_field: $extra_ty)?
            ) -> Self {
                Self { webdav, $($extra_field)? }
            }

            /// Configure the request compression strategy.
            pub fn set_request_compression_mode(
                &self,
                mode: $crate::webdav::client::RequestCompressionMode,
            ) {
                self.webdav.set_request_compression_mode(mode);
            }

            /// Get the current request compression strategy.
            pub fn request_compression_mode(
                &self,
            ) -> $crate::webdav::client::RequestCompressionMode {
                self.webdav.request_compression_mode()
            }

            /// Get the currently resolved request compression encoding.
            pub fn request_compression(&self) -> $crate::common::compression::ContentEncoding {
                self.webdav.request_compression()
            }

            pub fn build_uri(&self, path: &str) -> $crate::Result<hyper::Uri> {
                self.webdav.build_uri(path)
            }

            /// Generic **aggregated send** with automatic decompression
            /// (br/zstd/gzip). A caller-supplied `Content-Encoding` on the
            /// request is honored: the body is sent verbatim without
            /// automatic compression.
            pub async fn send(
                &self,
                method: hyper::Method,
                path: &str,
                headers: hyper::HeaderMap,
                body_bytes: Option<bytes::Bytes>,
                per_req_timeout: Option<std::time::Duration>,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav
                    .send(method, path, headers, body_bytes, per_req_timeout)
                    .await
            }

            /// Generic **streaming send**. Returns a `Response<Incoming>` (not aggregated).
            /// The caller must enforce its own read deadline on the returned body; the
            /// per-request timeout covers headers only.
            ///
            /// The body may still be encoded when the server compresses: the
            /// client advertises `Accept-Encoding` but leaves response
            /// decoding to the caller — see [`WebDavClient::send_stream`].
            pub async fn send_stream(
                &self,
                method: hyper::Method,
                path: &str,
                headers: hyper::HeaderMap,
                body_bytes: Option<bytes::Bytes>,
                per_req_timeout: Option<std::time::Duration>,
            ) -> $crate::Result<hyper::Response<hyper::body::Incoming>> {
                self.webdav
                    .send_stream(method, path, headers, body_bytes, per_req_timeout)
                    .await
            }

            /// Send an `OPTIONS` request.
            pub async fn options(
                &self,
                path: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.options(path).await
            }

            /// Send a `HEAD` request.
            pub async fn head(&self, path: &str) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.head(path).await
            }

            /// Send a `GET` request and return the fully aggregated (and decompressed) body.
            pub async fn get(&self, path: &str) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.get(path).await
            }

            /// Send a `DELETE` request.
            pub async fn delete(
                &self,
                path: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.delete(path).await
            }

            /// Conditional `DELETE` guarded by `If-Match`.
            ///
            /// # Errors
            ///
            /// Returns an error if the ETag is empty, malformed, or a weak
            /// entity-tag (`Error::InvalidEtag` with `EtagReason::Weak` —
            /// RFC 9110 strong comparison means weak validators never match
            /// `If-Match`).
            pub async fn delete_if_match(
                &self,
                path: &str,
                etag: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.delete_if_match(path, etag).await
            }

            /// Conditional `PUT` guarded by `If-Match`.
            ///
            /// The write only succeeds if the current resource ETag matches.
            /// Quoted strong ETags are accepted; bare ETags returned by some
            /// servers are quoted automatically. Weak entity-tags (`W/"abc"`)
            /// are rejected client-side: RFC 9110 strong comparison means a
            /// weak validator would never match, making the operation a
            /// guaranteed `412`.
            ///
            /// # Errors
            ///
            /// Returns an error if the path cannot be resolved to a valid
            /// URI, the ETag is empty, malformed, or a weak entity-tag
            /// (`Error::InvalidEtag` with `EtagReason::Weak`), or a
            /// network/server error occurs.
            pub async fn put_if_match(
                &self,
                path: &str,
                body: bytes::Bytes,
                etag: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                #[allow(unused_mut, unused_assignments)]
                let mut content_type =
                    hyper::header::HeaderValue::from_static($content_type);
                $(content_type = self.$validate(&body)?;)?
                self.webdav
                    .put_if_match_with(path, body, content_type, etag, None)
                    .await
            }

            /// Conditional `PUT` guarded by `If-Match`, additionally sending
            /// `Prefer: return=representation` (RFC 7240) so servers that
            /// honor it include the stored representation (typically with the
            /// new `ETag`) in the response.
            ///
            /// Check whether the server actually applied the preference with
            /// [`preference_applied_from_headers`](crate::webdav::preference_applied_from_headers)
            /// — servers may silently ignore it.
            ///
            /// # Errors
            ///
            /// Returns an error if the path cannot be resolved to a valid
            /// URI, the ETag is empty, malformed, or a weak entity-tag
            /// (`Error::InvalidEtag` with `EtagReason::Weak` — RFC 9110
            /// strong comparison means weak validators never match
            /// `If-Match`), or a network/server error occurs.
            ///
            /// # Example
            ///
            /// ```no_run
            /// use bytes::Bytes;
            /// use fast_dav_rs::CalDavClient;
            ///
            /// # async fn example(client: &CalDavClient) -> fast_dav_rs::Result<()> {
            /// let body = Bytes::from_static(b"BEGIN:VCALENDAR\nEND:VCALENDAR\n");
            /// let resp = client
            ///     .put_if_match_prefer("event.ics", body, "\"etag-1\"")
            ///     .await?;
            /// let honored = fast_dav_rs::preference_applied_from_headers(resp.headers());
            /// # Ok(())
            /// # }
            /// ```
            pub async fn put_if_match_prefer(
                &self,
                path: &str,
                body: bytes::Bytes,
                etag: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                #[allow(unused_mut, unused_assignments)]
                let mut content_type =
                    hyper::header::HeaderValue::from_static($content_type);
                $(content_type = self.$validate(&body)?;)?
                self.webdav
                    .put_if_match_with(
                        path,
                        body,
                        content_type,
                        etag,
                        Some($crate::webdav::Prefer::Representation),
                    )
                    .await
            }

            /// Send a WebDAV `COPY` from `src_path` to an absolute `Destination` URL.
            pub async fn copy(
                &self,
                src_path: &str,
                dest_absolute_url: &str,
                overwrite: bool,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav
                    .copy(src_path, dest_absolute_url, overwrite)
                    .await
            }

            /// Send a WebDAV `MOVE` from `src_path` to an absolute `Destination` URL.
            pub async fn r#move(
                &self,
                src_path: &str,
                dest_absolute_url: &str,
                overwrite: bool,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav
                    .r#move(src_path, dest_absolute_url, overwrite)
                    .await
            }

            /// Send a WebDAV `PROPFIND` with a custom XML body and `Depth` header.
            pub async fn propfind(
                &self,
                path: &str,
                depth: $crate::Depth,
                xml_body: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.propfind(path, depth, xml_body).await
            }

            /// Send a WebDAV `PROPPATCH` with a custom XML body.
            pub async fn proppatch(
                &self,
                path: &str,
                xml_body: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.proppatch(path, xml_body).await
            }

            /// Send a `REPORT` with a custom XML body and `Depth`.
            pub async fn report(
                &self,
                path: &str,
                depth: $crate::Depth,
                xml_body: &str,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.report(path, depth, xml_body).await
            }

            /// Send a WebDAV `MKCOL` to create a generic collection.
            pub async fn mkcol(
                &self,
                path: &str,
                xml_body: Option<&str>,
            ) -> $crate::Result<hyper::Response<bytes::Bytes>> {
                self.webdav.mkcol(path, xml_body).await
            }

            /// Acquire a WebDAV lock on a resource (`LOCK`, RFC 4918 §9.10).
            ///
            /// `owner_xml` is a raw XML fragment inserted inside `<D:owner>`
            /// (e.g. `<D:href>https://example.com/alice</D:href>`); an empty
            /// string omits the element. Returns the parsed `<D:activelock>`
            /// with the server-assigned lock token. The client keeps no
            /// implicit lock state — pass the token to
            /// [`refresh_lock`](Self::refresh_lock) /
            /// [`unlock`](Self::unlock), and send it in an `If` header (via
            /// the low-level `send`) on conditional writes. Check
            /// `capabilities().class2` to confirm the server supports locking.
            ///
            /// # Errors
            ///
            /// Returns [`Error::UnexpectedStatus`] (operation
            /// [`Operation::Lock`](crate::Operation::Lock)) on non-success
            /// statuses, including `423 Locked`.
            pub async fn lock(
                &self,
                path: &str,
                scope: $crate::webdav::LockScope,
                owner_xml: &str,
                timeout_secs: Option<u64>,
            ) -> $crate::Result<$crate::webdav::LockInfo> {
                self.webdav
                    .lock(path, scope, owner_xml, timeout_secs)
                    .await
            }

            /// Refresh an existing WebDAV lock (`LOCK` re-issued with the
            /// lock token in an `If` header, RFC 4918 §9.10.7).
            pub async fn refresh_lock(
                &self,
                path: &str,
                token: &str,
                timeout_secs: Option<u64>,
            ) -> $crate::Result<$crate::webdav::LockInfo> {
                self.webdav.refresh_lock(path, token, timeout_secs).await
            }

            /// Remove a WebDAV lock (`UNLOCK`, RFC 4918 §9.11) with the token
            /// in a `Lock-Token` header. Succeeds on any 2xx (typically `204`).
            pub async fn unlock(&self, path: &str, token: &str) -> $crate::Result<()> {
                self.webdav.unlock(path, token).await
            }

            /// Discover the current user's principal URL via `current-user-principal`.
            pub async fn discover_current_user_principal(&self) -> $crate::Result<Option<String>> {
                self.webdav.discover_current_user_principal().await
            }

            /// Run many `PROPFIND`s concurrently with a semaphore-bound concurrency limit.
            pub async fn propfind_many(
                &self,
                paths: impl IntoIterator<Item = String>,
                depth: $crate::Depth,
                xml_body: std::sync::Arc<bytes::Bytes>,
                max_concurrency: usize,
            ) -> Vec<$crate::BatchItem<hyper::Response<bytes::Bytes>>> {
                self.webdav
                    .propfind_many(paths, depth, xml_body, max_concurrency)
                    .await
            }

            /// Run many `REPORT`s concurrently with a semaphore-bound concurrency limit.
            pub async fn report_many(
                &self,
                paths: impl IntoIterator<Item = String>,
                depth: $crate::Depth,
                xml_body: std::sync::Arc<bytes::Bytes>,
                max_concurrency: usize,
            ) -> Vec<$crate::BatchItem<hyper::Response<bytes::Bytes>>> {
                self.webdav
                    .report_many(paths, depth, xml_body, max_concurrency)
                    .await
            }

            /// Check whether the server supports WebDAV-Sync (RFC 6578).
            pub async fn supports_webdav_sync(&self) -> $crate::Result<$crate::SyncCapability> {
                self.webdav.supports_webdav_sync().await
            }

            /// Streaming variant of `PROPFIND`, returning the non-aggregated body.
            ///
            /// The body may still be compressed when the server honors the
            /// `Accept-Encoding` the client advertises — decode before
            /// parsing (see [`WebDavClient::send_stream`]).
            pub async fn propfind_stream(
                &self,
                path: &str,
                depth: $crate::Depth,
                xml_body: &str,
            ) -> $crate::Result<hyper::Response<hyper::body::Incoming>> {
                self.webdav.propfind_stream(path, depth, xml_body).await
            }

            /// Streaming variant of `REPORT`, returning the non-aggregated body.
            ///
            /// The body may still be compressed when the server honors the
            /// `Accept-Encoding` the client advertises — decode before
            /// parsing (see [`WebDavClient::send_stream`]).
            pub async fn report_stream(
                &self,
                path: &str,
                depth: $crate::Depth,
                xml_body: &str,
            ) -> $crate::Result<hyper::Response<hyper::body::Incoming>> {
                self.webdav.report_stream(path, depth, xml_body).await
            }

            /// Incrementally synchronise a collection using `sync-collection`
            /// with a configurable `sync-level` (RFC 6578 §3.3).
            ///
            /// `sync_level` scopes the report: `SyncLevel::One` restricts the
            /// sync to the collection members, `SyncLevel::Infinite` includes
            /// all descendants. The existing `sync_collection` keeps the
            /// `SyncLevel::One` behavior.
            ///
            /// # Truncation
            ///
            /// If the server truncates the result set (RFC 6578 §3.6), the
            /// returned sync response has `truncated == true` and the
            /// request-URI appears in `items` with a `HTTP/1.1 507
            /// Insufficient Storage` status. The returned sync token is valid
            /// for fetching the next page of changes.
            ///
            /// # Errors
            ///
            /// Returns an error if the REPORT request fails or the server
            /// responds with a non-success status.
            ///
            /// # Example
            ///
            /// ```no_run
            /// use fast_dav_rs::{CalDavClient, SyncLevel};
            ///
            /// # async fn example(client: &CalDavClient) -> fast_dav_rs::Result<()> {
            /// let sync = client
            ///     .sync_collection_with_level("calendars/user/work/", None, None, true, SyncLevel::Infinite)
            ///     .await?;
            /// println!("token: {:?}", sync.sync_token);
            /// # Ok(())
            /// # }
            /// ```
            pub async fn sync_collection_with_level(
                &self,
                path: &str,
                sync_token: Option<&str>,
                limit: Option<u32>,
                include_data: bool,
                sync_level: $crate::webdav::SyncLevel,
            ) -> $crate::Result<$sync_response> {
                let (headers, items, token) = self
                    .webdav
                    .sync_collection_with_level(
                        path,
                        sync_token,
                        limit,
                        include_data,
                        $namespace,
                        $data_element,
                        sync_level,
                    )
                    .await?;
                Ok($map_sync_response(&headers, items, token))
            }

            /// 410-Gone-resilient sync-collection (RFC 6578 §3.11): when the
            /// server rejects the incremental request with `410 Gone` (stale
            /// sync token), or with `403 Forbidden` plus the
            /// `valid-sync-token` precondition (§3.2 alternative stale
            /// signal), the report is automatically re-issued with an empty
            /// sync token (initial sync) and the full result set with the new
            /// token is returned with the `resynced` field set to `true`.
            /// Any other error propagates unchanged.
            ///
            /// A `resynced == true` response is an **initial sync**: per
            /// RFC 6578 §3.4 it MUST NOT report deletions that predate the
            /// stale token, so callers must rebuild their caches from the
            /// returned items instead of applying them incrementally.
            ///
            /// Result-set truncation (RFC 6578 §3.6) sets `truncated == true`
            /// on the returned sync response; the returned token remains
            /// valid for fetching the next page.
            ///
            /// # Errors
            ///
            /// Returns the error of the underlying report; a `410 Gone` (or
            /// a `403` with `valid-sync-token`) triggers one retry as an
            /// initial sync, and the second failure is returned as-is.
            ///
            /// # Example
            ///
            /// ```no_run
            /// use fast_dav_rs::CalDavClient;
            ///
            /// # async fn example(client: &CalDavClient) -> fast_dav_rs::Result<()> {
            /// let sync = client
            ///     .sync_collection_resilient("calendars/user/work/", Some("stale-token"), None, true)
            ///     .await?;
            /// if sync.resynced {
            ///     println!("stale token: rebuild caches from {} items", sync.items.len());
            /// } else {
            ///     println!("token: {:?}", sync.sync_token);
            /// }
            /// # Ok(())
            /// # }
            /// ```
            pub async fn sync_collection_resilient(
                &self,
                path: &str,
                sync_token: Option<&str>,
                limit: Option<u32>,
                include_data: bool,
            ) -> $crate::Result<$sync_response> {
                let (headers, items, token, resynced) = self
                    .webdav
                    .sync_collection_resilient(path, sync_token, limit, include_data, $namespace, $data_element)
                    .await?;
                let mut response = $map_sync_response(&headers, items, token);
                response.resynced = resynced;
                Ok(response)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::compression::ContentEncoding;

    const BASE: &str = "https://dav.example.com/user01/";

    #[test]
    fn clone_shares_compression_mode() {
        let client_a = WebDavClient::new(BASE, None, None).unwrap();
        let client_b = client_a.clone();

        client_a.set_request_compression_mode(RequestCompressionMode::Force(ContentEncoding::Zstd));

        assert_eq!(
            client_b.request_compression_mode(),
            RequestCompressionMode::Force(ContentEncoding::Zstd)
        );
    }

    #[test]
    fn set_compression_does_not_require_mut() {
        let client = WebDavClient::new(BASE, None, None).unwrap();
        client.set_request_compression_mode(RequestCompressionMode::Disabled);
        assert_eq!(
            client.request_compression_mode(),
            RequestCompressionMode::Disabled
        );
    }

    #[test]
    fn test_normalize_etag_strips_double_quotes_strong() {
        assert_eq!(normalize_etag(r#""abc123""#), "abc123");
    }

    #[test]
    fn test_normalize_etag_strips_double_quotes_weak() {
        assert_eq!(normalize_etag(r#"W/"weak123""#), "W/weak123");
    }

    #[test]
    fn test_normalize_etag_bare_value_unchanged() {
        assert_eq!(normalize_etag("abc123"), "abc123");
    }

    #[test]
    fn test_normalize_etag_bare_weak_unchanged() {
        assert_eq!(normalize_etag("W/abc123"), "W/abc123");
    }

    #[test]
    fn test_normalize_etag_trims_whitespace() {
        assert_eq!(normalize_etag(r#"  "abc123"  "#), "abc123");
    }

    #[test]
    fn test_normalize_etag_empty_string() {
        assert_eq!(normalize_etag(""), "");
    }

    #[test]
    fn test_normalize_etag_only_quotes() {
        assert_eq!(normalize_etag(r#""""#), "");
    }

    #[test]
    fn test_normalize_etag_preserves_single_quotes_inside() {
        assert_eq!(normalize_etag(r#""ab'cd""#), "ab'cd");
    }

    #[test]
    fn test_normalize_sync_token_strips_double_quotes() {
        assert_eq!(normalize_sync_token(r#""token-123""#), "token-123");
    }

    #[test]
    fn test_normalize_sync_token_bare_unchanged() {
        assert_eq!(
            normalize_sync_token("http://example.com/sync/42"),
            "http://example.com/sync/42"
        );
    }

    #[test]
    fn test_normalize_sync_token_trims_whitespace() {
        assert_eq!(normalize_sync_token(r#"  "token"  "#), "token");
    }

    #[test]
    fn test_if_match_wildcard() {
        let val = if_match_header_value("*").unwrap();
        assert_eq!(val.to_str().unwrap(), "*");
    }

    #[test]
    fn test_if_match_quoted_strong_etag() {
        let val = if_match_header_value(r#""abc123""#).unwrap();
        assert_eq!(val.to_str().unwrap(), r#""abc123""#);
    }

    #[test]
    fn test_if_match_unquoted_gets_quoted() {
        let val = if_match_header_value("abc123").unwrap();
        assert_eq!(val.to_str().unwrap(), r#""abc123""#);
    }

    #[test]
    fn test_if_match_weak_quoted_rejected() {
        let err = if_match_header_value(r#"W/"abc""#).unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidEtag {
                reason: EtagReason::Weak,
                source: None
            }
        ));
    }

    #[test]
    fn test_if_match_weak_unquoted_rejected() {
        let err = if_match_header_value("W/abc").unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidEtag {
                reason: EtagReason::Weak,
                source: None
            }
        ));
    }

    #[test]
    fn test_if_match_trims_whitespace() {
        let val = if_match_header_value("  abc  ").unwrap();
        assert_eq!(val.to_str().unwrap(), r#""abc""#);
    }

    #[test]
    fn test_if_match_empty_rejected() {
        let err = if_match_header_value("").unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidEtag {
                reason: EtagReason::Empty,
                source: None
            }
        ));
    }

    #[test]
    fn test_if_match_whitespace_only_rejected() {
        let err = if_match_header_value("   ").unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidEtag {
                reason: EtagReason::Empty,
                source: None
            }
        ));
    }

    #[test]
    fn test_validate_opaque_tag_empty() {
        let err = validate_opaque_tag("").unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidEtag {
                reason: EtagReason::InvalidFormat,
                source: None
            }
        ));
    }

    #[test]
    fn test_validate_opaque_tag_contains_double_quote() {
        let err = validate_opaque_tag(r#"abc"def"#).unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidEtag {
                reason: EtagReason::InvalidFormat,
                source: None
            }
        ));
    }

    #[test]
    fn test_validate_opaque_tag_invalid_characters() {
        let err = validate_opaque_tag("abc\ndef").unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidEtag {
                reason: EtagReason::InvalidCharacters,
                source: None
            }
        ));
    }

    #[test]
    fn test_validate_opaque_tag_valid() {
        assert!(validate_opaque_tag("abc123").is_ok());
        assert!(validate_opaque_tag("W/abc").is_ok());
    }

    #[test]
    fn test_is_valid_entity_tag_strong_quoted() {
        assert!(is_valid_entity_tag(r#""abc""#));
    }

    #[test]
    fn test_is_valid_entity_tag_weak_quoted() {
        assert!(is_valid_entity_tag(r#"W/"abc""#));
    }

    #[test]
    fn test_is_valid_entity_tag_unquoted_is_invalid() {
        assert!(!is_valid_entity_tag("abc"));
    }

    #[test]
    fn test_is_valid_entity_tag_missing_closing_quote() {
        assert!(!is_valid_entity_tag(r#""abc"#));
    }

    #[test]
    fn test_is_valid_entity_tag_missing_opening_quote() {
        assert!(!is_valid_entity_tag(r#"abc""#));
    }

    #[test]
    fn test_is_valid_entity_tag_invalid_chars_inside_quotes() {
        assert!(!is_valid_entity_tag("\"ab\nc\""));
    }

    #[test]
    fn test_is_etag_character_allowed() {
        assert!(is_etag_character(b'!'));
        assert!(is_etag_character(b'#'));
        assert!(is_etag_character(b'~'));
        assert!(is_etag_character(b'A'));
        assert!(is_etag_character(b'0'));
        assert!(is_etag_character(0x80));
    }

    #[test]
    fn test_is_etag_character_disallowed() {
        assert!(!is_etag_character(b'"'));
        assert!(!is_etag_character(b' '));
        assert!(!is_etag_character(0x7F));
        assert!(!is_etag_character(b'\n'));
    }

    fn make_client(base: &str) -> WebDavClient {
        WebDavClient::new(base, None, None).unwrap()
    }

    #[test]
    fn handle_compression_outcome_415_retries_and_disables() {
        let client = make_client(BASE);
        let retry = client.handle_request_compression_outcome(
            Some(ContentEncoding::Gzip),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        );
        assert!(retry);
        assert_eq!(client.request_compression(), ContentEncoding::Identity);
    }

    #[test]
    fn handle_compression_outcome_501_retries_and_disables() {
        let client = make_client(BASE);
        let retry = client.handle_request_compression_outcome(
            Some(ContentEncoding::Br),
            StatusCode::NOT_IMPLEMENTED,
        );
        assert!(retry);
        assert_eq!(client.request_compression(), ContentEncoding::Identity);
    }

    #[test]
    fn handle_compression_outcome_400_retries_and_disables() {
        let client = make_client(BASE);
        let retry = client.handle_request_compression_outcome(
            Some(ContentEncoding::Zstd),
            StatusCode::BAD_REQUEST,
        );
        assert!(retry);
        assert_eq!(client.request_compression(), ContentEncoding::Identity);
    }

    #[test]
    fn handle_compression_outcome_200_caches_encoding() {
        let client = make_client(BASE);
        let retry =
            client.handle_request_compression_outcome(Some(ContentEncoding::Gzip), StatusCode::OK);
        assert!(!retry);
        assert_eq!(client.request_compression(), ContentEncoding::Gzip);
    }

    #[test]
    fn handle_compression_outcome_disabled_mode_no_retry() {
        let client = make_client(BASE);
        client.set_request_compression_mode(RequestCompressionMode::Disabled);
        let retry = client.handle_request_compression_outcome(
            Some(ContentEncoding::Gzip),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        );
        assert!(!retry);
    }

    #[test]
    fn handle_compression_outcome_none_attempted_no_retry() {
        let client = make_client(BASE);
        let retry = client.handle_request_compression_outcome(None, StatusCode::OK);
        assert!(!retry);
    }

    #[test]
    fn resolve_request_encoding_with_mode_disabled() {
        let client = make_client(BASE);
        let enc = client.resolve_request_encoding_with_mode(&RequestCompressionMode::Disabled);
        assert_eq!(enc, ContentEncoding::Identity);
    }

    #[test]
    fn resolve_request_encoding_with_mode_force() {
        let client = make_client(BASE);
        let enc = client.resolve_request_encoding_with_mode(&RequestCompressionMode::Force(
            ContentEncoding::Br,
        ));
        assert_eq!(enc, ContentEncoding::Br);
    }

    #[test]
    fn resolve_request_encoding_with_mode_auto_some() {
        let client = make_client(BASE);
        client.set_negotiated_encoding(Some(ContentEncoding::Zstd));
        let enc = client.resolve_request_encoding_with_mode(&RequestCompressionMode::Auto);
        assert_eq!(enc, ContentEncoding::Zstd);
    }

    #[test]
    fn resolve_request_encoding_with_mode_auto_none_uses_default() {
        let client = make_client(BASE);
        client.set_negotiated_encoding(None);
        let enc = client.resolve_request_encoding_with_mode(&RequestCompressionMode::Auto);
        assert_eq!(enc, ContentEncoding::Gzip);
    }

    #[test]
    fn normalize_decompressed_headers_empty_encodings_noop() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
        headers.insert(header::CONTENT_LENGTH, "100".parse().unwrap());
        normalize_decompressed_headers(&mut headers, &[], 42);
        assert_eq!(headers.get(header::CONTENT_ENCODING).unwrap(), "gzip");
        assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "100");
    }

    #[test]
    fn normalize_decompressed_headers_removes_encoding_sets_length() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
        normalize_decompressed_headers(&mut headers, &[ContentEncoding::Gzip], 42);
        assert!(headers.get(header::CONTENT_ENCODING).is_none());
        assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "42");
    }

    #[test]
    fn normalize_decompressed_headers_large_body_len_sets_length() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, "100".parse().unwrap());
        let huge = usize::MAX;
        normalize_decompressed_headers(&mut headers, &[ContentEncoding::Gzip], huge);
        assert!(headers.get(header::CONTENT_ENCODING).is_none());
        assert_eq!(
            headers.get(header::CONTENT_LENGTH).unwrap(),
            usize::MAX.to_string().as_str()
        );
    }

    #[test]
    fn build_uri_base_without_trailing_slash_and_relative_path() {
        let client = make_client("http://127.0.0.1:8080");
        let uri = client.build_uri("calendars/").unwrap();
        assert_eq!(uri.path(), "/calendars/");
    }

    #[test]
    fn build_uri_empty_combined_uses_base_path() {
        let client = make_client("http://127.0.0.1:8080/");
        let uri = client.build_uri("").unwrap();
        assert_eq!(uri.path(), "/");
    }

    #[test]
    fn build_uri_question_mark_is_part_of_the_path() {
        // Query strings are not part of the path contract: a `?` is a
        // resource-name character and must not change resource identity.
        let client = make_client("http://127.0.0.1:8080/");
        let uri = client.build_uri("?query").unwrap();
        assert_eq!(uri.path(), "/%3Fquery");
        assert!(uri.query().is_none());
    }

    #[test]
    fn build_uri_absolute_path_uses_as_is() {
        let client = make_client("http://127.0.0.1:8080/base/");
        let uri = client.build_uri("/calendars/").unwrap();
        assert_eq!(uri.path(), "/calendars/");
    }

    #[test]
    fn build_uri_absolute_url_parsed_directly() {
        let client = make_client("http://127.0.0.1:8080/base/");
        let uri = client.build_uri("https://other.example.com/foo").unwrap();
        assert_eq!(uri.path(), "/foo");
        assert_eq!(uri.host().unwrap(), "other.example.com");
    }

    #[test]
    fn build_uri_relative_path_appends_to_base() {
        let client = make_client("http://127.0.0.1:8080/base/");
        let uri = client.build_uri("calendars/").unwrap();
        assert_eq!(uri.path(), "/base/calendars/");
    }
}
