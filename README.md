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
- API stability gated by `cargo-semver-checks` on every PR, with 1.0 to be declared once the remediation roadmap stabilizes.

## Versioning & Backward Compatibility

This project follows Semantic Versioning. Patch releases fix bugs, minor releases add compatible
features, and major releases introduce breaking changes when needed.

## Table of Contents

- [Why This Library?](#why-this-library)
- [Stability & Maturity](#stability--maturity)
- [Roadmap](#roadmap)
- [Versioning & Backward Compatibility](#versioning--backward-compatibility)
- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Documentation](#documentation)
- [Configuration](#configuration)
- [Security](#security)
- [Observability](#observability)
- [Usage Examples](#usage-examples)
- [Batch Operations](#batch-operations)
- [Runnable Examples](#runnable-examples)
- [Testing](#testing)
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
- The low-level APIs in some snippets in this README and the docs/ guides use `hyper` (`HeaderMap`,
  `Method`) and `bytes` (`Bytes`) directly; `fast-dav-rs` does not re-export
  them, so add them to your own `Cargo.toml` when you use those APIs.

## Installation

```bash
cargo add fast-dav-rs
```

## Quick Start

### CalDAV discovery

```rust,no_run
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

```rust,no_run
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
use fast_dav_rs::CalDavClient;
use fast_dav_rs::Error;

async fn retry_guidance(client: &CalDavClient) -> fast_dav_rs::Result<()> {
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
Ok(())
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
use fast_dav_rs::CalDavClient;
use fast_dav_rs::webdav::Privilege;

async fn check_privileges(client: &CalDavClient) -> fast_dav_rs::Result<()> {
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
Ok(())
}
```

## Documentation

Deep-dives live in the `docs/` directory (they are also part of the crate docs
on docs.rs, and their Rust snippets are run by the doc tests):

- [Error Handling & Migration](docs/error-handling.md) — typed `Error` variants, migrating from `anyhow`, streaming-callback errors
- [Advanced Configuration](docs/advanced-configuration.md) — builders: auth, token providers, TLS, proxy, HTTP/1.1, custom hyper client, redirects, retries, `Prefer`, validation
- [Streaming & Sync](docs/streaming-and-sync.md) — streaming XML parsing, WebDAV-Sync, `SyncSession`, WebDAV locking
- [End-to-End Testing](docs/e2e-testing.md) — Docker fixtures (SabreDAV, Radicale, Nextcloud) and the Provider A smoke tier
- [Provider compatibility matrix](docs/compatibility.md) — feature coverage per fixture with e2e evidence

## Configuration

### Request compression

```rust
fn main() -> fast_dav_rs::Result<()> {
use fast_dav_rs::{CalDavClient, ContentEncoding};
use fast_dav_rs::webdav::RequestCompressionMode;

let mut client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
client.set_request_compression_mode(RequestCompressionMode::Force(ContentEncoding::Gzip));
client.set_request_compression_mode(RequestCompressionMode::Auto);
client.set_request_compression_mode(RequestCompressionMode::Disabled);
Ok(())
}
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

For deployments that know they never talk plain HTTP, the builder offers an opt-in
**require HTTPS** guard (issue #200). With `require_https(true)` a plain `http://` base URL is
rejected at construction with `Error::InvalidConfig`, and any redirect whose target is not
`https://` — including an `https`→`http` downgrade during request handling **or** `.well-known`
service discovery — fails the request with `Error::InvalidInput` instead of being followed:

```rust,no_run
use fast_dav_rs::WebDavClient;
let client = WebDavClient::builder("https://dav.example.com/")
    .require_https(true)
    .build()?;
Ok::<(), fast_dav_rs::Error>(())
```

The flag is additive and off by default: without it, behavior is unchanged (plain `http://` base
URLs remain accepted for isolated test environments, and an `https`→`http` downgrade redirect is
never followed — the 3xx response is returned as-is; with `require_https(true)` that downgrade is
rejected with `Error::InvalidInput` instead).

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

```rust,no_run
use fast_dav_rs::{CalDavClient, Result};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    let calendar_path = "calendars/alice/work/";

    let event_path = format!("{calendar_path}kickoff.ics");
    let create = Bytes::from("BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//example//EN\nBEGIN:VEVENT\nUID:kickoff\nEND:VEVENT\nEND:VCALENDAR\n");
    client.put_if_none_match(&event_path, create).await?;

    let events = client
        .calendar_query_timerange(calendar_path, "VEVENT", None, None, true, None)
        .await?;

    if let Some(event) = events.first() {
        if let Some(etag) = &event.etag {
            let updated = Bytes::from("BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//example//EN\nBEGIN:VEVENT\nUID:kickoff\nSUMMARY:Updated\nEND:VEVENT\nEND:VCALENDAR\n");
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

```rust,no_run
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

```rust,no_run
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

## Batch Operations

```rust,no_run
use fast_dav_rs::{CalDavClient, Depth, Result};
use bytes::Bytes;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    let paths = vec!["calendars/alice/work/".to_string(), "calendars/alice/home/".to_string()];

    let body = Arc::new(Bytes::from(r#"<D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>"#));
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
them with the `setup.sh` scripts from [End-to-End Testing](docs/e2e-testing.md);
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
# Unit tests (nextest; equivalent: cargo test --all-features --test unit_tests)
cargo nextest run --all-features --locked --test unit_tests

# Doc tests — compile and run every Rust snippet in this README and docs/
cargo test --doc --all-features

# E2E — bring the fixture up first (see docs/e2e-testing.md)
./sabredav-test/setup.sh
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
| LOCK (RFC 4918 class 2) | ✅ | ❌ | ◐ | — |
| Scheduling (RFC 6638) | ✅ | — | — | — |
| `calendar-timezone` (RFC 4791 §5.2.2) | — | ✅ | ✅ | — |
| Compression | ✅ | — | — | — |
| OAuth / Bearer | — | — | — | — |

✅ asserted by an e2e test · ❌ known unsupported (Radicale: `LOCK` → `405` despite an advertised class 2) · ◐ partial (Provider A: unauthenticated smoke-tier probes only; Nextcloud LOCK: asserted on the files tree — the CalDAV tree accepts but does not enforce locks) · — not tested. New providers are added only with a fixture.

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
