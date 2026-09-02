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
- [Usage Examples](#usage-examples)
- [Streaming & Sync](#streaming--sync)
- [Batch Operations](#batch-operations)
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
- Client-side iCalendar validation for CalDAV writes (`ValidationLevel`, default `Structural`).
- CardDAV addressbook discovery, queries, and contact CRUD.
- HTTP/2 with connection pooling and automatic response decompression.
- Streaming XML parsing for multistatus responses.
- ETag helpers and conditional methods for safe updates.

### Advanced Features

- WebDAV-Sync (RFC 6578) for incremental sync.
- Bounded parallelism for batch PROPFIND/REPORT operations.
- Automatic request compression negotiation (br, zstd, gzip) with overrides.
- Streaming send APIs for custom workflows.
- RFC 6764 `.well-known` service discovery (`discover_caldav`/`discover_carddav`).

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
`UnexpectedStatus` (e.g. `PropfindCollections`, `ReportCalendarQuery`). The
`EtagReason` enum describes why an ETag was rejected (`Empty`,
`InvalidFormat`, `InvalidCharacters`, `InvalidHeaderValue`).

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

### Per-request timeouts

The low-level `send` and `send_stream` methods accept an optional `per_req_timeout: Option<Duration>`
so you can override the default timeout for specific requests.

### Batch concurrency

`propfind_many` and `report_many` accept a `max_concurrency` parameter to bound the number of in-flight
requests while preserving input order in the result list.

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

### Redirect following

HTTP redirects (301/302/303/307/308) are followed automatically in `send`/`send_stream`,
up to a configurable limit. On 303 the request is re-sent as `GET` without a body, and
when a redirect crosses origins (scheme, host, or port change) the `Authorization` and
`Cookie` headers are stripped for the remainder of the chain. Exceeding the limit fails
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
Redirects are followed by the client's redirect pipeline, so the **final** request URL is the
discovered service URL. A `404` (or a success answered directly on the `.well-known` URI)
returns the base URL unchanged as a documented fallback; any other non-success status fails
with `Error::UnexpectedStatus`. Client credentials are attached to the probe and stripped
automatically on cross-origin redirect hops. DNS SRV record lookup (RFC 6764 §3) is not
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

## Streaming & Sync

- Use `caldav::parse_multistatus_stream` for CalDAV responses and `carddav::parse_multistatus_stream`
  for CardDAV responses.
- `supports_webdav_sync` and `sync_collection` work for both calendars and addressbooks.
- `sync_collection_with_level` (all clients) sends a configurable `sync-level` (RFC 6578 §3.3):
  `SyncLevel::One` restricts the sync to the collection members, `SyncLevel::Infinite` includes
  all descendants.
- `sync_collection_resilient` (all clients) recovers automatically from `410 Gone` (stale sync
  token, RFC 6578 §3.11) by re-issuing the report as an initial sync and returning the full
  result set with the new token; any other error propagates unchanged.

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

## Testing

```bash
cargo test --all-features
cargo test --doc
./run-e2e-tests.sh
```

## End-to-End Testing

This project includes a complete e2e testing environment with a SabreDAV server that supports CalDAV and CardDAV
features including compression.

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
