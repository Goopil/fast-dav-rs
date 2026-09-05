# fast-dav-rs

[![Crates.io](https://img.shields.io/crates/v/fast-dav-rs.svg)](https://crates.io/crates/fast-dav-rs)
[![Documentation](https://docs.rs/fast-dav-rs/badge.svg)](https://docs.rs/fast-dav-rs)
[![CI](https://github.com/Goopil/fast-dav-rs/workflows/CI/badge.svg)](https://github.com/Goopil/fast-dav-rs/actions)
[![dependency status](https://deps.rs/repo/github/goopil/fast-dav-rs/status.svg)](https://deps.rs/repo/github/goopil/fast-dav-rs)
[![License: LGPL v3](https://img.shields.io/badge/License-LGPL%20v3-blue.svg)](https://www.gnu.org/licenses/lgpl-3.0)
[![Coverage](https://codecov.io/gh/Goopil/fast-dav-rs/graph/badge.svg)](https://codecov.io/gh/Goopil/fast-dav-rs)
[![Quality Gate](https://sonarcloud.io/api/project_badges/measure?project=Goopil_fast-dav-rs&metric=alert_status)](https://sonarcloud.io/dashboard?id=Goopil_fast-dav-rs)

fast-dav-rs is a high-performance asynchronous CalDAV/CardDAV client for Rust. It blends hyper 1.x, tokio,
rustls, and streaming XML tooling so your services can discover calendars, manage events, sync addressbooks,
and keep remote DAV stores in sync without re-implementing the protocol by hand.

## Why This Library?

- CalDAV and CardDAV discovery, queries, and sync with a consistent API surface.
- HTTP/2, connection pooling, and configurable timeouts built on hyper and tokio.
- Automatic response decompression plus optional request compression (br, zstd, gzip).
- Streaming XML parsing for large multistatus responses.
- Safe conditional methods and ETag helpers for update/delete workflows.
- Batch operations with bounded concurrency and predictable ordering.

## Stability & Maturity

This library focuses on correctness and predictable behavior across CalDAV and CardDAV servers.

- Core discovery, CRUD, and query flows are covered by unit and e2e tests.
- Streaming parsing and sync are stable, but server quirks still vary.
- Compatibility feedback from real deployments is welcome.

## Roadmap

- Documentation parity across CalDAV and CardDAV, with more recipes and examples.
- Expanded server compatibility notes and fixtures.
- Incremental improvements to error reporting and diagnostics.

## Governance & Project Direction

The project prioritizes correctness, performance, and a low-ceremony API. New features are welcome
when they improve protocol compliance or compatibility without adding unnecessary abstraction.

## Versioning & Backward Compatibility

This project follows Semantic Versioning. Patch releases fix bugs, minor releases add compatible
features, and major releases introduce breaking changes when needed.

## Table of Contents

- [Why This Library?](#why-this-library)
- [Stability & Maturity](#stability--maturity)
- [Roadmap](#roadmap)
- [Governance & Project Direction](#governance--project-direction)
- [Versioning & Backward Compatibility](#versioning--backward-compatibility)
- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Error Handling & Migration](#error-handling--migration)
- [Configuration](#configuration)
- [Security](#security)
- [Observability](#observability)
- [Usage Examples](#usage-examples)
- [Streaming & Sync](#streaming--sync)
- [Batch Operations](#batch-operations)
- [Runnable Examples](#runnable-examples)
- [Testing](#testing)
- [End-to-End Testing](#end-to-end-testing)
- [Limitations & Non-Goals](#limitations--non-goals)
- [When NOT to Use This Library](#when-not-to-use-this-library)
- [Performance Tips](#performance-tips)
- [Contributing](#contributing)
- [Credits](#credits)
- [License](#license)
- [Support](#support)

## Features

### Core Features

- CalDAV calendar discovery, queries, and event CRUD.
- CalDAV `free-busy-query` reports and server-side recurrence expansion (`expand`, RFC 4791 §9.6-9.7).
- CalDAV scheduling (RFC 6638): schedule endpoint discovery, outbox `POST`, schedule-inbox listing, and `If-Schedule-Tag-Match` conditional writes.
- CalDAV `calendar-timezone` read + write (RFC 4791 §5.2.2): per-calendar read and via `CalendarInfo.timezone`; `set_calendar_timezone` stores/removes the property via `PROPPATCH`.
- CalDAV managed attachments (RFC 8607, sent in the non-IETF CalendarServer collection-targeted form): `post_managed_attachment` stores an attachment via `?action=attachment-add` and returns its href + `Cal-Managed-ID`; the streaming parser reads the `managed-ids` property into `DavItem.managed_ids`.
- Client-side iCalendar validation for CalDAV writes (`ValidationLevel`, default `Structural`).
- CardDAV addressbook discovery, queries, and contact CRUD.
- HTTP/2 with connection pooling and automatic response decompression.
- Streaming XML parsing for multistatus responses.
- ETag helpers and conditional methods for safe updates.
- Typed current-user privileges (`current_user_privileges`, RFC 3744 §5.4).

### Advanced Features

- WebDAV locking (RFC 4918 class 2): `LOCK`/`UNLOCK`, lock refresh via the `If` header,
  and `lockdiscovery` parsing (`LockInfo`, `LockScope`).
- WebDAV-Sync (RFC 6578) for incremental sync.
- Bounded parallelism for batch PROPFIND/REPORT operations.
- Automatic request compression negotiation (br, zstd, gzip) with overrides.
- Streaming send APIs for custom workflows.
- RFC 6764 `.well-known` service discovery (`discover_caldav`/`discover_carddav`).
- Retry with exponential backoff for transient failures (429/503/504) with `Retry-After` support.
- Optional `tracing` instrumentation behind the `tracing` feature (zero-cost when disabled).

## Requirements

- Rust 2024 edition.
- tokio runtime with the `macros`, `rt-multi-thread`, and `time` features.
- Optional: Docker and Docker Compose for e2e tests.

## Installation

```bash
cargo add fast-dav-rs
```

## Quick Start

### CalDAV discovery

```rust
use fast_dav_rs::{CalDavClient, Error, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new(
        "https://caldav.example.com/users/alice/",
        Some("alice"),
        Some("hunter2"),
    )?;

    let principal = client
        .discover_current_user_principal()
        .await?
        .ok_or_else(|| Error::other("no principal returned"))?;

    let homes = client.discover_calendar_home_set(&principal).await?;
    let home = homes.first().expect("missing calendar-home-set");

    for calendar in client.list_calendars(home).await? {
        println!("Calendar: {:?}", calendar.displayname);
    }

    Ok(())
}
```

### CardDAV discovery

```rust
use fast_dav_rs::{CardDavClient, Error, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = CardDavClient::new(
        "https://carddav.example.com/users/alice/",
        Some("alice"),
        Some("hunter2"),
    )?;

    let principal = client
        .discover_current_user_principal()
        .await?
        .ok_or_else(|| Error::other("no principal returned"))?;

    let homes = client.discover_addressbook_home_set(&principal).await?;
    let home = homes.first().expect("missing addressbook-home-set");

    for book in client.list_addressbooks(home).await? {
        println!("Addressbook: {:?}", book.displayname);
    }

    Ok(())
}
```

### Discovery order and principal-404 hardening

`discover_current_user_principal` probes the **authenticated root URL
directly** — a single credentialed `PROPFIND`, and the primary discovery
step. The RFC 6764 `.well-known` probes (`discover_caldav` /
`discover_carddav`) are the fallback for servers that host DAV under a
context path; some providers answer `.well-known` unreliably.

If authentication succeeds but the principal `PROPFIND` returns `404` (the
server never answers `401`), discovery fails with `Error::PrincipalNotFound`.
On some providers this is the signature of a wrong username form — e.g. an
email address where the provider expects an internal short account ID:

```rust
use fast_dav_rs::Error;

match client.discover_current_user_principal().await {
    Err(Error::PrincipalNotFound { url, .. }) => {
        eprintln!(
            "auth OK but no principal at {url}: retry with the provider's \
             canonical account ID"
        );
    }
    other => {
        other?;
    }
}
```

The `OPTIONS` `DAV:` compliance header (RFC 4918 §10.1) is available as a
typed view: `WebDavClient::capabilities` parses the header into
`DavCapabilities`, and `DavCapabilities::compliance()` maps it to
`DavCompliance` values (`One`, `Two` (locking), `Three`, `AccessControl`,
`CalendarAccess`, `Addressbook`, `ExtendedMkcol`, `CalendarProxy`), with
`calendarserver-*` vendor tokens and unknown extensions passing through as
`DavCompliance::Other`.

### Current-user privileges (RFC 3744 §5.4)

`current_user_privileges` (all clients) `PROPFIND`s the
`current-user-privilege-set` property and returns the typed
`Privilege` set the authenticated user holds on a path. The set is
advisory — servers may grant privileges through inherited or aggregated
ACEs, and an absent privilege does not prove an operation will be denied.
Unrecognized privilege elements surface as `Privilege::Other(name)`:

```rust
use fast_dav_rs::webdav::Privilege;

let privileges = client.current_user_privileges("calendars/alice/").await?;
if privileges.contains(&Privilege::WriteContent) {
    // safe to offer editing in the UI
}
// `#[non_exhaustive]`: always keep a wildcard arm when matching.
for privilege in &privileges {
    match privilege {
        Privilege::Read => println!("read"),
        Privilege::Other(name) => println!("server-specific: {name}"),
        _ => println!("other"),
    }
}
```

## Error Handling & Migration

Public APIs return `fast_dav_rs::Result<T>`, whose error type is the public
`fast_dav_rs::Error` enum. The enum is `#[non_exhaustive]`, so always include a
wildcard arm (`_ => …`) when matching — new variants may be added in future
releases without a breaking change.

Applications can match its variants to make decisions without inspecting error
messages or downcasting an opaque error:

```rust
use fast_dav_rs::Error;

fn is_retryable(error: &Error) -> bool {
    matches!(error, Error::Timeout { .. } | Error::Transport(_))
}
```

### Error variants

| Variant                | When it occurs                                                        |
|------------------------|-----------------------------------------------------------------------|
| `InvalidUrl`           | A base URL or resolved request URI is invalid                         |
| `InvalidInput`         | Catch-all for a caller-provided value that failed validation           |
| `InvalidEtag`          | An ETag value failed validation                                        |
| `InvalidComponentName` | A calendar/addressbook component name failed validation               |
| `InvalidDateTime`      | A date-time value did not match the expected iCalendar UTC format     |
| `InvalidICalendar`     | An iCalendar body failed structural validation (CalDAV `PUT`)          |
| `InvalidConfig`        | A builder configuration value is invalid                               |
| `InvalidHeader`        | An HTTP header value could not be constructed                         |
| `InvalidMethod`        | An HTTP method was invalid                                            |
| `Http`                 | Building an HTTP request failed                                       |
| `Hyper`                | A low-level Hyper connection or body operation failed                 |
| `Connection`           | The TCP/TLS handshake failed (DNS, refused, TLS)                      |
| `Transport`            | A request was sent but the response stream broke                      |
| `UnexpectedStatus`     | The server returned an unexpected HTTP status code                    |
| `UnexpectedStatusWithDav` | Unexpected status with a `<D:error>` body (e.g. `423` + `no-conflicting-lock`) |
| `PrincipalNotFound`    | Authentication succeeded but `current-user-principal` PROPFIND returned 404 — on some providers the signature of a wrong username form (e.g. email instead of the account ID) |
| `Timeout`              | An operation exceeded its configured time limit                      |
| `BodyTooLarge`         | A decompressed response body exceeded the 256 MiB limit              |
| `Xml`                  | Parsing or decoding XML failed                                        |
| `XmlStructure`         | The XML element hierarchy is malformed or incomplete                  |
| `XmlEscape`            | Unescaping XML entity references failed                              |
| `XmlAttribute`         | Parsing an XML attribute failed                                       |
| `Io`                   | An I/O operation failed                                               |
| `Utf8`                 | Decoding UTF-8 text failed                                             |
| `TlsRustls`             | A rustls TLS operation failed                                         |
| `Tls`                  | TLS, certificate, or PEM parsing failed                               |
| `Other`                | User callback error or error that doesn't fit another variant         |

The `Operation` enum identifies which DAV operation produced an
`UnexpectedStatus` (e.g. `PropfindCollections`, `ReportCalendarQuery`,
`PropfindScheduleEndpoints`, `PostSchedule`, `ScheduleInbox`,
`PostManagedAttachment`, `Lock`, `Unlock`). The
`EtagReason` enum describes why an ETag was rejected (`Empty`,
`InvalidFormat`, `InvalidCharacters`, `InvalidHeaderValue`, `Weak`).

> **Note:** TLS errors may appear as either `TlsRustls` (automatic
> `rustls::Error` propagation via `?`) or `Tls` (manually wrapped with
> context, e.g. PEM parsing). Consumers checking for TLS errors should
> match both variants.

### Migrating from `anyhow`

Earlier releases returned `anyhow::Error`. Replace library-facing
`anyhow::Result<T>` signatures with `fast_dav_rs::Result<T>` when you want to
preserve and match the typed error:

```rust
use fast_dav_rs::{CalDavClient, Error, Result};

async fn discover_principal(client: &CalDavClient) -> Result<String> {
    client
        .discover_current_user_principal()
        .await?
        .ok_or_else(|| Error::other("no principal returned"))
}
```

Applications that use `anyhow` at their own boundary can continue to propagate
library errors with `?`; `fast_dav_rs::Error` implements `std::error::Error`:

```rust
use anyhow::Result;
use fast_dav_rs::CalDavClient;

async fn synchronize(client: &CalDavClient) -> Result<()> {
    client.discover_current_user_principal().await?;
    Ok(())
}
```

Replace calls specific to `anyhow::Error`, such as `downcast_ref`, `context`, or
`with_context`, with pattern matching on variants such as `Error::Timeout`,
`Error::UnexpectedStatus`, `Error::InvalidInput`, and `Error::Transport`. Error
messages remain intended for diagnostics; use variants and their fields for
programmatic handling.

### Distinguishing connection vs transport errors

`Error::Connection` is returned when the TCP/TLS handshake itself fails (DNS
resolution, connection refused, TLS error), while `Error::Transport` covers
failures during an already-established connection (read/write abort, body
decode error). This lets retry logic target only transient connection issues:

```rust
use fast_dav_rs::Error;

fn should_retry(error: &Error) -> bool {
    match error {
        // The server was unreachable; a retry may reach a different node.
        Error::Connection(_) | Error::Timeout { .. } => true,
        // The request was sent but the response stream broke mid-flight.
        // Retrying is only safe for idempotent methods (GET, PUT with If-Match).
        Error::Transport(_) => false,
        // The server explicitly rejected the request.
        Error::UnexpectedStatus { status, .. } => {
            status.is_server_error()
        }
        _ => false,
    }
}
```

For errors originating in user callbacks or when wrapping an error that does
not fit a specific variant, use [`Error::other`] for a standalone message or
[`Error::with_source`] to preserve the underlying cause in the error chain:

```rust
use fast_dav_rs::Error;
use std::error::Error as _;

let standalone = Error::other("no principal returned");
let with_cause = Error::with_source("callback failed", std::io::Error::other("disk full"));
assert!(with_cause.source().is_some());
```

### Custom errors in streaming callbacks

When using streaming APIs with a visitor callback, return `Error::other` for
application-level failures and `Error::with_source` to wrap an underlying cause:

```rust,no_run
use fast_dav_rs::{CalDavClient, Depth, Error, Result};
use fast_dav_rs::caldav::{parse_multistatus_stream_visit, DavItem};

async fn sync_calendar(client: &CalDavClient, path: &str) -> Result<()> {
    let resp = client.report(path, Depth::One, "<body/>").await?;

    parse_multistatus_stream_visit(resp.into_body(), &[], |item: DavItem| {
        // Save to a database; wrap the DB error with context.
        save_to_db(&item).map_err(|e| {
            Error::with_source(format!("failed to save {}", item.href), e)
        })
    }).await?;

    Ok(())
}

fn save_to_db(_item: &DavItem) -> std::result::Result<(), DbError> {
    Ok(())
}

#[derive(Debug)]
struct DbError;
impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "database error")
    }
}
impl std::error::Error for DbError {}
```

### Migrating from `anyhow` with `map_err`

If your codebase uses `anyhow::Context` to add context to library errors,
replace `.context("...")` with `.map_err(|e| Error::with_source("...", e))`:

```rust
// Before (anyhow)
use anyhow::Context;
let principal = client
    .discover_current_user_principal()
    .await
    .context("discovery failed")?
    .ok_or_else(|| anyhow::anyhow!("no principal"))?;

// After (typed errors)
use fast_dav_rs::Error;
let principal = client
    .discover_current_user_principal()
    .await
    .map_err(|e| Error::with_source("discovery failed", e))?
    .ok_or_else(|| Error::other("no principal"))?;
```

### Complete migration example

A compilable, step-by-step migration example lives in
[`examples/migration.rs`](examples/migration.rs). It covers defining a typed
`Error` enum with `#[from]` and `#[error(...)]`, using `?` with automatic
conversions, replacing `anyhow!()` and `.context()`, and pattern matching on
variants for programmatic error handling.

Run it:

```sh
cargo run --example migration
```

Key patterns at a glance:

```rust
// 1. Define typed variants — #[from] for simple variants, #[source] for rich ones
//
// #[from] generates a From<E> impl so `?` works automatically — but only
// for newtype/tuple variants with NO extra fields:
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("parse failed: {0}")]
    Parse(#[from] ParseIntError),  // ? converts automatically

    // For struct variants with extra context, use #[source] + .map_err():
    #[error("invalid port `{raw}`: {source}")]
    InvalidPort { raw: String, #[source] source: ParseIntError },

    #[error("port out of range: {0}")]
    OutOfRange(u16),
}

// 2. Use ? for #[from] variants; .map_err() for #[source] variants
fn parse_port(raw: &str) -> Result<u16, AppError> {
    let port: u16 = raw.parse()
        .map_err(|source| AppError::InvalidPort { raw: raw.to_owned(), source })?;
    Ok(port)
}

// 3. Match on variants — the payoff over anyhow
match parse_port("abc") {
    Ok(port) => println!("port: {port}"),
    Err(AppError::InvalidPort { raw, .. }) => eprintln!("bad input: {raw}"),
    Err(AppError::OutOfRange(p)) => eprintln!("port {p} is reserved"),
}
```

### Removed legacy module paths

The former top-level modules (`client`, `streaming`, `types`,
`compression`) — previously available behind the `legacy` Cargo feature —
have been **removed**. Use the canonical paths:

```rust
use fast_dav_rs::caldav::client::CalDavClient;
use fast_dav_rs::caldav::streaming::parse_multistatus_stream;
```

## Configuration

### Request compression

```rust
use fast_dav_rs::{CalDavClient, ContentEncoding};
use fast_dav_rs::webdav::RequestCompressionMode;

let mut client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
client.set_request_compression_mode(RequestCompressionMode::Force(ContentEncoding::Gzip));
client.set_request_compression_mode(RequestCompressionMode::Auto);
client.set_request_compression_mode(RequestCompressionMode::Disabled);
```

In `Auto` mode the client sends one extra compressed `PROPFIND` probe per client
instance until the server's answer is cached (clones share the cache). Short-lived
clients — e.g. one built per request in serverless setups — pay that probe every
time; prefer reusing a client, or pin `Disabled`/`Force` to skip the probe. A
transient probe failure is not cached: the current request proceeds uncompressed
and the next request re-probes. Re-selecting `Auto` with
`set_request_compression_mode` resets the cached answer.

### Per-request timeouts

The low-level `send` and `send_stream` methods accept an optional `per_req_timeout: Option<Duration>`
so you can override the default timeout for specific requests.

### Batch concurrency

`propfind_many` and `report_many` accept a `max_concurrency` parameter to bound the number of in-flight
requests while preserving input order in the result list.

`CalDavClient::calendar_multiget_many` applies the same machinery to `calendar-multiget`: the href
list is chunked into `batch_size` slices, one REPORT is issued per chunk with at most
`max_concurrency` in flight, and results come back as `Vec<BatchItem<CalendarObject>>` ordered by
chunk. A failed chunk is a single error `BatchItem`; sibling chunks are unaffected. Every
`BatchItem` carries the request hrefs of its batch in `hrefs`, so a failed chunk is attributable
to the hrefs to re-fetch. Multiget REPORTs are sent with `Depth: 0` (RFC 4791 §7.9, RFC 6352
§8.7). Pick `batch_size` (e.g. 100 hrefs per REPORT) and `max_concurrency` (e.g. 4) to match your
server's limits.

`CardDavClient::addressbook_multiget_many` mirrors those semantics for `addressbook-multiget`
and returns `Vec<BatchItem<AddressObject>>` (no `expand` parameter). Both batched multigets share
one engine, so they behave identically apart from the object type.

#### `missing_hrefs` reconciliation

Every `BatchItem` from a batched multiget also carries `missing_hrefs`: the requested hrefs the
server did not answer with a `<D:response>` element (exact href string comparison — a compliant
server echoes every requested href, possibly with an error status, RFC 4791 §9.6.1 / RFC 6352
§8.7). A non-empty value signals a non-compliant server; the answered objects are still
delivered. `missing_hrefs` is empty for non-multiget batch operations (`propfind_many`,
`report_many`) and for batches that failed as a whole — their `hrefs` already name everything to
re-fetch.

#### Empty hrefs are dropped before chunking

Both batched multigets filter empty hrefs out of the input **before** chunking: they never reach
a REPORT and are not recorded in any `BatchItem::hrefs`. An input with no non-empty href yields
`Ok(Vec::new())` without any network I/O.

Note on exact comparison: some servers percent-encode characters such as `@` in the hrefs they
echo (`a@b.ics` comes back `a%40b.ics`). If you construct hrefs yourself instead of using the
hrefs the server published (e.g. from `sync_collection` or PROPFIND responses), compare against
that server's echo behavior — `missing_hrefs` uses exact string matching.

## Advanced Configuration

For production use, use the builder pattern to configure auth, timeouts,
connection pool, TLS, proxy, and more:

### Basic auth + timeout + pool

```rust
use fast_dav_rs::CalDavClient;
use std::time::Duration;

let client = CalDavClient::builder("https://cal.example.com/dav/")
    .basic_auth("user", "pass")
    .timeout(Duration::from_secs(30))
    .user_agent("MyApp/1.0")
    .pool_max_idle_per_host(10)
    .build()?;
```

### Bearer/OAuth 2.0 token

```rust
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .bearer_token("ya29.token...")
    .build()?;
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

```rust
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .basic_auth("user", "pass")
    .proxy("http://127.0.0.1:9090")
    .proxy_basic_auth("proxyuser", "proxypass")
    .extra_root_certs_pem(vec![std::fs::read("/path/proxyman-ca.pem")?])
    .build()?;
```

### Force HTTP/1.1

For servers or proxies that misbehave with HTTP/2:

```rust
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .force_http1(true)
    .build()?;
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
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .follow_redirects(true) // default
    .max_redirects(5)       // default
    .build()?;
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

# async fn example() -> fast_dav_rs::Result<()> {
let client = WebDavClient::builder("https://dav.example.com/")
    .basic_auth("user", "pass")
    .build()?;
let service_url = discover_caldav(&client).await?;
# Ok(())
# }
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
let client = CalDavClient::builder("https://cal.example.com/dav/")
    .max_retries(3)     // default 0 — no retry
    .retry_all(false)   // default — only idempotent methods are retried
    .build()?;
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
use fast_dav_rs::webdav::Prefer;

let client = CalDavClient::builder("https://cal.example.com/dav/")
    .prefer(Some(Prefer::Minimal)) // default: none
    .build()?;
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
use fast_dav_rs::caldav::ValidationLevel;
use fast_dav_rs::CalDavClient;

let client = CalDavClient::builder("https://cal.example.com/dav/")
    .validation_level(ValidationLevel::Strict) // also require UID in every VEVENT/VTODO
    // .validation_level(ValidationLevel::None) // pre-validation behavior
    .build()?;
```

`fast_dav_rs::caldav::validate_icalendar(&body)` runs all seven structural
checks directly. CardDAV (vCard) requests are never validated as iCalendar.

## Security

Basic credentials are sent as an `Authorization: Basic` header on every request. Base64 is an
encoding, not encryption: over plain `http://` your username and password travel effectively in
cleartext and can be read by anyone on the network path. The connector intentionally accepts both
`http://` and `https://` (plain HTTP is convenient for isolated test environments such as the
bundled Docker setup), so the library does not reject `http://` at runtime — **always use
`https://` outside isolated test environments**.

The same applies to Bearer tokens (static or resolved through a
`TokenProvider`): they are sent as an `Authorization: Bearer` header on every request and must
never travel over plain `http://` in production. Token material — access tokens, refresh tokens,
client secrets — never appears in the crate's `Debug` output, error messages, or tracing events;
errors about failed token refreshes carry only a typed reason and an HTTP status.

## Observability

The client optionally emits structured diagnostics through the [`tracing`](https://crates.io/crates/tracing)
ecosystem standard. Enable it with a feature flag:

```bash
cargo add fast-dav-rs --features tracing
```

Everything (WebDAV, CalDAV, and CardDAV) is instrumented in the shared request pipeline, so one
feature flag covers all three clients:

| Level | Events |
|---|---|
| `DEBUG` | Request start (`method`, `uri`) and finish (`method`, `uri`, `status`, `duration_us`) per attempt; each redirect hop (source, target, status); transient retries (status, `delay_ms`, attempt number); exhausted retry budget; per-request timeout hit (`limit_ms`); compression-probe outcome and negotiated encoding |
| `TRACE` | Decompressed response body size (`bytes`) after the aggregated `send` path |

The feature is **disabled by default and zero-cost when off**: no `tracing` dependency is pulled
in and no instrumentation code is compiled into your binary. When enabled, no subscriber is
installed for you — plug in your own (`tracing-subscriber`'s `fmt()` layer, OpenTelemetry, …):

```rust,ignore
tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();
```

## Usage Examples

### CalDAV event CRUD

```rust
use fast_dav_rs::{CalDavClient, Result};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    let calendar_path = "calendars/alice/work/";

    let event_path = format!("{calendar_path}kickoff.ics");
    let create = Bytes::from("BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:kickoff\nEND:VEVENT\nEND:VCALENDAR\n");
    client.put_if_none_match(&event_path, create).await?;

    let events = client
        .calendar_query_timerange(calendar_path, "VEVENT", None, None, true, None)
        .await?;

    if let Some(event) = events.first() {
        if let Some(etag) = &event.etag {
            let updated = Bytes::from("BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:kickoff\nSUMMARY:Updated\nEND:VEVENT\nEND:VCALENDAR\n");
            client.put_if_match(&event.href, updated, etag).await?;
        }
    }

    Ok(())
}
```

### Timezones (RFC 4791 §5.2.2)

`CalDavClient::calendar_timezone(path)` reads a calendar's `calendar-timezone`
property (`Depth: 0` `PROPFIND`) and returns the stored iCalendar object —
an ICS document with exactly one `VTIMEZONE` component — verbatim as
`Option<String>` (`None` when the server does not store it). The same value is
surfaced per calendar in `CalendarInfo.timezone` by `list_calendars`.

Pair the returned object with a dedicated iCalendar parser (e.g. `icalendar`)
to derive the UTC offset rules; this library does not interpret `VTIMEZONE`
data — neither on read nor on write.

`CalDavClient::set_calendar_timezone(path, vtimezone)` writes the property
with a `Depth: 0` `PROPPATCH` (RFC 4791 §5.2.2): `Some(vtimezone)` sends a
`<D:set>` with the VTIMEZONE iCalendar object verbatim (XML-escaped), `None`
sends a `<D:remove>`. A blank value is rejected as
`Error::InvalidInput` before any network I/O. Because servers commonly accept
the request but reject the property, the per-property status inside the 207
multistatus decides the outcome: a non-success propstat for
`calendar-timezone` maps to `Error::UnexpectedStatus` with
`Operation::ProppatchCalendarTimezone` — except for a remove, where a `404`
propstat is success (removing an absent property is not an error per
RFC 4918 §14.23, which makes the remove idempotent).

Server support:

| Server | `calendar-timezone` support |
| --- | --- |
| Radicale | Supported (3.7.6): the PROPPATCH set stores the object and the remove makes it read back absent (the read-back value has LF line endings — every conformant XML processor normalizes CRLF → LF in parsed content per XML 1.0 §2.11); verified against the fixture |
| SabreDAV | Supported on calendar creation (set at `MKCALENDAR` time); the `PROPPATCH` write path is untested on this fixture |
| Nextcloud | Supported on calendar creation, and the `PROPPATCH` write path round-trips (set → read back the stored object, remove → absent; the read-back value has LF line endings — every conformant XML processor normalizes CRLF → LF in parsed content per XML 1.0 §2.11; verified against the fixture) |

### CalDAV scheduling (RFC 6638)

```rust
use fast_dav_rs::{CalDavClient, Result};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;

    let principal = client
        .discover_current_user_principal()
        .await?
        .ok_or_else(|| fast_dav_rs::Error::other("no principal returned"))?;
    let endpoints = client.discover_schedule_endpoints(&principal).await?;

    // Scheduling request against the outbox: `Originator` header (the
    // sender's cal-address) plus one `Recipient` header per attendee (a
    // widely-implemented CalendarServer extension, not defined by
    // RFC 6638); the raw iTIP body is sent verbatim, no parsing.
    // RFC 6638 §5 requires the outbox POST body to be a VFREEBUSY
    // component with METHOD:REQUEST.
    if let Some(outbox) = &endpoints.outbox {
        let request = Bytes::from(
            "BEGIN:VCALENDAR\nVERSION:2.0\nMETHOD:REQUEST\nBEGIN:VFREEBUSY\nUID:kickoff\nDTSTAMP:20260101T000000Z\nDTSTART:20260104T000000Z\nDTEND:20260105T000000Z\nORGANIZER:mailto:alice@example.com\nATTENDEE:mailto:bob@example.com\nEND:VFREEBUSY\nEND:VCALENDAR\n",
        );
        let response = client
            .post_schedule(
                outbox,
                "mailto:alice@example.com",
                &["mailto:bob@example.com"],
                request,
            )
            .await?;
        println!("scheduling POST returned {}", response.status);
    }

    // Incoming scheduling messages in the schedule inbox.
    if let Some(inbox) = &endpoints.inbox {
        for item in client.list_inbox(inbox).await? {
            println!("scheduling message: {}", item.href);
        }
    }

    Ok(())
}
```

### CardDAV contact CRUD

```rust
use fast_dav_rs::{CardDavClient, Result};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CardDavClient::new("https://carddav.example.com/users/alice/", None, None)?;
    let addressbook_path = "addressbooks/alice/team/";

    let contact_path = format!("{addressbook_path}jane.vcf");
    let vcard = Bytes::from("BEGIN:VCARD\nVERSION:3.0\nFN:Jane Doe\nUID:jane-1\nEMAIL:jane@example.com\nEND:VCARD\n");
    client.put_if_none_match(&contact_path, vcard).await?;

    let matches = client
        .addressbook_query_email(addressbook_path, "jane@example.com", true)
        .await?;

    if let Some(contact) = matches.first() {
        if let Some(etag) = &contact.etag {
            let updated = Bytes::from("BEGIN:VCARD\nVERSION:3.0\nFN:Jane Doe\nUID:jane-1\nEMAIL:jane@example.com\nTEL:+1-555-0100\nEND:VCARD\n");
            client.put_if_match(&contact.href, updated, etag).await?;
        }
    }

    Ok(())
}
```

For structured filtering, `CardDavClient::addressbook_query_filter` takes a
`CardDavFilter` and validates the RFC 6352 DTD exclusivity (§10.5.1: a
`prop-filter` cannot combine `is-not-defined` with a `text-match` or
`param-filter` children; §10.5.2: a `param-filter` cannot combine
`is-not-defined` with a `text-match`) before any network I/O, mirroring the
pre-I/O comp-filter/prop-filter/param-filter exclusivity validation of CalDAV
`calendar_query` (RFC 4791 §9.7.1-§9.7.3).

## Streaming & Sync

- Use `caldav::parse_multistatus_stream` for CalDAV responses and `carddav::parse_multistatus_stream`
  for CardDAV responses.
- `supports_webdav_sync` and `sync_collection` work for both calendars and addressbooks.
- `sync_collection_with_level` (all clients) sends a configurable `sync-level` (RFC 6578 §3.3):
  `SyncLevel::One` restricts the sync to the collection members, `SyncLevel::Infinite` includes
  all descendants.
- `sync_collection_resilient` (all clients) recovers automatically from a stale sync token —
  `410 Gone` (RFC 6578 §3.11) or `403 Forbidden` + `valid-sync-token` (§3.2) — by re-issuing the
  report as an initial sync and returning the full result set with the new token; any other error
  propagates unchanged. The response is flagged: the `WebDavClient` variant returns a 4-tuple whose
  last element is the `resynced` flag, and `caldav::SyncResponse`/`carddav::SyncResponse` expose
  `resynced == true`. Per RFC 6578 §3.4 an initial sync MUST NOT report deletions that predate the
  stale token, so rebuild your caches from `items` instead of applying them incrementally when the
  flag is set.
- **Result truncation (RFC 6578 §3.6):** when the server truncates a sync result set it reports
  `507 Insufficient Storage` inside the 207 multistatus (normally on the request-URI).
  `caldav::SyncResponse`/`carddav::SyncResponse` expose this as `truncated == true`; the 507
  element still appears in `items` with its per-item status, and the returned `sync_token`
  stays valid for fetching the next page. At the `WebDavClient` level, inspect `items` for a
  `HTTP/1.1 507 …` status.
- The sync types and helpers (`SyncItem`, `SyncResponse`, `build_sync_collection_body`,
  `map_sync_response`) are **module-qualified**: CalDAV and CardDAV define distinct same-named
  items, so they are only available as `fast_dav_rs::caldav::{SyncItem, SyncResponse, …}` and
  `fast_dav_rs::carddav::{SyncItem, SyncResponse, …}` — never at the crate root.

### SyncSession (stateful sync with transparent fallback)

`SyncSession` (new in this release, issue #160) packages the sync algorithm
above into a per-collection, in-memory state machine — the DAVx⁵ approach:

1. it probes `supported-report-set` **once** and caches the answer;
2. while the server supports RFC 6578 `sync-collection`, `initial()` returns
   the full state snapshot and `incremental()` returns a typed delta
   (`added` / `modified` / `deleted`) carrying the token to persist; 507
   result-set truncation is continued transparently;
3. on an unsupported server (or one that rejects the report with `403`/`405`)
   it falls back transparently to a `PROPFIND Depth: 1` etag diff, fetching
   content for changed members via batched `calendar-multiget` /
   `addressbook-multiget` REPORTs (CalDAV/CardDAV sessions);
4. a stale token — `410 Gone`, or `403` + `valid-sync-token` as observed on
   Radicale — resets the session transparently to a full initial sync,
   flagged `resynced == true` (rebuild caches; per RFC 6578 §3.4 the delta
   then reports no deletions);
5. conflicts: the server wins.

The session is in-memory only: **you** persist `sync_token` between runs
(store it next to your application data) and restore it with
`with_sync_token`. Clones share the token and the probe cache, like client
clones share the connection pool.

```rust
use fast_dav_rs::{CalDavClient, Result, SyncSession};

async fn sync_loop(client: &CalDavClient, saved_token: Option<&str>) -> Result<()> {
    // Restore the persisted token from your own storage when resuming.
    let session = client
        .sync_session("calendars/alice/work/")
        .with_sync_token(saved_token);

    let delta = session.incremental().await?;
    if delta.resynced {
        // Stale token: this is a full snapshot — rebuild your cache from
        // `delta.added` instead of applying it incrementally.
        println!("resync: {} live items", delta.added.len());
    }
    for entry in delta.added.iter().chain(&delta.modified) {
        println!("upsert {} (etag {:?})", entry.href, entry.etag);
    }
    for href in &delta.deleted {
        println!("remove {href}");
    }
    println!("persist this token: {:?}", delta.sync_token);
    Ok(())
}
```

A plain `WebDavClient::sync_session(collection)` requests `getetag` only;
the `CalDavClient`/`CardDavClient` constructors also fetch
`calendar-data`/`address-data` for every entry (and via multiget on the
fallback path).

A runnable end-to-end version — initial + incremental + stale-token resync
against the Radicale fixture, with `calendar-data` parsed by the `icalendar`
crate and a file-based token store — lives in
[`examples/sync_loop.rs`](examples/sync_loop.rs).

### WebDAV locking (class 2)

All clients (`WebDavClient`, `CalDavClient`, `CardDavClient`) support WebDAV locking (RFC 4918
class 2). `lock` sends `LOCK` with an explicit `Depth: 0` header (RFC 4918 §9.10.4), a
`Timeout: Second-N` header (clamped to `u32::MAX` seconds, RFC 4918 §10.7) and a `<D:lockinfo>`
body and returns the parsed `<D:activelock>` (`LockInfo`: token, timeout, scope, owner, lockroot,
depth); `refresh_lock` re-issues the `LOCK` with the token in an `If` header (RFC 4918 §9.10.7)
and falls back to the request token when the server omits `<D:locktoken>` in the response;
`unlock` sends `UNLOCK` with the token in a `Lock-Token` header. Tokens are validated
(RFC 4918 §10.5 Coded-URL grammar) before being embedded in a header. Non-success statuses
surface as `Error::UnexpectedStatus` with `Operation::Lock`/`Operation::Unlock` — or as
`Error::UnexpectedStatusWithDav` when the error body carries a `<D:error>` precondition (e.g.
`423 Locked` + `no-conflicting-lock`, RFC 4918 §16). A successful `LOCK` response without a lock
token fails with `Error::InvalidInput` (RFC 4918 §9.10.9).

The client keeps **no implicit lock state**: callers keep the token and pass it to
`refresh_lock`/`unlock`, or send it in an `If` header via the low-level `send` on conditional
writes. Check `capabilities()` (`class2`) to confirm the server supports locking. `PROPFIND`
responses containing the `lockdiscovery` property can be parsed with
`webdav::parse_lock_discovery_bytes`.

```rust
use fast_dav_rs::webdav::LockScope;
use fast_dav_rs::{CalDavClient, Result};

async fn edit_shared_doc(client: &CalDavClient) -> Result<()> {
    let lock = client
        .lock(
            "docs/plan.txt",
            LockScope::Exclusive,
            "<D:href>https://example.com/alice</D:href>",
            Some(300),
        )
        .await?;

    // Write while holding the lock: the token goes in an If header.
    let mut headers = hyper::HeaderMap::new();
    headers.insert("If", format!("(<{}>)", lock.token).parse().unwrap());
    client
        .send(
            hyper::Method::PUT,
            "docs/plan.txt",
            headers,
            Some(bytes::Bytes::from_static(b"updated content")),
            None,
        )
        .await?;

    client.refresh_lock("docs/plan.txt", &lock.token, Some(300)).await?;
    client.unlock("docs/plan.txt", &lock.token).await?;
    Ok(())
}
```

### Resilient sync example

```rust
use fast_dav_rs::{CalDavClient, Result, SyncLevel};

async fn sync(client: &CalDavClient) -> Result<()> {
    // Incremental sync; on 410 Gone the report is re-issued as an initial sync
    // and the full result set with the new token is returned.
    let sync = client
        .sync_collection_resilient("calendars/alice/work/", Some("stale-token"), None, true)
        .await?;
    println!("new token: {:?}", sync.sync_token);

    // Custom sync-level (RFC 6578 §3.3).
    let full = client
        .sync_collection_with_level("calendars/alice/work/", None, None, false, SyncLevel::Infinite)
        .await?;
    println!("items: {}", full.items.len());

    Ok(())
}
```

### CalDAV streaming example

```rust
use fast_dav_rs::{CalDavClient, Depth, Result, detect_encoding};
use fast_dav_rs::caldav::parse_multistatus_stream;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    let propfind_xml = r#"<D:propfind xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\"><D:prop><D:getetag/><C:calendar-data/></D:prop></D:propfind>"#;

    let response = client.propfind_stream("calendars/alice/work/", Depth::One, propfind_xml).await?;
    let encoding = detect_encoding(response.headers());
    let parsed = parse_multistatus_stream(response.into_body(), &[encoding]).await?;

    for item in parsed.items {
        if let Some(data) = item.calendar_data {
            println!("{} -> {} bytes", item.href, data.len());
        }
    }

    Ok(())
}
```

### CardDAV streaming example

```rust
use fast_dav_rs::{CardDavClient, Depth, Result, detect_encoding};
use fast_dav_rs::carddav::parse_multistatus_stream;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CardDavClient::new("https://carddav.example.com/users/alice/", None, None)?;
    let report_xml = r#"<C:addressbook-query xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:carddav\"><D:prop><D:getetag/><C:address-data/></D:prop></C:addressbook-query>"#;

    let response = client.report_stream("addressbooks/alice/team/", Depth::One, report_xml).await?;
    let encoding = detect_encoding(response.headers());
    let parsed = parse_multistatus_stream(response.into_body(), &[encoding]).await?;

    for item in parsed.items {
        if let Some(data) = item.address_data {
            println!("{} -> {} bytes", item.href, data.len());
        }
    }

    Ok(())
}
```

## Batch Operations

```rust
use fast_dav_rs::{CalDavClient, Depth, Result};
use bytes::Bytes;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    let paths = vec!["calendars/alice/work/".to_string(), "calendars/alice/home/".to_string()];

    let body = Arc::new(Bytes::from(r#"<D:propfind xmlns:D=\"DAV:\"><D:prop><D:displayname/></D:prop></D:propfind>"#));
    let results = client.propfind_many(paths, Depth::Zero, body, 4).await;

    for item in results {
        println!("{} -> {:?}", item.pub_path, item.result.as_ref().map(|r| r.status()));
    }

    Ok(())
}
```

## Runnable Examples

The `examples/` directory ships eight standalone binaries covering the main
workflows of this library. Each one documents its fixture prerequisites at
the top of the file and runs against one of the local e2e fixtures (start
them with the `setup.sh` scripts from [End-to-End Testing](#end-to-end-testing);
`typed_error_handling` needs no server):

```bash
cargo run --example <name>
```

| Example | Fixture | Demonstrates |
|---------|---------|--------------|
| `getting_started` | Radicale (:8081) | Discovery, calendar CRUD, `If-None-Match`/`If-Match` conditional writes and the 412 stale-etag race |
| `sync_loop` | Radicale (:8081) | `SyncSession` initial + incremental + stale-token resync, `icalendar` parsing, file-based sync-token persistence |
| `nextcloud_client` | Nextcloud (:8083) | Bearer-token builder vs Basic auth, VTODO creation and `calendar-query` fetch |
| `radicale_client` | Radicale (:8081) | Compliance/SyncSession probes on a no-LOCK provider, graceful `LOCK` → `405` handling |
| `streaming_large_collections` | Radicale (:8081) | `propfind_stream` + `parse_multistatus_stream_visit` with constant memory |
| `locking_concurrent_edits` | SabreDAV (:8080) | Full `lock`/`refresh_lock`/`unlock` lifecycle, `423` for token-less writes, graceful `405` on Radicale |
| `multiget_batched` | Radicale (:8081) | `calendar_multiget_many` chunked REPORTs with per-chunk failure reporting |
| `typed_error_handling` | none (offline) | Matching on `Error` variants with the `#[non_exhaustive]` wildcard arm |

Fixture-specific details (credentials, quirks) are in the fixture READMEs
under `sabredav-test/`, `radicale-test/`, and `nextcloud-test/`.

## Testing

```bash
cargo test --all-features
cargo test --doc
./run-e2e-tests.sh
```

### Provider compatibility matrix

Feature coverage per fixture — every ✅ cites the e2e test that asserts it
(full matrix, evidence, and per-fixture notes:
[`docs/compatibility.md`](docs/compatibility.md)):

| Feature | SabreDAV | Radicale | Nextcloud | Provider A |
| --- | --- | --- | --- | --- |
| Discovery (RFC 6764) | ✅ | ✅ | ✅ | ◐ |
| WebDAV-Sync (RFC 6578) | ✅ | ✅ | ✅ | — |
| LOCK (RFC 4918 class 2) | ✅ | ❌ | ✅ | — |
| Scheduling (RFC 6638) | ✅ | — | — | — |
| `calendar-timezone` (RFC 4791 §5.2.2) | — | ✅ | ✅ | — |
| Compression | ✅ | — | — | — |
| OAuth / Bearer | — | — | — | — |

✅ asserted by an e2e test · ❌ known unsupported (Radicale: `LOCK` → `405` despite an advertised class 2) · ◐ partial (Provider A: unauthenticated smoke-tier probes only) · — not tested. New providers are added only with a fixture.

## End-to-End Testing

The project ships three Docker e2e fixtures (SabreDAV, Radicale, Nextcloud)
plus an opt-in, credential-free smoke tier against a real-world deployment
(referred to as **Provider A** — never named in the repository). The suites
live in one tree per fixture under `tests/e2e/` (`sabredav/`, `radicale/`,
`nextcloud/`, `provider_a/`), with shared fixture helpers in
`tests/e2e/util.rs`.

| Tier | Fixture dir | Test target | URL (env override) | Credentials |
|------|-------------|-------------|--------------------|-------------|
| SabreDAV | `sabredav-test/` | `--test e2e_tests` | http://localhost:8080 (`SABREDAV_URL`) | `test` / `test` |
| Radicale | `radicale-test/` | `--test e2e_radicale` | http://localhost:8081 (`RADICALE_URL`) | `test` / `test` |
| Nextcloud | `nextcloud-test/` | `--test e2e_nextcloud` | http://localhost:8083 (`NEXTCLOUD_URL`) | `test` / `fixture-dav-password` |
| Provider A smoke | — (no fixture) | `--test e2e_provider_a_smoke -- --ignored` | `PROVIDER_A_DAV_URL` (required) | none |

CI runs the SabreDAV, Radicale, and Nextcloud tiers (jobs `e2e-tests`,
`e2e-radicale`, `e2e-nextcloud`); the Provider A smoke tier is `#[ignore]`-gated,
never runs in CI, uses zero credentials, and skips itself when
`PROVIDER_A_DAV_URL` is unset:

```bash
PROVIDER_A_DAV_URL=https://dav.example.test \
  cargo test --test e2e_provider_a_smoke -- --ignored --nocapture
```

It probes only the unauthenticated surface (`OPTIONS /`, the two well-known
URIs, an unauthenticated principal PROPFIND — 4 requests) and asserts the
401 + `WWW-Authenticate: Basic` challenge while recording the well-known
shape (redirect vs direct 401).

### SabreDAV (primary fixture)

This project includes a complete e2e testing environment with a SabreDAV server that supports CalDAV and CardDAV
features including compression, WebDAV locking (class 2), and WebDAV sync.

### Prerequisites

1. Docker and Docker Compose
2. The SabreDAV test environment (located in `sabredav-test/`)

### Setting up the test environment

```bash
cd sabredav-test
./setup.sh
```

This will start a complete SabreDAV environment with:

- Nginx with gzip, Brotli, and zstd compression modules
- PHP-FPM for better performance
- MySQL database with preconfigured SabreDAV tables
- Test user (test/test) and sample calendar events

### Running e2e tests

```bash
./run-e2e-tests.sh
```

Or manually:

```bash
cargo test --test e2e_tests -- --nocapture
```

### Resetting the test environment

To reset the database to a clean state:

```bash
cd sabredav-test
./reset-db.sh
```

### Radicale fixture

```bash
./radicale-test/setup.sh   # http://localhost:8081, user test/test
./radicale-test/reset.sh   # restart wipes the tmpfs data, re-seeds
cargo test --test e2e_radicale
```

Radicale is the second engine in the matrix and exercises different failure
modes: sync-token invalidation (`403` + `valid-sync-token`), no LOCK support
(405 despite an advertised class 2), and the auto-create-on-first-principal-
access quirk. See `radicale-test/README.md` for the full quirk list.

### Nextcloud fixture

```bash
./nextcloud-test/setup.sh   # http://localhost:8083, first boot installs the instance
./nextcloud-test/reset.sh   # full wipe + reinstall (slow)
cargo test --test e2e_nextcloud
```

Nextcloud is the real-world reference: DAV strictly under `/remote.php/dav/`,
`principals/users/{uid}` paths, VTODO coverage, and Basic auth (with app
passwords as the documented path for hardened instances; Bearer/OIDC is out
of scope for the fixture). See `nextcloud-test/README.md`.

### Provider quirks: UTF-8 double-encoding on CardDAV writes

Provider A's CardDAV write path can double-encode multi-byte UTF-8 in vCard
writes (corrupted text on later reads). The full quirk note and the
read-back workaround live in [`docs/compatibility.md`](docs/compatibility.md#provider-a).

## Limitations & Non-Goals

This library focuses on being a fast, low-level CalDAV/CardDAV client.

- It does not provide a server implementation.
- It does not model iCalendar or vCard data into high-level domain types.
- It does not manage offline sync state or conflict resolution for you.
- Some server-specific behaviors may require custom XML payloads.

## When NOT to Use This Library

Consider alternatives if:

- You need a full calendaring or contact domain model (RRULE handling, normalization, etc.).
- You need an offline-first sync engine with conflict resolution and local storage.
- You are looking for a server implementation rather than a client.

## Performance Tips

1. Prefer `sync_collection` over full scans when WebDAV-Sync is supported.
2. Use streaming parsing for large multistatus responses.
3. Reuse a single client instance to take advantage of connection pooling.
4. Use bounded concurrency for batch operations to avoid overload.
5. Keep request compression in `Auto` unless your payloads are tiny.

## Contributing

We welcome contributions. See `CONTRIBUTING.md` for the workflow and `AGENTS.md` for repository-specific guidelines.

## Credits

fast-dav-rs builds on the Rust ecosystem, including hyper, tokio, rustls, quick-xml, and async-compression.

## License

This package is licensed under the GNU Lesser General Public License v3.0 (LGPL-3.0).
See `LICENSE` for details.

## Support

- [Issue tracker](https://github.com/Goopil/fast-dav-rs/issues)
- [Discussions](https://github.com/Goopil/fast-dav-rs/discussions)
