# Fix PR #69 Code Review Items Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address all 10 code review items from PR #69 so the branch merges clean.

**Architecture:** Incremental fixes to the `Error` enum and its call sites, plus CI hardening and cleanup. No architectural changes — each task is independently testable and committable.

**Tech Stack:** Rust, thiserror, hyper, rustls, GitHub Actions

## Global Constraints

- All public API changes must keep `cargo clippy --all-targets --all-features -- -D warnings` passing
- All public API changes must keep `cargo fmt --all --check` passing
- All tests must pass: `cargo test --all-features --locked --test unit_tests`
- Doc tests must pass: `cargo test --doc`
- The `Error` enum is `#[non_exhaustive]` — always include a wildcard arm when matching in tests
- No comments in code unless explicitly requested by a review item
- Follow existing naming conventions: `PascalCase` structs/enums, `snake_case` functions

---

## Review Item Summary

From Goopil's review on PR #69:

| # | Severity | Item | Action |
|---|----------|------|--------|
| 1 | HIGH | `InvalidInput(String)` is string-matching backdoor | Add structured variants for ETag, component, datetime, config |
| 2 | HIGH | `UnexpectedStatus { operation: String }` allocates on every error | Change to `Operation` enum (no allocation in error path) |
| 3 | HIGH | Struct variants are not `#[non_exhaustive]` | Add `#[non_exhaustive]` to each struct variant |
| 4 | MEDIUM | `Other` is an anyhow-like backdoor | Document as escape-hatch only (docs-only) |
| 5 | MEDIUM | `Tls { source: Option<...> }` — why Option? | Document when source is None, keep Option for "no roots" case |
| 6 | MEDIUM | No `#[from]` for `rustls::Error` | Add `#[from]` conversion to reduce boilerplate |
| 7 | MEDIUM | `from_quick_xml` stringifies Syntax/IllFormed | Keep as-is, already documented (no action needed) |
| 8 | LOW | Scope creep — security bonuses + CI badges | Acknowledge in PR reply (no code action) |
| 9 | LOW | `examples/migration.rs` not build-checked in CI | Add `cargo build --examples` to CI |
| 10 | LOW | Legacy deprecated modules | Put behind `legacy` feature gate |

---

## File Structure

**Files to modify:**
- `src/error.rs` — Add structured `InvalidInput` variants, `Operation` enum, `#[non_exhaustive]` on struct variants, `#[from]` for rustls, docs
- `src/webdav/client.rs` — Update `InvalidInput` call sites to new variants, update `UnexpectedStatus` to use `Operation` enum
- `src/webdav/builder.rs` — Update `InvalidInput` call sites to new variants, remove manual rustls wrapping
- `src/webdav/xml.rs` — Update `InvalidInput` call sites to new variants
- `src/caldav/client.rs` — Update `UnexpectedStatus` call sites to use `Operation` enum
- `src/carddav/client.rs` — Update `UnexpectedStatus` call sites to use `Operation` enum
- `src/lib.rs` — Gate legacy modules behind `legacy` feature
- `Cargo.toml` — Add `legacy` feature, add `rustls` as direct dependency for `#[from]`
- `tests/unit/common/error_tests.rs` — Update tests for new variants
- `tests/unit/webdav/builder_tests.rs` — Update tests for new `InvalidInput` variants
- `.github/workflows/ci.yml` — Add `cargo build --examples` step

**Files to create:**
- None

---

## Task 1: Add `#[non_exhaustive]` to struct variants (HIGH #3)

**Rationale:** The enum is `#[non_exhaustive]` but struct variants are not. Adding a field to a struct variant is currently a breaking change. Adding `#[non_exhaustive]` to each struct variant makes them future-proof.

**Files:**
- Modify: `src/error.rs:15-22` (InvalidUrl)
- Modify: `src/error.rs:53-59` (UnexpectedStatus)
- Modify: `src/error.rs:62-66` (Timeout)
- Modify: `src/error.rs:98-105` (Tls)
- Modify: `src/error.rs:114-121` (Other)
- Test: `tests/unit/common/error_tests.rs`

**Interfaces:**
- Produces: All struct variants in `Error` are now `#[non_exhaustive]`. Callers constructing these variants outside the crate must use `..` syntax. Inside the crate, construction is unaffected (private fields can still be set directly, but `#[non_exhaustive]` still requires `..` for external construction). Note: the `Error::Tls` variant is constructed in tests directly (`tests/unit/common/error_tests.rs:177-180` and `:203-206`) — these will need `..Default::default()` or `..` added.

- [ ] **Step 1: Update error.rs struct variants**

Add `#[non_exhaustive]` to each struct variant in `src/error.rs`:

```rust
#[error("invalid URL `{url}`: {source}")]
#[non_exhaustive]
InvalidUrl {
    /// The URL value that failed validation.
    url: String,
    /// The URI parser error.
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
},
```

Do the same for `UnexpectedStatus`, `Timeout`, `Tls`, and `Other`.

- [ ] **Step 2: Fix test construction of Tls variant**

In `tests/unit/common/error_tests.rs`, the `tls_error_preserves_source_chain` test (line 177) and `tls_error_display_includes_context` test (line 203) construct `Error::Tls` directly. Add `..` to make them compile:

```rust
let error = Error::Tls {
    context: "failed to parse PEM certificate".to_owned(),
    source: Some(Box::new(source)),
    ..
};
```

Do the same for `public_error_variants_expose_retry_relevant_context` (line 50) which constructs `UnexpectedStatus`:

```rust
let status_error = Error::UnexpectedStatus {
    operation: "PROPFIND calendars".to_owned(),
    status: StatusCode::FORBIDDEN,
    ..
};
```

And for `Timeout` construction in the same test (line 59):

```rust
let timeout_error = Error::Timeout {
    limit: Duration::from_secs(20),
    ..
};
```

- [ ] **Step 3: Run clippy and tests**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --all-features --locked --test unit_tests`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/error.rs tests/unit/common/error_tests.rs
git commit -m "fix: add #[non_exhaustive] to Error struct variants

Struct variants (InvalidUrl, UnexpectedStatus, Timeout, Tls, Other)
were not non_exhaustive, meaning adding a field would be a breaking
change even with the enum being #[non_exhaustive]. Addresses review
item #3 (HIGH)."
```

---

## Task 2: Replace `UnexpectedStatus { operation: String }` with `Operation` enum (HIGH #2)

**Rationale:** All 12 call sites use `.to_owned()` on static string literals. For a "fast" library, this is unnecessary allocation in the error path. An `Operation` enum with `&'static str` avoids this entirely.

**Files:**
- Modify: `src/error.rs:52-59` — Add `Operation` enum, change `UnexpectedStatus`
- Modify: `src/caldav/client.rs` — 6 call sites
- Modify: `src/carddav/client.rs` — 6 call sites
- Test: `tests/unit/common/error_tests.rs`

**Interfaces:**
- Produces: `pub enum Operation` with variants for each DAV operation. `Error::UnexpectedStatus` now has `operation: Operation` instead of `operation: String`.

- [ ] **Step 1: Define the `Operation` enum in error.rs**

Add after the `Error` enum definition (before `impl Error`), around line 122:

```rust
/// Identifies which DAV operation produced an [`Error::UnexpectedStatus`].
///
/// Using an enum instead of `String` avoids allocation in the error path
/// and lets callers match on the operation without string comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Operation {
    /// `PROPFIND` to discover the current-user-principal.
    PropfindCurrentUserPrincipal,
    /// `PROPFIND` to discover the calendar-home-set.
    PropfindCalendarHomeSet,
    /// `PROPFIND` to discover the addressbook-home-set.
    PropfindAddressbookHomeSet,
    /// `PROPFIND` to list calendars or addressbooks.
    PropfindCollections,
    /// `REPORT` calendar-query.
    ReportCalendarQuery,
    /// `REPORT` calendar-multiget.
    ReportCalendarMultiget,
    /// `REPORT` addressbook-query.
    ReportAddressbookQuery,
    /// `REPORT` addressbook-multiget.
    ReportAddressbookMultiget,
    /// `REPORT` sync-collection.
    ReportSyncCollection,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::PropfindCurrentUserPrincipal => "PROPFIND current-user-principal",
            Self::PropfindCalendarHomeSet => "PROPFIND calendar-home-set",
            Self::PropfindAddressbookHomeSet => "PROPFIND addressbook-home-set",
            Self::PropfindCollections => "PROPFIND calendars",
            Self::ReportCalendarQuery => "REPORT calendar-query",
            Self::ReportCalendarMultiget => "REPORT calendar-multiget",
            Self::ReportAddressbookQuery => "REPORT addressbook-query",
            Self::ReportAddressbookMultiget => "REPORT addressbook-multiget",
            Self::ReportSyncCollection => "REPORT sync-collection",
        };
        f.write_str(s)
    }
}
```

- [ ] **Step 2: Change `UnexpectedStatus` variant to use `Operation`**

In `src/error.rs`, change the `UnexpectedStatus` variant:

```rust
/// A DAV operation returned an unexpected HTTP status.
#[error("{operation} failed with {status}")]
#[non_exhaustive]
UnexpectedStatus {
    /// The operation that failed.
    operation: Operation,
    /// The status returned by the server.
    status: StatusCode,
},
```

- [ ] **Step 3: Update caldav/client.rs call sites**

In `src/caldav/client.rs`, replace each `operation: "PROPFIND ...".to_owned()` with the matching `Operation` variant. There are 6 call sites:

- Line 395: `operation: "PROPFIND current-user-principal".to_owned()` → `operation: Operation::PropfindCurrentUserPrincipal`
- Line 426: `operation: "PROPFIND calendar-home-set".to_owned()` → `operation: Operation::PropfindCalendarHomeSet`
- Line 460: `operation: "PROPFIND calendars".to_owned()` → `operation: Operation::PropfindCollections`
- Line 507: `operation: "REPORT calendar-query".to_owned()` → `operation: Operation::ReportCalendarQuery`
- Line 533: `operation: "REPORT calendar-multiget".to_owned()` → `operation: Operation::ReportCalendarMultiget`
- Line 554: `operation: "REPORT sync-collection".to_owned()` → `operation: Operation::ReportSyncCollection`

Make sure to add `use crate::error::Operation;` to the imports (or `use crate::{Error, Operation, Result};`).

- [ ] **Step 4: Update carddav/client.rs call sites**

In `src/carddav/client.rs`, replace each `operation: "PROPFIND ...".to_owned()` with the matching `Operation` variant. There are 6 call sites:

- Line 415: `operation: "PROPFIND current-user-principal".to_owned()` → `operation: Operation::PropfindCurrentUserPrincipal`
- Line 446: `operation: "PROPFIND addressbook-home-set".to_owned()` → `operation: Operation::PropfindAddressbookHomeSet`
- Line 479: `operation: "PROPFIND addressbooks".to_owned()` → `operation: Operation::PropfindCollections`
- Line 499: `operation: "REPORT addressbook-query".to_owned()` → `operation: Operation::ReportAddressbookQuery`
- Line 561: `operation: "REPORT addressbook-multiget".to_owned()` → `operation: Operation::ReportAddressbookMultiget`
- Line 582: `operation: "REPORT sync-collection".to_owned()` → `operation: Operation::ReportSyncCollection`

Make sure to add `use crate::error::Operation;` to the imports.

- [ ] **Step 5: Update lib.rs re-exports**

In `src/lib.rs`, add `Operation` to the re-exports from `error`:

```rust
pub use error::{Error, Operation, Result};
```

- [ ] **Step 6: Update error_tests.rs**

In `tests/unit/common/error_tests.rs`, update the test at line 50:

```rust
let status_error = Error::UnexpectedStatus {
    operation: Operation::PropfindCollections,
    status: StatusCode::FORBIDDEN,
    ..,
};
assert_eq!(
    status_error.to_string(),
    "PROPFIND calendars failed with 403 Forbidden"
);
```

Add `use fast_dav_rs::Operation;` to the test imports.

- [ ] **Step 7: Run clippy, fmt, and tests**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --all-features --locked --test unit_tests`
Run: `cargo test --doc`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/error.rs src/caldav/client.rs src/carddav/client.rs src/lib.rs tests/unit/common/error_tests.rs
git commit -m "fix: replace UnexpectedStatus String with Operation enum

Replaces operation: String (which allocated on every error via
.to_owned() on static literals) with a #[non_exhaustive] Operation
enum carrying &'static str via Display. Removes allocation from the
error path. Addresses review item #2 (HIGH)."
```

---

## Task 3: Add structured `InvalidInput` variants (HIGH #1)

**Rationale:** `InvalidInput(String)` is used for ~15 distinct cases (empty ETag, invalid ETag format, component name, datetime, timeout, pool, bearer, basic_auth, proxy_auth). Tests already string-match on it, which is exactly the pattern this PR was supposed to eliminate. We add structured variants so callers can match programmatically.

**Files:**
- Modify: `src/error.rs` — Add new variants, keep `InvalidInput(String)` as catch-all
- Modify: `src/webdav/client.rs` — Update ETag validation call sites (lines 48, 53, 61, 68, 74, 79)
- Modify: `src/webdav/builder.rs` — Update config validation call sites (lines 255, 258, 265, 275, 283, 290, 297, 304)
- Modify: `src/webdav/xml.rs` — Update component/datetime validation call sites (lines 31, 39, 66)
- Test: `tests/unit/common/error_tests.rs` — Update existing string-matching tests
- Test: `tests/unit/webdav/builder_tests.rs` — Update existing string-matching tests

**Interfaces:**
- Produces: New `Error` variants: `InvalidEtag`, `InvalidComponentName`, `InvalidDateTime`, `InvalidConfig`. `InvalidInput(String)` remains as a catch-all for any cases not covered by specific variants.

- [ ] **Step 1: Add new variants to Error enum in error.rs**

Add these new variants after `InvalidInput(String)` (line 26) and before `InvalidHeader`:

```rust
/// An ETag value failed validation (empty, malformed, or contains
/// invalid characters for use in an `If-Match` / `If-None-Match` header).
#[error("invalid ETag: {reason}")]
#[non_exhaustive]
InvalidEtag {
    /// Why the ETag was rejected.
    reason: EtagReason,
},

/// A calendar or addressbook component name failed validation.
#[error("invalid component name `{name}`: {reason}")]
#[non_exhaustive]
InvalidComponentName {
    /// The component name that was rejected.
    name: String,
    /// Why it was rejected.
    reason: &'static str,
},

/// A date-time value did not match the expected iCalendar UTC format.
#[error("invalid UTC date-time `{value}`: {reason}")]
#[non_exhaustive]
InvalidDateTime {
    /// The value that failed validation.
    value: String,
    /// Why it was rejected.
    reason: &'static str,
},

/// A builder configuration value is invalid (timeout, pool size, auth, etc.).
#[error("invalid configuration: {0}")]
InvalidConfig(String),
```

And define the `EtagReason` enum before `Error`:

```rust
/// Why an ETag was rejected by validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EtagReason {
    /// The ETag string was empty or only whitespace.
    Empty,
    /// The ETag has an invalid entity-tag format (e.g. unbalanced quotes).
    InvalidFormat,
    /// The ETag contains characters not allowed in entity tags.
    InvalidCharacters,
    /// The ETag cannot be used as an HTTP header value.
    InvalidHeaderValue,
}

impl std::fmt::Display for EtagReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Empty => "ETag cannot be empty",
            Self::InvalidFormat => "invalid entity-tag format",
            Self::InvalidCharacters => "contains invalid entity-tag characters",
            Self::InvalidHeaderValue => "cannot be used as an If-Match header value",
        };
        f.write_str(s)
    }
}
```

- [ ] **Step 2: Update webdav/client.rs ETag validation**

In `src/webdav/client.rs`, update the `if_match_header_value` function (lines 45-84):

Replace the `Error::InvalidInput(...)` calls with typed variants:

```rust
pub(crate) fn if_match_header_value(etag: &str) -> Result<header::HeaderValue> {
    let etag = etag.trim();
    if etag.is_empty() {
        return Err(Error::InvalidEtag {
            reason: EtagReason::Empty,
            ..
        });
    }

    if etag == "*" || is_valid_entity_tag(etag) {
        return header::HeaderValue::from_str(etag).map_err(|err| {
            Error::InvalidEtag {
                reason: EtagReason::InvalidHeaderValue,
                source: Box::new(err),
                ..
            }
        });
    }

    if let Some(opaque) = etag.strip_prefix("W/") {
        validate_opaque_tag(opaque)?;
        let value = format!("W/\"{opaque}\"");
        return header::HeaderValue::from_str(&value).map_err(|err| {
            Error::InvalidEtag {
                reason: EtagReason::InvalidHeaderValue,
                source: Box::new(err),
                ..
            }
        });
    }

    validate_opaque_tag(etag)?;
    let value = format!("\"{etag}\"");
    header::HeaderValue::from_str(&value).map_err(|err| {
        Error::InvalidEtag {
            reason: EtagReason::InvalidHeaderValue,
            source: Box::new(err),
            ..
        }
    })
}

fn validate_opaque_tag(opaque: &str) -> Result<()> {
    if opaque.is_empty() || opaque.contains('"') {
        return Err(Error::InvalidEtag {
            reason: EtagReason::InvalidFormat,
            ..
        });
    }
    if !opaque.bytes().all(is_etag_character) {
        return Err(Error::InvalidEtag {
            reason: EtagReason::InvalidCharacters,
            ..
        });
    }
    Ok(())
}
```

Wait — `InvalidEtag` needs a `source` field for the `InvalidHeaderValue` case (to preserve the `InvalidHeaderValue` error chain). Update the variant definition to include an optional source:

```rust
#[error("invalid ETag: {reason}")]
#[non_exhaustive]
InvalidEtag {
    /// Why the ETag was rejected.
    reason: EtagReason,
    /// The underlying header parsing error, if applicable.
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
},
```

And simplify the call sites (constructing with `source: None` when not applicable):

```rust
pub(crate) fn if_match_header_value(etag: &str) -> Result<header::HeaderValue> {
    let etag = etag.trim();
    if etag.is_empty() {
        return Err(Error::InvalidEtag {
            reason: EtagReason::Empty,
            source: None,
            ..
        });
    }

    if etag == "*" || is_valid_entity_tag(etag) {
        return header::HeaderValue::from_str(etag).map_err(|err| {
            Error::InvalidEtag {
                reason: EtagReason::InvalidHeaderValue,
                source: Some(Box::new(err)),
                ..
            }
        });
    }

    if let Some(opaque) = etag.strip_prefix("W/") {
        validate_opaque_tag(opaque)?;
        let value = format!("W/\"{opaque}\"");
        return header::HeaderValue::from_str(&value).map_err(|err| {
            Error::InvalidEtag {
                reason: EtagReason::InvalidHeaderValue,
                source: Some(Box::new(err)),
                ..
            }
        });
    }

    validate_opaque_tag(etag)?;
    let value = format!("\"{etag}\"");
    header::HeaderValue::from_str(&value).map_err(|err| {
        Error::InvalidEtag {
            reason: EtagReason::InvalidHeaderValue,
            source: Some(Box::new(err)),
            ..
        }
    })
}

fn validate_opaque_tag(opaque: &str) -> Result<()> {
    if opaque.is_empty() || opaque.contains('"') {
        return Err(Error::InvalidEtag {
            reason: EtagReason::InvalidFormat,
            source: None,
            ..
        });
    }
    if !opaque.bytes().all(is_etag_character) {
        return Err(Error::InvalidEtag {
            reason: EtagReason::InvalidCharacters,
            source: None,
            ..
        });
    }
    Ok(())
}
```

Add `use crate::error::EtagReason;` to imports.

- [ ] **Step 3: Update webdav/xml.rs validation functions**

In `src/webdav/xml.rs`, update `validate_component_name` (lines 29-45):

```rust
pub(crate) fn validate_component_name(name: &str, context: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::InvalidComponentName {
            name: name.to_owned(),
            reason: "component name must not be empty",
            ..
        });
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
    {
        return Err(Error::InvalidComponentName {
            name: name.to_owned(),
            reason: "only ASCII letters, digits and '-' are allowed (e.g. VEVENT, X-CUSTOM)",
            ..
        });
    }
    Ok(())
}
```

Update `validate_utc_datetime` (lines 58-72):

```rust
pub(crate) fn validate_utc_datetime(value: &str, context: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let structurally_valid = bytes.len() == 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'Z';
    if !structurally_valid {
        return Err(Error::InvalidDateTime {
            value: value.to_owned(),
            reason: "expected iCalendar format YYYYMMDDTHHMMSSZ (e.g. 20240101T000000Z)",
            ..
        });
    }
    Ok(())
}
```

Note: The `context` parameter is no longer used in the error message (the variant carries its own context). But keep the parameter signature for API stability — just don't use it in the error construction. Actually, to avoid an unused variable warning, prefix with underscore: `_context`.

- [ ] **Step 4: Update webdav/builder.rs config validation**

In `src/webdav/builder.rs`, replace all `Error::InvalidInput(...)` calls for config validation with `Error::InvalidConfig(...)`:

- Line 255: `Error::InvalidInput("timeout must be > 0".to_owned())` → `Error::InvalidConfig("timeout must be > 0".to_owned())`
- Line 258: `Error::InvalidInput("pool_max_idle_per_host must be > 0".to_owned())` → `Error::InvalidConfig("pool_max_idle_per_host must be > 0".to_owned())`
- Line 265: `Error::InvalidInput("bearer_token must not be empty".to_owned())` → `Error::InvalidConfig("bearer_token must not be empty".to_owned())`
- Line 275: `Error::InvalidInput("bearer_token contains invalid characters...".to_owned())` → `Error::InvalidConfig("bearer_token contains invalid characters (allowed: A-Z a-z 0-9 - . _ ~ + / =)".to_owned())`
- Line 283: `Error::InvalidInput("basic_auth requires both user and pass to be non-empty".to_owned())` → `Error::InvalidConfig("basic_auth requires both user and pass to be non-empty".to_owned())`
- Line 290: `Error::InvalidInput("proxy_basic_auth requires a proxy to be set via .proxy()".to_owned())` → `Error::InvalidConfig("proxy_basic_auth requires a proxy to be set via .proxy()".to_owned())`
- Line 297: `Error::InvalidInput("proxy_basic_auth requires both user and pass to be non-empty".to_owned())` → `Error::InvalidConfig("proxy_basic_auth requires both user and pass to be non-empty".to_owned())`
- Line 304: `Error::InvalidInput(format!("proxy_basic_auth {label} contains control..."))` → `Error::InvalidConfig(format!("proxy_basic_auth {label} contains control or whitespace characters which are not allowed in HTTP header values"))`

- [ ] **Step 5: Update error_tests.rs**

In `tests/unit/common/error_tests.rs`, update the `invalid_etag_is_a_typed_input_error` test (line 20):

```rust
#[tokio::test]
async fn invalid_etag_is_a_typed_input_error() {
    let client = CalDavClient::new("http://localhost/", None, None).unwrap();
    let error = client
        .put_if_match("event.ics", bytes::Bytes::new(), "")
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Error::InvalidEtag { reason: EtagReason::Empty, .. }
    ));
}
```

Add `use fast_dav_rs::EtagReason;` to imports.

Update the `invalid_calendar_component_is_a_typed_input_error` test (line 34):

```rust
#[tokio::test]
async fn invalid_calendar_component_is_a_typed_input_error() {
    let client = CalDavClient::new("http://localhost/", None, None).unwrap();
    let error = client
        .calendar_query_timerange("calendar/", "VEVENT/INVALID", None, None, false)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Error::InvalidComponentName { name, .. } if name == "VEVENT/INVALID"
    ));
}
```

- [ ] **Step 6: Update builder_tests.rs**

In `tests/unit/webdav/builder_tests.rs`, update the string-matching tests:

- `proxy_basic_auth_with_newline_user_errors` (line 819): change `Error::InvalidInput(ref msg) if msg.contains("proxy_basic_auth")` → `Error::InvalidConfig(ref msg) if msg.contains("proxy_basic_auth")`
- `proxy_basic_auth_with_newline_pass_errors` (line 835): same change
- `error_message_contains_timeout_hint` (line 868): change `Error::InvalidInput(ref msg) if msg.contains("timeout must be > 0")` → `Error::InvalidConfig(ref msg) if msg.contains("timeout must be > 0")`
- `error_message_contains_pool_hint` (line 880): change `Error::InvalidInput(ref msg) if msg.contains("pool_max_idle_per_host must be > 0")` → `Error::InvalidConfig(ref msg) if msg.contains("pool_max_idle_per_host must be > 0")`

- [ ] **Step 7: Re-export new types from lib.rs**

In `src/lib.rs`, update the re-export:

```rust
pub use error::{Error, EtagReason, Operation, Result};
```

- [ ] **Step 8: Run clippy, fmt, and tests**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --all-features --locked --test unit_tests`
Run: `cargo test --doc`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add src/error.rs src/webdav/client.rs src/webdav/builder.rs src/webdav/xml.rs src/lib.rs tests/unit/common/error_tests.rs tests/unit/webdav/builder_tests.rs
git commit -m "fix: add structured InvalidInput variants for typed errors

Replaces string-matching on InvalidInput(String) with typed variants:
InvalidEtag (with EtagReason enum), InvalidComponentName,
InvalidDateTime, and InvalidConfig. InvalidInput(String) remains as a
catch-all. Addresses review item #1 (HIGH)."
```

---

## Task 4: Add `#[from]` for `rustls::Error` (MEDIUM #6)

**Rationale:** rustls errors are wrapped manually via `Error::tls("...", e)`. An automatic `#[from]` conversion reduces boilerplate at call sites.

**Files:**
- Modify: `src/error.rs` — Add `#[from]` variant for `rustls::Error`
- Modify: `src/webdav/builder.rs:489` — Use `?` instead of manual wrapping
- Modify: `Cargo.toml` — Ensure `rustls` is a direct dependency (not transitive)

**Interfaces:**
- Produces: `Error::TlsRustls(rustls::Error)` variant with `#[from]`. The existing `Error::Tls { context, source }` variant remains for cases that need additional context.

- [ ] **Step 1: Check Cargo.toml for rustls dependency**

Check if `rustls` is already listed as a direct dependency in `Cargo.toml`. If not, add it. Read `Cargo.toml` first.

If `rustls` is only a transitive dependency through `hyper-rustls`, add it as a direct dependency:

```toml
rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }
```

Match the version already resolved in `Cargo.lock` to avoid pulling a different version.

- [ ] **Step 2: Add `#[from]` variant for rustls::Error in error.rs**

Add a new variant in `src/error.rs` after `Utf8` and before `Tls`:

```rust
/// A rustls TLS operation failed.
#[error("rustls error: {0}")]
TlsRustls(#[from] rustls::Error),
```

- [ ] **Step 3: Update builder.rs to use `?` instead of manual wrapping**

In `src/webdav/builder.rs` line 489, the PEM parsing error is currently:

```rust
let cert = cert.map_err(|e| Error::tls("failed to parse PEM certificate", e))?;
```

This can now use `?` directly since `#[from]` provides the conversion:

```rust
let cert = cert?;
```

However, this loses the context string "failed to parse PEM certificate". Since the `rustls::Error` Display output is preserved via `#[error("rustls error: {0}")]`, the context is somewhat redundant. But if the reviewer specifically wanted to keep the context, keep the manual wrapping. For now, use `?` to reduce boilerplate as the review item requests.

Wait — the `cert.map_err(...)` is parsing a PEM certificate via `rustls_pemfile`, which returns `io::Error`, not `rustls::Error`. Let me re-read the code.

Looking at line 489: `rustls_pemfile::certs(&mut pem.as_slice())` returns `Result<CertificateDer<'static>, io::Error>`. The `Error::tls()` call wraps an `io::Error`, not a `rustls::Error`. So `#[from]` for `rustls::Error` doesn't apply here.

The `#[from]` for `rustls::Error` applies to other call sites where rustls errors are wrapped. Search for other `Error::tls(...)` calls that wrap `rustls::Error`.

Looking at `build_rustls_config` in builder.rs: `roots.add(cert)` returns `Result<(), Error>` from rustls, and `ClientConfig::builder()` returns a `ClientConfig`, not a `Result`. The `load_native_certs()` returns a `CertsResult` with `errors: Vec<Error>` where `Error` is `rustls_native_certs::Error`, not `rustls::Error`.

So the `#[from]` for `rustls::Error` would benefit any future rustls error wrapping. Currently there are no direct `rustls::Error` wrapping sites — the `Error::tls("...", e)` calls wrap `io::Error` or `rustls_pemfile::Error`.

Add the `#[from]` variant anyway as it's good practice and enables `?` for any future rustls error handling. No call site changes needed right now.

- [ ] **Step 4: Run clippy, fmt, and tests**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --all-features --locked --test unit_tests`
Run: `cargo test --doc`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/error.rs Cargo.toml Cargo.lock
git commit -m "fix: add #[from] conversion for rustls::Error

Adds Error::TlsRustls(rustls::Error) with automatic From conversion,
enabling ? for rustls errors without manual wrapping. Addresses review
item #6 (MEDIUM)."
```

---

## Task 5: Document `Other` as escape-hatch only (MEDIUM #4)

**Rationale:** `Error::other()` and `Error::with_source()` create `Other { context, source }` — essentially anyhow with extra steps. The review says this is justified for user callbacks but should be clearly documented as escape-hatch only.

**Files:**
- Modify: `src/error.rs` — Enhance docs on `Other` variant and `other()` / `with_source()` methods

- [ ] **Step 1: Update doc comments on Other variant**

In `src/error.rs`, update the `Other` variant doc comment (lines 107-121):

```rust
/// An error returned by user-provided callback code or when wrapping an
/// error that does not fit any other variant.
///
/// # When to use this variant
///
/// This is an **escape-hatch** for cases that do not fit a specific
/// variant — primarily errors from user-provided callbacks. If a new
/// specific failure mode becomes common, prefer adding a dedicated
/// variant over relying on `Other`.
///
/// The `context` string is used for `Display`; the underlying `source` is
/// accessible only via [`std::error::Error::source`]. This intentionally
/// avoids leaking the cause into the `Display` output, but consumers that
/// print errors should walk the source chain to avoid losing information.
#[error("{context}")]
#[non_exhaustive]
Other {
    /// Human-readable context describing where or why the error occurred.
    context: String,
    /// The underlying error, if any.
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
},
```

- [ ] **Step 2: Update doc comments on `other()` and `with_source()` methods**

In `src/error.rs`, update the `other()` method (lines 165-170):

```rust
/// Wrap an error message originating outside the DAV protocol stack.
///
/// This is an **escape-hatch** for errors that do not fit a specific
/// [`Error`] variant. Prefer a dedicated variant when one exists.
///
/// Use [`Error::with_source`] when you have an underlying error to chain.
pub fn other(message: impl Into<String>) -> Self {
```

Update the `with_source()` method (lines 176-184):

```rust
/// Wrap an error with a context message and an underlying source.
///
/// This is an **escape-hatch** for errors that do not fit a specific
/// [`Error`] variant. Prefer a dedicated variant when one exists.
///
/// The context is used for `Display`; the source is returned by
/// [`std::error::Error::source`] so the full error chain is preserved.
pub fn with_source(
    context: impl Into<String>,
    source: impl std::error::Error + Send + Sync + 'static,
) -> Self {
```

- [ ] **Step 3: Run clippy, fmt, and doc tests**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --doc`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/error.rs
git commit -m "docs: document Other variant as escape-hatch only

Clarifies that Error::other() and Error::with_source() are escape-hatch
methods for user callbacks, not a replacement for specific variants.
Addresses review item #4 (MEDIUM)."
```

---

## Task 6: Document `Tls { source: Option }` — when source is None (MEDIUM #5)

**Rationale:** The `Tls` variant has `source: Option<Box<dyn Error>>`. The review asks when a TLS error has no source. The answer is the "no roots" case (when native cert loading finds no roots and we fall back to webpki). Document this.

**Files:**
- Modify: `src/error.rs:92-105` — Update `Tls` variant docs

- [ ] **Step 1: Update Tls variant doc comment**

In `src/error.rs`, update the `Tls` variant doc comment (lines 92-105):

```rust
/// A TLS, certificate, or PKI operation failed.
///
/// Covers PEM parsing errors, rustls configuration failures, and
/// native certificate store errors. The `context` string describes
/// where or why the error occurred; the underlying cause is
/// accessible via `source()`.
///
/// `source` is `None` when the error has no underlying cause — for
/// example, when native certificate loading returns no roots and we
/// fall back to the bundled webpki roots without a specific error.
/// `source` is `Some` when wrapping a concrete error from rustls,
/// `rustls_pemfile`, or `rustls_native_certs`.
#[error("TLS error: {context}")]
#[non_exhaustive]
Tls {
    /// Human-readable context describing the TLS failure.
    context: String,
    /// The underlying error, if any. `None` when the error has no
    /// deeper cause (e.g. "no roots found" without a specific error).
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
},
```

- [ ] **Step 2: Run clippy and doc tests**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --doc`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "docs: document when Tls source is None

Explains that Tls.source is None for errors without an underlying
cause (e.g. no roots found) and Some when wrapping a concrete error.
Addresses review item #5 (MEDIUM)."
```

---

## Task 7: Add `cargo build --examples` to CI (LOW #9)

**Rationale:** `examples/migration.rs` is 228 lines but not build-checked in CI. If it breaks, nobody notices until a user tries to run it.

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add `cargo build --examples` step to CI**

In `.github/workflows/ci.yml`, add a new step after the test step (line 47):

```yaml
      - name: Cargo build examples
        run: cargo build --examples --all-features
```

The full steps section becomes:

```yaml
    steps:
      - name: Checkout sources
        uses: actions/checkout@v7

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          shared-key: stable-rust

      - name: Cargo fmt
        run: cargo fmt --all --check

      - name: Cargo clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Cargo test
        run: cargo test --all-features --locked --test unit_tests

      - name: Cargo build examples
        run: cargo build --examples --all-features
```

- [ ] **Step 2: Verify examples build locally**

Run: `cargo build --examples --all-features`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add cargo build --examples to CI

Ensures examples/migration.rs and any future examples are compiled
in CI. Addresses review item #9 (LOW)."
```

---

## Task 8: Gate legacy deprecated modules behind `legacy` feature (LOW #10)

**Rationale:** The legacy modules in `src/lib.rs:726-756` are deprecated re-exports. They should be behind a `legacy` feature gate so they can be removed in a future major version and don't clutter the default API.

**Files:**
- Modify: `Cargo.toml` — Add `legacy` feature
- Modify: `src/lib.rs:726-756` — Gate modules behind `#[cfg(feature = "legacy")]`

**Interfaces:**
- Produces: A `legacy` Cargo feature that must be explicitly enabled to access the deprecated `client`, `streaming`, `types`, `compression` modules. Default features do not include `legacy`.

- [ ] **Step 1: Add `legacy` feature to Cargo.toml**

Read `Cargo.toml` to see the existing `[features]` section. Add:

```toml
[features]
default = []
legacy = []
```

If there's already a `[features]` section, add `legacy = []` to it.

- [ ] **Step 2: Gate legacy modules in lib.rs**

In `src/lib.rs`, wrap the four deprecated modules (lines 726-756) with `#[cfg(feature = "legacy")]`:

```rust
#[cfg(feature = "legacy")]
#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::caldav::client` directly instead"
)]
pub mod client {
    pub use crate::caldav::client::*;
}

#[cfg(feature = "legacy")]
#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::caldav::streaming` directly instead"
)]
pub mod streaming {
    pub use crate::caldav::streaming::*;
}

#[cfg(feature = "legacy")]
#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::caldav::types` directly instead"
)]
pub mod types {
    pub use crate::caldav::types::*;
}

#[cfg(feature = "legacy")]
#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::common::compression` directly instead"
)]
pub mod compression {
    pub use crate::common::compression::*;
}
```

- [ ] **Step 3: Check for any internal usage of legacy modules**

Search for any uses of the legacy modules within the crate itself (e.g. `use crate::client::...`). If found, update to use the canonical path.

Run: `grep -r "use crate::client::" src/` and `grep -r "use crate::streaming::" src/` etc.

- [ ] **Step 4: Run clippy, fmt, and tests (without legacy feature)**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --all-features --locked --test unit_tests`
Run: `cargo test --doc`
Expected: PASS (legacy modules are compiled with `--all-features`)

- [ ] **Step 5: Run tests with legacy feature explicitly**

Run: `cargo test --features legacy --locked --test unit_tests`
Expected: PASS

- [ ] **Step 6: Run tests without legacy feature (default)**

Run: `cargo test --locked --test unit_tests`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/lib.rs
git commit -m "fix: gate legacy deprecated modules behind legacy feature

The deprecated client, streaming, types, and compression modules are
now behind a legacy Cargo feature, default-off. This keeps the default
API clean and allows removal in a future major version. Addresses
review item #10 (LOW)."
```

---

## Task 9: Final verification and PR update

**Rationale:** Ensure all changes work together and update the PR with a summary of what was addressed.

**Files:**
- None (verification only)

- [ ] **Step 1: Full verification run**

Run all checks in sequence:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --locked --test unit_tests
cargo test --doc
cargo build --examples --all-features
```

All must pass.

- [ ] **Step 2: Check for any remaining `Error::InvalidInput` string-matching in tests**

Run: `grep -r "Error::InvalidInput" tests/`

Any remaining `InvalidInput` matches should be for the catch-all variant only (not for ETag, component, datetime, or config errors). If any test is still string-matching on a now-typed error, update it.

- [ ] **Step 3: Post a summary comment on the PR**

Post a comment on PR #69 summarizing the fixes:

```bash
gh pr comment 69 --repo Goopil/fast-dav-rs --body "## Review fixes applied

All review items addressed:

### HIGH
1. **InvalidInput(String) string-matching** — Added structured variants: \`InvalidEtag\` (with \`EtagReason\` enum), \`InvalidComponentName\`, \`InvalidDateTime\`, \`InvalidConfig\`. \`InvalidInput(String)\` remains as catch-all.
2. **UnexpectedStatus allocates on every error** — Replaced \`operation: String\` with \`#[non_exhaustive] Operation\` enum carrying \`&'static str\` via \`Display\`. No allocation in error path.
3. **Struct variants not #[non_exhaustive]** — Added \`#[non_exhaustive]\` to all struct variants.

### MEDIUM
4. **Other is an anyhow-like backdoor** — Documented as escape-hatch only.
5. **Tls source: Option** — Documented when source is None (no-roots fallback).
6. **No #[from] for rustls::Error** — Added \`Error::TlsRustls(#[from] rustls::Error)\`.
7. **from_quick_xml stringifies** — No action needed, already documented.

### LOW
8. **Scope creep** — Acknowledged.
9. **examples/migration.rs not build-checked** — Added \`cargo build --examples\` to CI.
10. **Legacy deprecated modules** — Gated behind \`legacy\` feature (default-off).
"
```

- [ ] **Step 4: Push changes**

Push all commits to the `feat/typed-thiserrors` branch.

---

## Self-Review

### Spec Coverage

| Review Item | Task(s) | Status |
|---|---|---|
| #1 HIGH: InvalidInput string-matching | Task 3 | Covered |
| #2 HIGH: UnexpectedStatus allocation | Task 2 | Covered |
| #3 HIGH: Struct variants not non_exhaustive | Task 1 | Covered |
| #4 MEDIUM: Other is anyhow backdoor | Task 5 | Covered (docs) |
| #5 MEDIUM: Tls source Option | Task 6 | Covered (docs) |
| #6 MEDIUM: No #[from] for rustls | Task 4 | Covered |
| #7 MEDIUM: from_quick_xml stringifies | — | No action (already documented) |
| #8 LOW: Scope creep | — | No code action (PR reply only) |
| #9 LOW: Examples not build-checked | Task 7 | Covered |
| #10 LOW: Legacy modules | Task 8 | Covered |

### Placeholder Scan

No placeholders found. All steps contain concrete code or shell commands.

### Type Consistency

- `Operation` enum defined in Task 2, used in Tasks 2 (call sites) and 3 (tests)
- `EtagReason` enum defined in Task 3, used in Task 3 (call sites and tests)
- `InvalidEtag`, `InvalidComponentName`, `InvalidDateTime`, `InvalidConfig` defined in Task 3, used in Task 3
- All `#[non_exhaustive]` additions in Task 1 are consistent with construction patterns using `..` in tests

### Dependency Order

Tasks 1-3 are independent of each other in terms of types defined, but they all modify `src/error.rs`. They should be executed sequentially to avoid merge conflicts. Task 1 (non_exhaustive) should come first as it's the simplest. Task 2 (Operation enum) second. Task 3 (InvalidInput variants) third. Tasks 4-8 are independent and can be done in any order after Task 3. Task 9 (final verification) must be last.

**Recommended execution order:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9
