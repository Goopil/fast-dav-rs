# Error Handling & Migration

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
use fast_dav_rs::caldav::{DavItem, parse_multistatus_bytes_visit};

async fn sync_calendar(client: &CalDavClient, path: &str) -> Result<()> {
    let report_xml = r#"<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/></D:prop></C:calendar-query>"#;
    let resp = client.report(path, Depth::One, report_xml).await?;

    parse_multistatus_bytes_visit(&resp.into_body(), |item: DavItem| {
        // Save to a database; wrap the DB error with context.
        save_to_db(&item).map_err(|e| {
            Error::with_source(format!("failed to save {}", item.href), e)
        })
    })?;

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
use fast_dav_rs::CalDavClient;
async fn migrate(client: &CalDavClient) -> anyhow::Result<()> {
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
let _ = principal;
Ok(())
}
```

### Complete migration example

The doctest below covers defining a typed `Error` enum with `#[from]` and
`#[error(...)]`, using `?` with automatic conversions, replacing `anyhow!()`
and `.context()`, and pattern matching on variants for programmatic error
handling.

Key patterns at a glance:

```rust
use std::num::ParseIntError;
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
    Err(AppError::Parse(source)) => eprintln!("not a number: {source}"),
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
