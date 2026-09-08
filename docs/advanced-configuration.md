# Advanced Configuration

For production use, use the builder pattern to configure auth, timeouts,
connection pool, TLS, proxy, and more:

### Basic auth + timeout + pool

```rust
fn main() -> fast_dav_rs::Result<()> {
use fast_dav_rs::CalDavClient;
use std::time::Duration;

let client = CalDavClient::builder("https://cal.example.com/dav/")
    .basic_auth("user", "pass")
    .timeout(Duration::from_secs(30))
    .user_agent("MyApp/1.0")
    .pool_max_idle_per_host(10)
    .build()?;
let _ = client;
Ok(())
}
```

### Bearer/OAuth 2.0 token

```rust
use fast_dav_rs::CalDavClient;
fn main() -> fast_dav_rs::Result<()> {
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .bearer_token("ya29.token...")
    .build()?;
let _ = client;
Ok(())
}
```

### Base-URL credentials are rejected

A base URL carrying `user:pass@` userinfo is rejected at build time with
`Error::InvalidConfig` — before any network I/O. Pass credentials via
`basic_auth(...)`, `bearer_token(...)`, or a `TokenProvider` instead. URLs
discovered through `.well-known` redirects (RFC 6764 §5) are likewise
returned without userinfo: redirect targets are server-controlled, so
discovery never echoes credentials back to the caller.

### Pluggable token provider (renewable OAuth2)

`token_provider(...)` is the third auth mode (mutually exclusive with
`basic_auth`/`bearer_token`, last-set wins): the client asks a
`TokenProvider` for the bearer token before each request, and when the
server rejects a token with `401` it refreshes **once** and retries. The
provided `OAuth2RefreshProvider` implements the generic RFC 6749 §6
refresh grant (pure HTTP — no browser flows, no provider presets; obtaining
the initial refresh token is the caller's job):

```rust
fn main() -> fast_dav_rs::Result<()> {
use std::sync::Arc;
use fast_dav_rs::webdav::{OAuth2RefreshProvider, WebDavClient};

let provider = OAuth2RefreshProvider::new(
    "https://auth.example.com/oauth2/token",
    "my-client-id",
    "my-client-secret",
    "the-long-lived-refresh-token",
)?;

let client = WebDavClient::builder("https://dav.example.com/")
    .token_provider(Arc::new(provider))
    .build()?;
let _ = client;
Ok(())
}
```

Renewal is transparent and single-flight: tokens are cached until
`expires_in` passes or a `401` arrives, and concurrent requests share one
in-flight refresh instead of stampeding the token endpoint. Refresh
failures surface as `Error::TokenRefresh` (`Rejected` / `MalformedResponse`
/ `Transport`); a `401` after one refresh is returned as-is. Clones of the
client share the provider's cache. Any custom token source works by
implementing the `TokenProvider` trait (see its docs for the exact
401-renewal contract).

> **Security**: tokens travel as `Authorization: Bearer` headers on every
> request — always use `https://` outside isolated test environments.
> Tokens never appear in the crate's `Debug` output, error messages, or
> tracing events.

### Proxy + custom CA for debugging

Route traffic through a debugging proxy (Proxyman/Charles/mitmproxy)
and trust its MITM CA — works on Android non-rooted and iOS/macOS alike:

```rust,no_run
use fast_dav_rs::CalDavClient;
fn main() -> fast_dav_rs::Result<()> {
let proxy_uri: hyper::Uri = "http://127.0.0.1:9090".parse().expect("valid proxy URI");
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .basic_auth("user", "pass")
    .proxy(proxy_uri)
    .proxy_basic_auth("proxyuser", "proxypass")
    .extra_root_certs_pem(vec![std::fs::read("/path/proxyman-ca.pem")?])
    .build()?;
let _ = client;
Ok(())
}
```

### Force HTTP/1.1

For servers or proxies that misbehave with HTTP/2:

```rust
use fast_dav_rs::CalDavClient;
fn main() -> fast_dav_rs::Result<()> {
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .force_http1(true)
    .build()?;
let _ = client;
Ok(())
}
```

> HTTP/2 is negotiated over **TLS via ALPN** on `https://` URLs only. Cleartext
> `http://` connections always use HTTP/1.1 (h2c is not attempted).

### Custom Hyper client injection

Bring your own hyper client — for custom transports, wiremock-style test
harnesses, or tailored TLS/pool settings. The injected client is used **as-is**:
the builder skips its own transport construction, so `force_http1`, pool,
TLS, and proxy options are **not** applied (the caller owns the transport).
Request-level options (auth, timeout, compression, redirects, `Prefer`,
retries) still apply:

```rust
use fast_dav_rs::CalDavClient;
fn main() -> fast_dav_rs::Result<()> {
use fast_dav_rs::common::http::MaybeProxied;
use fast_dav_rs::webdav::{HyperClient, WebDavClient};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;

let mut http = HttpConnector::new();
http.enforce_http(false);
let https = HttpsConnectorBuilder::new()
    .with_webpki_roots()
    .https_or_http()
    .enable_http1()
    .enable_http2()
    .wrap_connector(MaybeProxied::direct(http));
let hyper_client: HyperClient = Client::builder(TokioExecutor::new())
    .pool_max_idle_per_host(8)
    .build(https);

let client = CalDavClient::builder("https://cal.example.com/dav/")
    .with_hyper_client(hyper_client)
    .build()?;
let _ = client;
Ok(())
}
```

The method is available on `WebDavClientBuilder`, `CalDavClientBuilder`, and
`CardDavClientBuilder`.

### Redirect following

HTTP redirects (301/302/303/307/308) are followed automatically in `send`/`send_stream`,
up to a configurable limit. On 303 the request is re-sent as `GET` without a body, and
when a redirect crosses origins (scheme, host, or port change) the `Authorization`,
`Cookie`, `If-Match`, and `If-None-Match` headers are stripped for the remainder of the
chain. An `https`→`http` downgrade is never followed (RFC 6764 §6 is TLS-first): the 3xx
response is returned as-is so the caller can observe it. Exceeding the limit fails
with `Error::TooManyRedirects`:

```rust
use fast_dav_rs::CalDavClient;
fn main() -> fast_dav_rs::Result<()> {
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .follow_redirects(true) // default
    .max_redirects(5)       // default
    .build()?;
let _ = client;
Ok(())
}
```

### Auto-discovery (RFC 6764)

`discover_caldav` and `discover_carddav` (free functions taking `&WebDavClient`) locate the
service "context path" for a base URL per RFC 6764 §5: a `PROPFIND` with `Depth: 0` and a
`DAV:current-user-principal` body is sent to `{base}/.well-known/caldav` (or `/carddav`).
Redirects are followed by the client's redirect pipeline when `follow_redirects` is
enabled (the builder default — RFC 6764 §5 requires clients to handle `.well-known`
redirects), so the **final** request URL is the discovered service URL. A `404` (or a
success answered directly on the `.well-known` URI) returns the base URL unchanged as a
documented fallback; any other non-success status fails with `Error::UnexpectedStatus`,
except a 3xx that could not be followed (redirect following disabled, unresolvable
`Location`, or an https→http downgrade), which fails with a descriptive error. Client
credentials are attached to the probe and stripped
automatically on cross-origin redirect hops, and the discovered service URL is returned
without userinfo (redirect targets are server-controlled; see "Base-URL credentials are
rejected"). DNS SRV record lookup (RFC 6764 §3) is not
implemented:

```rust
use fast_dav_rs::{WebDavClient, discover_caldav};

async fn example() -> fast_dav_rs::Result<()> {
let client = WebDavClient::builder("https://dav.example.com/")
    .basic_auth("user", "pass")
    .build()?;
let service_url = discover_caldav(&client).await?;
Ok(())
}
```

### Retry & backoff

Transient failures are retried automatically in `send`/`send_stream` once you opt in with
`max_retries` (default **0** — no retry, each request is sent exactly once). Retries apply
to `429`, `503`, and `504` responses: a `429` honors the server's `Retry-After` header
(integer seconds or HTTP-date; absent → exponential backoff), while `503`/`504` always use
an exponential backoff (base 2, initial ~250 ms, doubling per attempt, capped at ~8 s) with
±25 % jitter. Only idempotent methods (`GET`, `HEAD`, `OPTIONS`, `PROPFIND`, `REPORT`) are
retried by default; `retry_all(true)` extends retrying to every method (`PUT`, `POST`,
`DELETE`, `MKCOL`, `COPY`, `MOVE`, `LOCK`, …). When retries are exhausted, the **last
response is returned as-is** — callers see the real status through the existing error
handling. The retry budget counts every HTTP attempt across the whole redirect chain
(total attempts = `1 + max_retries`), and each attempt — retries included — runs under the
same per-request timeout:

```rust
use fast_dav_rs::CalDavClient;
fn main() -> fast_dav_rs::Result<()> {
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .max_retries(3)     // default 0 — no retry
    .retry_all(false)   // default — only idempotent methods are retried
    .build()?;
let _ = client;
Ok(())
}
```

### Prefer header

The `Prefer` header (RFC 7240) can be set client-wide and is then sent on **every**
request. `put_if_match_prefer` sends a conditional `PUT` with
`Prefer: return=representation` so servers that honor it include the stored
representation (typically with the new `ETag`) in the response. Servers may ignore
preferences — check the `Preference-Applied` response header with
`preference_applied_from_headers` to see whether one was actually applied. Other
preferences (`wait`, `handling`, …) and per-request overrides can be sent manually via
the `HeaderMap` accepted by `send`/`send_stream` (an explicit per-request `Prefer`
header wins over the builder default):

```rust
use fast_dav_rs::CalDavClient;
fn main() -> fast_dav_rs::Result<()> {
use fast_dav_rs::webdav::Prefer;

let client = CalDavClient::builder("https://cal.example.com/dav/")
    .prefer(Some(Prefer::Minimal)) // default: none
    .build()?;
let _ = client;
Ok(())
}
```

### Conditional requests (If-Match)

`put_if_match`, `put_if_match_prefer`, and `delete_if_match` send `If-Match`
guarded requests using **RFC 9110 strong comparison**. Quoted strong ETags are
sent as-is; bare ETags (as returned by some servers) are quoted automatically.
Weak entity-tags (`W/"abc"`) are rejected **client-side before any network
I/O** with `Error::InvalidEtag` and `EtagReason::Weak`: under strong comparison
a weak validator never matches, so a server would always answer `412
Precondition Failed`. Weak ETags remain accepted everywhere they are purely
informational (`etag_from_headers`, `normalize_etag`).

### iCalendar validation (CalDAV)

CalDAV `PUT` bodies (`put`, `put_if_match`, `put_if_none_match`) are validated
client-side **before any network I/O**. The default `ValidationLevel::Structural`
checks that the body is valid UTF-8, starts with `BEGIN:VCALENDAR`, ends with
`END:VCALENDAR`, declares `VERSION:2.0` and a `PRODID`, and has balanced
`BEGIN`/`END` component pairs. On a body that declares a `VERSION`, the wire
`Content-Type` gains a matching `version` parameter
(`text/calendar; charset=utf-8; version=2.0`). Invalid bodies fail with
`Error::InvalidICalendar` (carrying an `ICalendarViolation`) without a request
being sent:

```rust
fn main() -> fast_dav_rs::Result<()> {
use fast_dav_rs::caldav::ValidationLevel;
use fast_dav_rs::CalDavClient;

let client = CalDavClient::builder("https://cal.example.com/dav/")
    .validation_level(ValidationLevel::Strict) // also require UID in every VEVENT/VTODO
    // .validation_level(ValidationLevel::None) // pre-validation behavior
    .build()?;
let _ = client;
Ok(())
}
```

`fast_dav_rs::caldav::validate_icalendar(&body)` runs all seven structural
checks directly. CardDAV (vCard) requests are never validated as iCalendar.

