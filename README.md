# fast-dav-rs

[![Crates.io](https://img.shields.io/crates/v/fast-dav-rs.svg)](https://crates.io/crates/fast-dav-rs)
[![Documentation](https://docs.rs/fast-dav-rs/badge.svg)](https://docs.rs/fast-dav-rs)
[![CI](https://github.com/Goopil/fast-dav-rs/workflows/CI/badge.svg)](https://github.com/Goopil/fast-dav-rs/actions)

**fast-dav-rs** is a high-performance asynchronous CalDAV client for Rust. It blends `hyper 1.x`, `tokio`, `rustls`,
and streaming XML tooling so your services can discover calendars, manage events, and keep remote CalDAV stores in sync
without re-implementing the protocol by hand.

## Highlights

- HTTP/2 with connection pooling, adaptive windows, and configurable timeouts.
- Automatic response decompression plus optional request compression (br, zstd, gzip).
- Streaming XML parsing for large `PROPFIND`/`REPORT` responses.
- Convenience helpers for ETags, conditional methods, and safe deletions.
- First-class support for `REPORT calendar-query`, `calendar-multiget`, WebDAV-Sync (RFC 6578), and bounded parallelism.

## Install

```bash
cargo add fast-dav-rs
```

The crate targets Rust 2024 and expects a multi-threaded `tokio` runtime with the `macros`, `rt-multi-thread`, and
`time` features enabled.

## Quick Start

Discover calendars and list available collections:

```rust
use fast_dav_rs::CalDavClient;
use anyhow::Result;

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
        .ok_or_else(|| anyhow::anyhow!("no principal returned"))?;

    let homes = client.discover_calendar_home_set(&principal).await?;
    let home = homes.first().expect("missing calendar-home-set");

    for calendar in client.list_calendars(home).await? {
        println!("Calendar: {:?}", calendar.displayname);
    }

    Ok(())
}
```

## Common Operations

- **Create / update events**: `put_if_none_match` and `put_if_match` accept ICS payloads and attach the proper
  conditional headers.
- **Filter collections**: `calendar_query_timerange` builds a `REPORT` to fetch events within a date range.
- **Safe deletion**: `delete_if_match` ensures you do not remove an event that changed on the server.
- **Batch work**: `propfind_many` and the `map_*` helpers run multiple requests concurrently with bounded concurrency.
- **Compression**: `set_request_compression(ContentEncoding::Gzip)` compresses request bodies when the server supports
  it.

```rust
use fast_dav_rs::{CalDavClient, ContentEncoding};
use bytes::Bytes;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let mut client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    client.set_request_compression(ContentEncoding::Gzip);

    let ics = Bytes::from_static(b"BEGIN:VCALENDAR\n...END:VCALENDAR\n");
    client.put_if_none_match("work/calendar/event.ics", ics).await?;

    let events = client
        .calendar_query_timerange("work/calendar/", "VEVENT", None, None, true)
        .await?;

    for event in events {
        println!("{} -> {:?}", event.href, event.etag);
    }
    Ok(())
}
```

## Streaming & Sync

- `propfind_stream` combined with `parse_multistatus_stream` iterates a `207 Multi-Status` response without buffering
  the entire payload.
- `detect_encoding` chooses the right decoder for compressed responses.
- `supports_webdav_sync` and `sync_collection` let you build incremental sync loops based on sync tokens rather than
  full scans.

## Development Commands

- `cargo build` — fast local compile.
- `cargo fmt` / `cargo clippy --all-targets --all-features` — mandatory formatting and linting before you push.
- `cargo test --all-features` and `cargo test --doc` — run unit, integration, and doctests to keep examples accurate.
- `./run-e2e-tests.sh` — run end-to-end tests against a local SabreDAV server.

## End-to-End Testing

This project includes a complete E2E testing environment with a SabreDAV server that supports all major CalDAV features including compression.

See [E2E_TESTING.md](E2E_TESTING.md) for detailed documentation on the testing environment and how to run the tests.

### Prerequisites

1. Docker and Docker Compose
2. The SabreDAV test environment (located in `sabredav-test/`)

### Setting up the Test Environment

```bash
cd sabredav-test
./setup.sh
```

This will start a complete SabreDAV environment with:
- Nginx with gzip, Brotli, and zstd compression modules
- PHP-FPM for better performance
- MySQL database with preconfigured SabreDAV tables
- Test user (test/test) and sample calendar events

### Running E2E Tests

```bash
./run-e2e-tests.sh
```

Or manually:

```bash
cargo test --test caldav_suite -- e2e_tests --nocapture
```

### Resetting the Test Environment

To reset the database to a clean state:

```bash
cd sabredav-test
./reset-db.sh
```

See `CONTRIBUTING.md` for the standard workflow and `AGENTS.md` for repository-specific guidelines. Pull requests
improving server compatibility, ergonomics, or documentation are very welcome.

