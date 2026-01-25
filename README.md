# fast-dav-rs

[![Crates.io](https://img.shields.io/crates/v/fast-dav-rs.svg)](https://crates.io/crates/fast-dav-rs)
[![Documentation](https://docs.rs/fast-dav-rs/badge.svg)](https://docs.rs/fast-dav-rs)
[![CI](https://github.com/Goopil/fast-dav-rs/workflows/CI/badge.svg)](https://github.com/Goopil/fast-caldav-rs/actions)
[![dependency status](https://deps.rs/repo/github/goopil/fast-dav-rs/status.svg)](https://deps.rs/repo/github/goopil/fast-dav-rs)
[![License: LGPL v3](https://img.shields.io/badge/License-LGPL%20v3-blue.svg)](https://www.gnu.org/licenses/lgpl-3.0)

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
- [Configuration](#configuration)
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
- CardDAV addressbook discovery, queries, and contact CRUD.
- HTTP/2 with connection pooling and automatic response decompression.
- Streaming XML parsing for multistatus responses.
- ETag helpers and conditional methods for safe updates.

### Advanced Features

- WebDAV-Sync (RFC 6578) for incremental sync.
- Bounded parallelism for batch PROPFIND/REPORT operations.
- Automatic request compression negotiation (br, zstd, gzip) with overrides.
- Streaming send APIs for custom workflows.

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

### CardDAV discovery

```rust
use fast_dav_rs::CardDavClient;
use anyhow::Result;

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
        .ok_or_else(|| anyhow::anyhow!("no principal returned"))?;

    let homes = client.discover_addressbook_home_set(&principal).await?;
    let home = homes.first().expect("missing addressbook-home-set");

    for book in client.list_addressbooks(home).await? {
        println!("Addressbook: {:?}", book.displayname);
    }

    Ok(())
}
```

## Configuration

### Request compression

```rust
use fast_dav_rs::{CalDavClient, ContentEncoding};
use fast_dav_rs::webdav::RequestCompressionMode;

let mut client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
client.set_request_compression_mode(RequestCompressionMode::Force(ContentEncoding::Gzip));
client.set_request_compression_auto();
client.disable_request_compression();
```

### Per-request timeouts

The low-level `send` and `send_stream` methods accept an optional `per_req_timeout: Option<Duration>`
so you can override the default timeout for specific requests.

### Batch concurrency

`propfind_many` and `report_many` accept a `max_concurrency` parameter to bound the number of in-flight
requests while preserving input order in the result list.

## Usage Examples

### CalDAV event CRUD

```rust
use fast_dav_rs::CalDavClient;
use bytes::Bytes;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    let calendar_path = "calendars/alice/work/";

    let event_path = format!("{calendar_path}kickoff.ics");
    let create = Bytes::from("BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:kickoff\nEND:VEVENT\nEND:VCALENDAR\n");
    client.put_if_none_match(&event_path, create).await?;

    let events = client
        .calendar_query_timerange(calendar_path, "VEVENT", None, None, true)
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
use fast_dav_rs::CardDavClient;
use bytes::Bytes;
use anyhow::Result;

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

### CalDAV streaming example

```rust
use fast_dav_rs::{CalDavClient, Depth, detect_encoding};
use fast_dav_rs::caldav::parse_multistatus_stream;
use anyhow::Result;

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
use fast_dav_rs::{CardDavClient, Depth, detect_encoding};
use fast_dav_rs::carddav::parse_multistatus_stream;
use anyhow::Result;

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
use fast_dav_rs::{CalDavClient, Depth};
use bytes::Bytes;
use std::sync::Arc;
use anyhow::Result;

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

- [Issue tracker](https://github.com/Goopil/fast-caldav-rs/issues)
- [Discussions](https://github.com/Goopil/fast-dav-rs/discussions)
