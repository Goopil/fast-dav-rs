# Normalize ETag and Sync-Token at Read Time Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Strip surrounding double quotes from ETag and Sync-Token values at read time (HTTP headers + XML parsing) so internal storage is always quote-free; the existing 0.5.0 write-time quoting in `if_match_header_value` re-adds them on the wire.

**Architecture:** Add two `pub(crate)` normalization functions in `src/webdav/client.rs`: `normalize_etag` (strips `"` from strong and weak entity-tags, preserving the `W/` prefix) and `normalize_sync_token` (strips `"`). Apply them at every read path: `etag_from_headers`, the shared XML parser `CommonParser::on_text` (for `<D:getetag>` and per-item `<D:sync-token>`), the CalDAV/CardDAV streaming parsers (top-level `<D:sync-token>`), and `map_sync_response` (for the `Sync-Token` header). Update `if_match_header_value` to accept bare weak etags (`W/value`). Update all existing tests whose etag assertions expect quoted values.

**Tech Stack:** Rust, hyper 1.x, quick-xml, anyhow, tokio. Unit tests in `tests/unit/`.

## Global Constraints

- Only strip **double quotes** (`"`). Single quotes (`'`) and angle brackets (`<>`) are NOT stripped — RFC 7232 entity-tags use only `"`.
- The `W/` weak-etag prefix is preserved: `W/"abc"` normalizes to `W/abc`.
- Empty etags after normalization return `None` from `etag_from_headers` (filter out).
- `if_match_header_value` must still accept all inputs it currently accepts (quoted, `*`, bare strong) and additionally accept bare weak (`W/value`).
- All public APIs keep the same signatures — only the returned values change (no quotes).
- `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, `cargo test --doc` must all pass.
- No comments in code unless explicitly requested by existing patterns.
- E2E tests check only `.is_some()` / `.is_some()` on etags — no value assertions, so they need no changes.

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `src/webdav/client.rs` | `normalize_etag`, `normalize_sync_token` helpers; `etag_from_headers`; `if_match_header_value` | Modify |
| `src/webdav/streaming.rs` | Shared XML parser — `<D:getetag>`, per-item `<D:sync-token>` | Modify |
| `src/caldav/streaming.rs` | CalDAV XML parser — top-level `<D:sync-token>` | Modify |
| `src/carddav/streaming.rs` | CardDAV XML parser — top-level `<D:sync-token>` | Modify |
| `src/caldav/client.rs` | `map_sync_response` — `Sync-Token` header | Modify |
| `src/carddav/client.rs` | `map_sync_response` — `Sync-Token` header | Modify |
| `tests/unit/caldav/etag_tests.rs` | ETag header extraction + conditional operations tests | Modify |
| `tests/unit/carddav/etag_tests.rs` | ETag header extraction + conditional operations tests | Modify |
| `tests/unit/caldav/parser_tests.rs` | XML parser tests with etag assertions | Modify |
| `tests/unit/carddav/parser_tests.rs` | XML parser tests with etag assertions | Modify |
| `tests/unit/caldav/streaming_tests.rs` | Streaming parser tests (etag comparison) | Modify |
| `tests/unit/carddav/streaming_tests.rs` | Streaming parser tests (etag comparison) | Modify |
| `tests/unit/caldav/caldav_helpers.rs` | Helper tests with etag XML | Modify |
| `tests/unit/carddav/carddav_helpers.rs` | Helper tests with etag XML | Modify |
| `tests/unit/caldav/client_tests.rs` | Client tests with etag assertions in `map_*` functions | Modify |
| `tests/unit/carddav/client_tests.rs` | Client tests with etag assertions in `map_*` functions | Modify |
| `tests/unit/caldav/parser_edge_cases.rs` | Performance test — etag in XML, no value assertion | No change needed |
| `tests/unit/carddav/parser_edge_cases.rs` | Performance/malformed test — etag in XML, no value assertion | No change needed |

---

### Task 1: Add `normalize_etag` and `normalize_sync_token` functions

**Files:**
- Modify: `src/webdav/client.rs` (insert after `is_etag_character` at line 78)

**Interfaces:**
- Consumes: nothing
- Produces: `pub(crate) fn normalize_etag(etag: &str) -> String` and `pub(crate) fn normalize_sync_token(token: &str) -> String`

- [ ] **Step 1: Write the failing test**

Add a `#[cfg(test)]` module at the end of `src/webdav/client.rs` (after the last impl block, before EOF):

```rust
#[cfg(test)]
mod tests {
    use super::{normalize_etag, normalize_sync_token};

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
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --all-features normalize_etag`
Expected: FAIL — `normalize_etag` and `normalize_sync_token` do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add to `src/webdav/client.rs` after `is_etag_character` (after line 78):

```rust
pub(crate) fn normalize_etag(etag: &str) -> String {
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

pub(crate) fn normalize_sync_token(token: &str) -> String {
    token.trim().trim_matches('"').to_string()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --all-features normalize_etag`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/webdav/client.rs
git commit -m "feat: add normalize_etag and normalize_sync_token helpers"
```

---

### Task 2: Apply `normalize_etag` in `etag_from_headers`

**Files:**
- Modify: `src/webdav/client.rs:727-733`
- Modify: `tests/unit/caldav/etag_tests.rs:44-88`
- Modify: `tests/unit/carddav/etag_tests.rs:44-88`

**Interfaces:**
- Consumes: `normalize_etag` from Task 1
- Produces: `etag_from_headers` returns quote-free `Option<String>`

- [ ] **Step 1: Write the failing test (update existing tests)**

In `tests/unit/caldav/etag_tests.rs`, update these test assertions:

Line 49 — `test_etag_from_headers_present`:
```rust
// Before:
assert_eq!(etag, Some("\"abc123\"".to_string()));
// After:
assert_eq!(etag, Some("abc123".to_string()));
```

Line 78 — `test_etag_from_headers_multiple_values`:
```rust
// Before:
assert_eq!(etag, Some("\"first\"".to_string()));
// After:
assert_eq!(etag, Some("first".to_string()));
```

Line 87 — `test_etag_from_headers_weak_etag`:
```rust
// Before:
assert_eq!(etag, Some("W/\"weak123\"".to_string()));
// After:
assert_eq!(etag, Some("W/weak123".to_string()));
```

Apply the **exact same changes** to `tests/unit/carddav/etag_tests.rs` (same line numbers, same assertions).

Also add a new test to **both** `etag_tests.rs` files (after `test_etag_from_headers_weak_etag`):

```rust
#[test]
fn test_etag_from_headers_strips_quotes_and_returns_none_if_empty() {
    let mut headers = HeaderMap::new();
    headers.insert("ETag", HeaderValue::from_static("\"\""));
    let etag = CalDavClient::etag_from_headers(&headers);
    assert_eq!(etag, None);
}
```

(Use `CardDavClient` in the carddav test file.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --all-features --test unit_tests caldav::etag_tests`
Expected: FAIL — assertions expect quoted values, but `etag_from_headers` still returns raw.

- [ ] **Step 3: Write minimal implementation**

In `src/webdav/client.rs:727-733`, change `etag_from_headers`:

```rust
/// Extract the `ETag` from a response header map, if present.
///
/// The returned value is **normalized**: surrounding double quotes are stripped,
/// so `"abc"` becomes `abc` and `W/"abc"` becomes `W/abc`.
/// Use the value directly with `put_if_match` / `delete_if_match`, which
/// re-adds the quoting on the wire.
pub fn etag_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(normalize_etag)
        .filter(|s| !s.is_empty())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --all-features --test unit_tests caldav::etag_tests`
Run: `cargo test --all-features --test unit_tests carddav::etag_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/webdav/client.rs tests/unit/caldav/etag_tests.rs tests/unit/carddav/etag_tests.rs
git commit -m "feat: normalize etag_from_headers to strip double quotes"
```

---

### Task 3: Apply `normalize_etag` in the shared XML parser

**Files:**
- Modify: `src/webdav/streaming.rs:1` (add import), `src/webdav/streaming.rs:153`
- Modify: `tests/unit/caldav/parser_tests.rs:64,114`
- Modify: `tests/unit/carddav/parser_tests.rs:68,115`

**Interfaces:**
- Consumes: `normalize_etag` from Task 1
- Produces: `DavItemCommon.etag` is always quote-free when parsed from XML

- [ ] **Step 1: Write the failing test (update existing tests)**

In `tests/unit/caldav/parser_tests.rs`:

Line 64 — `parse_multistatus_extracts_calendar_properties`:
```rust
// Before:
assert_eq!(calendar.etag.as_deref(), Some("\"etag-123\""));
// After:
assert_eq!(calendar.etag.as_deref(), Some("etag-123"));
```

Line 114 — `parse_multistatus_extracts_common_properties_and_top_level_sync_token`:
```rust
// Before:
assert_eq!(item.etag.as_deref(), Some("\"etag-999\""));
// After:
assert_eq!(item.etag.as_deref(), Some("etag-999"));
```

In `tests/unit/carddav/parser_tests.rs`:

Line 68 — `parse_multistatus_extracts_addressbook_properties`:
```rust
// Before:
assert_eq!(book.etag.as_deref(), Some("\"etag-123\""));
// After:
assert_eq!(book.etag.as_deref(), Some("etag-123"));
```

Line 115 — `parse_multistatus_extracts_common_properties_and_top_level_sync_token`:
```rust
// Before:
assert_eq!(item.etag.as_deref(), Some("\"etag-777\""));
// After:
assert_eq!(item.etag.as_deref(), Some("etag-777"));
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --all-features --test unit_tests caldav::parser_tests`
Run: `cargo test --all-features --test unit_tests carddav::parser_tests`
Expected: FAIL — assertions expect unquoted, but parser still returns quoted.

- [ ] **Step 3: Write minimal implementation**

In `src/webdav/streaming.rs`, add import at line 1:

```rust
use crate::webdav::client::normalize_etag;
use crate::webdav::types::DavItemCommon;
use anyhow::{Result, anyhow};
```

(Replace the existing first two lines to add the `normalize_etag` import.)

At line 153, change:
```rust
// Before:
self.current.etag = Some(trimmed.to_string());
// After:
self.current.etag = Some(normalize_etag(trimmed));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --all-features --test unit_tests caldav::parser_tests`
Run: `cargo test --all-features --test unit_tests carddav::parser_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/webdav/streaming.rs tests/unit/caldav/parser_tests.rs tests/unit/carddav/parser_tests.rs
git commit -m "feat: normalize etag in shared XML parser"
```

---

### Task 4: Apply `normalize_sync_token` in XML parsers (shared + CalDAV + CardDAV)

**Files:**
- Modify: `src/webdav/streaming.rs:160` (per-item sync-token)
- Modify: `src/caldav/streaming.rs:307` (top-level sync-token)
- Modify: `src/carddav/streaming.rs:308` (top-level sync-token)
- Modify: `tests/unit/caldav/parser_tests.rs` (add new test)
- Modify: `tests/unit/carddav/parser_tests.rs` (add new test)

**Interfaces:**
- Consumes: `normalize_sync_token` from Task 1
- Produces: `DavItemCommon.sync_token` and top-level `ParseResult.sync_token` are always quote-free

- [ ] **Step 1: Write the failing test**

Add a new test to `tests/unit/caldav/parser_tests.rs` (at end of file):

```rust
#[test]
fn parse_multistatus_normalizes_quoted_sync_token() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:sync-token>"http://example.com/sync/99"</D:sync-token>
  <D:response>
    <D:href>/dav/user01/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:sync-token>"item-token-quoted"</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;
    let result = parse_multistatus_bytes(xml.as_bytes()).expect("xml parsing succeeds");
    assert_eq!(result.sync_token.as_deref(), Some("http://example.com/sync/99"));
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].sync_token.as_deref(),
        Some("item-token-quoted")
    );
}
```

Add the equivalent test to `tests/unit/carddav/parser_tests.rs` (at end of file, using `fast_dav_rs::carddav::parse_multistatus_bytes`):

```rust
#[test]
fn parse_multistatus_normalizes_quoted_sync_token() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:sync-token>"http://example.com/sync/99"</D:sync-token>
  <D:response>
    <D:href>/dav/user01/ab/</D:href>
    <D:propstat>
      <D:prop>
        <D:sync-token>"item-token-quoted"</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;
    let result = parse_multistatus_bytes(xml.as_bytes()).expect("xml parsing succeeds");
    assert_eq!(result.sync_token.as_deref(), Some("http://example.com/sync/99"));
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].sync_token.as_deref(),
        Some("item-token-quoted")
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --all-features --test unit_tests caldav::parser_tests::parse_multistatus_normalizes_quoted_sync_token`
Run: `cargo test --all-features --test unit_tests carddav::parser_tests::parse_multistatus_normalizes_quoted_sync_token`
Expected: FAIL — sync tokens still contain quotes.

- [ ] **Step 3: Write minimal implementation**

In `src/webdav/streaming.rs`, update the import (add `normalize_sync_token`):

```rust
use crate::webdav::client::{normalize_etag, normalize_sync_token};
```

At line 160, change:
```rust
// Before:
self.current.sync_token = Some(trimmed.to_string());
// After:
self.current.sync_token = Some(normalize_sync_token(trimmed));
```

In `src/caldav/streaming.rs`, add import at top of file (after line 3):

```rust
use crate::webdav::client::normalize_sync_token;
```

At line 307, change:
```rust
// Before:
self.sync_token = Some(trimmed.to_string());
// After:
self.sync_token = Some(normalize_sync_token(trimmed));
```

In `src/carddav/streaming.rs`, add import at top of file (after line 3):

```rust
use crate::webdav::client::normalize_sync_token;
```

At line 308, change:
```rust
// Before:
self.sync_token = Some(trimmed.to_string());
// After:
self.sync_token = Some(normalize_sync_token(trimmed));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --all-features --test unit_tests caldav::parser_tests`
Run: `cargo test --all-features --test unit_tests carddav::parser_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/webdav/streaming.rs src/caldav/streaming.rs src/carddav/streaming.rs tests/unit/caldav/parser_tests.rs tests/unit/carddav/parser_tests.rs
git commit -m "feat: normalize sync-token in XML parsers"
```

---

### Task 5: Apply `normalize_sync_token` in `map_sync_response` (CalDAV + CardDAV)

**Files:**
- Modify: `src/caldav/client.rs:766-768`
- Modify: `src/carddav/client.rs:838-840`

**Interfaces:**
- Consumes: `normalize_sync_token` from Task 1
- Produces: `SyncResponse.sync_token` from headers is always quote-free

- [ ] **Step 1: Write the failing test**

Add a test to `tests/unit/caldav/caldav_helpers.rs` (at end of file):

```rust
#[test]
fn map_sync_response_normalizes_quoted_header_token() {
    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", r#""quoted-header-token""#.parse().unwrap());
    let sync = map_sync_response(&headers, Vec::new(), None);
    assert_eq!(sync.sync_token.as_deref(), Some("quoted-header-token"));
}
```

Add the equivalent test to `tests/unit/carddav/carddav_helpers.rs` (at end of file, using `fast_dav_rs::carddav::client::map_sync_response`):

```rust
#[test]
fn map_sync_response_normalizes_quoted_header_token() {
    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", r#""quoted-header-token""#.parse().unwrap());
    let sync = map_sync_response(&headers, Vec::new(), None);
    assert_eq!(sync.sync_token.as_deref(), Some("quoted-header-token"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --all-features --test unit_tests caldav::caldav_helpers::map_sync_response_normalizes_quoted_header_token`
Run: `cargo test --all-features --test unit_tests carddav::carddav_helpers::map_sync_response_normalizes_quoted_header_token`
Expected: FAIL — header token still contains quotes.

- [ ] **Step 3: Write minimal implementation**

In `src/caldav/client.rs`, add import (near top of file, after existing webdav imports):

```rust
use crate::webdav::client::normalize_sync_token;
```

At lines 766-768, change:
```rust
// Before:
headers
    .get("Sync-Token")
    .and_then(|v| v.to_str().ok())
    .map(|s| s.to_string())
// After:
headers
    .get("Sync-Token")
    .and_then(|v| v.to_str().ok())
    .map(normalize_sync_token)
```

In `src/carddav/client.rs`, add import (near top of file, after existing webdav imports):

```rust
use crate::webdav::client::normalize_sync_token;
```

At lines 838-840, change:
```rust
// Before:
headers
    .get("Sync-Token")
    .and_then(|v| v.to_str().ok())
    .map(|s| s.to_string())
// After:
headers
    .get("Sync-Token")
    .and_then(|v| v.to_str().ok())
    .map(normalize_sync_token)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --all-features --test unit_tests caldav::caldav_helpers`
Run: `cargo test --all-features --test unit_tests carddav::carddav_helpers`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/caldav/client.rs src/carddav/client.rs tests/unit/caldav/caldav_helpers.rs tests/unit/carddav/carddav_helpers.rs
git commit -m "feat: normalize sync-token header in map_sync_response"
```

---

### Task 6: Update `if_match_header_value` to accept bare weak etags

**Files:**
- Modify: `src/webdav/client.rs:46-66`
- Modify: `tests/unit/caldav/etag_tests.rs:91-132`
- Modify: `tests/unit/carddav/etag_tests.rs:91-132`

**Interfaces:**
- Consumes: nothing new
- Produces: `if_match_header_value` accepts `W/value` (bare weak) and produces `W/"value"`

- [ ] **Step 1: Write the failing test (update existing tests)**

In `tests/unit/caldav/etag_tests.rs`, update `test_conditional_operations_normalize_if_match` (line 92-97):

```rust
// Before:
for (etag, expected) in [
    ("  abc  ", "\"abc\""),
    ("\"abc\"", "\"abc\""),
    ("W/\"abc\"", "W/\"abc\""),
    ("*", "*"),
]
// After:
for (etag, expected) in [
    ("  abc  ", "\"abc\""),
    ("\"abc\"", "\"abc\""),
    ("W/\"abc\"", "W/\"abc\""),
    ("W/abc", "W/\"abc\""),
    ("*", "*"),
]
```

In `test_conditional_operations_reject_invalid_etags_before_request` (line 123), **remove** `"W/abc"` from the invalid list:

```rust
// Before:
for etag in ["", "   ", "\"abc", "W/abc", "abc\ndef"]
// After:
for etag in ["", "   ", "\"abc", "abc\ndef"]
```

Apply the **exact same changes** to `tests/unit/carddav/etag_tests.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --all-features --test unit_tests caldav::etag_tests::test_conditional_operations_normalize_if_match`
Expected: FAIL — `W/abc` is currently rejected.

Run: `cargo test --all-features --test unit_tests caldav::etag_tests::test_conditional_operations_reject_invalid_etags_before_request`
Expected: FAIL — `W/abc` no longer in the rejection list but is still rejected by current code.

- [ ] **Step 3: Write minimal implementation**

In `src/webdav/client.rs`, replace `if_match_header_value` (lines 46-66) with:

```rust
pub(crate) fn if_match_header_value(etag: &str) -> Result<header::HeaderValue> {
    let etag = etag.trim();
    if etag.is_empty() {
        return Err(anyhow!("ETag cannot be empty"));
    }

    if etag == "*" || is_valid_entity_tag(etag) {
        return header::HeaderValue::from_str(etag)
            .map_err(|err| anyhow!("ETag cannot be used as an If-Match header: {err}"));
    }

    if let Some(opaque) = etag.strip_prefix("W/") {
        validate_opaque_tag(opaque)?;
        let value = format!("W/\"{opaque}\"");
        return header::HeaderValue::from_str(&value)
            .map_err(|err| anyhow!("ETag cannot be used as an If-Match header: {err}"));
    }

    validate_opaque_tag(etag)?;
    let value = format!("\"{etag}\"");
    header::HeaderValue::from_str(&value)
        .map_err(|err| anyhow!("ETag cannot be used as an If-Match header: {err}"))
}

fn validate_opaque_tag(opaque: &str) -> Result<()> {
    if opaque.is_empty() || opaque.contains('"') {
        return Err(anyhow!("ETag has an invalid entity-tag format"));
    }
    if !opaque.bytes().all(is_etag_character) {
        return Err(anyhow!("ETag contains invalid entity-tag characters"));
    }
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --all-features --test unit_tests caldav::etag_tests`
Run: `cargo test --all-features --test unit_tests carddav::etag_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/webdav/client.rs tests/unit/caldav/etag_tests.rs tests/unit/carddav/etag_tests.rs
git commit -m "feat: accept bare weak etags in if_match_header_value"
```

---

### Task 7: Update helper and client tests with etag assertions

**Files:**
- Modify: `tests/unit/caldav/caldav_helpers.rs` (XML contains `"cal-etag"`, `"1234-1"`, `"1234-2"` — no explicit etag value assertions in `maps_caldav_multistatus_structures`, but etag values flow through `map_calendar_list` and `map_calendar_objects`)
- Modify: `tests/unit/carddav/carddav_helpers.rs` (XML contains `"ab-etag"`, `"1234-1"`, `"1234-2"`)
- Modify: `tests/unit/caldav/client_tests.rs:242,248,265,288` (etag assertions in `test_map_calendar_objects` and `test_map_sync_response`)
- Modify: `tests/unit/carddav/client_tests.rs:237,243,260,283` (etag assertions in `test_map_address_objects` and `test_map_sync_response`)

**Interfaces:**
- Consumes: normalized etags from Tasks 2-3
- Produces: all tests pass with quote-free etag assertions

- [ ] **Step 1: Write the failing test (update existing test assertions)**

In `tests/unit/caldav/client_tests.rs`:

Line 242 (`test_map_calendar_objects`):
```rust
// Before:
assert_eq!(objects[0].etag, Some("\"abc123\"".to_string()));
// After:
assert_eq!(objects[0].etag, Some("abc123".to_string()));
```

Line 248:
```rust
// Before:
assert_eq!(objects[1].etag, Some("\"def456\"".to_string()));
// After:
assert_eq!(objects[1].etag, Some("def456".to_string()));
```

Line 265 (`test_map_sync_response`):
```rust
// Before:
item1.etag = Some("\"abc123\"".to_string());
// After:
item1.etag = Some("abc123".to_string());
```

Line 288:
```rust
// Before:
assert_eq!(response.items[0].etag, Some("\"abc123\"".to_string()));
// After:
assert_eq!(response.items[0].etag, Some("abc123".to_string()));
```

In `tests/unit/carddav/client_tests.rs`:

Line 237 (`test_map_address_objects`):
```rust
// Before:
assert_eq!(objects[0].etag, Some("\"abc123\"".to_string()));
// After:
assert_eq!(objects[0].etag, Some("abc123".to_string()));
```

Line 243:
```rust
// Before:
assert_eq!(objects[1].etag, Some("\"def456\"".to_string()));
// After:
assert_eq!(objects[1].etag, Some("def456".to_string()));
```

Line 260 (`test_map_sync_response`):
```rust
// Before:
item1.etag = Some("\"abc123\"".to_string());
// After:
item1.etag = Some("abc123".to_string());
```

Line 283:
```rust
// Before:
assert_eq!(response.items[0].etag, Some("\"abc123\"".to_string()));
// After:
assert_eq!(response.items[0].etag, Some("abc123".to_string()));
```

Note: These client tests construct `DavItem` structs manually and pass them through `map_calendar_objects` / `map_address_objects` / `map_sync_response`. Since those mapping functions pass `etag` through unchanged, the test data itself must be quote-free to simulate what the parser would now produce.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --all-features --test unit_tests caldav::client_tests`
Run: `cargo test --all-features --test unit_tests carddav::client_tests`
Expected: FAIL — assertions now expect unquoted, but test data still has quotes (for the map_sync_response tests that set etag directly) or assertions still have quotes (for map_calendar_objects tests).

- [ ] **Step 3: No implementation needed — these are test-only changes**

The production code changes were already made in Tasks 2-3. The `map_*` functions pass etag through unchanged — the normalization happens at parse time, not at map time. The client tests that manually construct `DavItem` with etags should use quote-free values to match what the parser now produces.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --all-features --test unit_tests caldav::client_tests`
Run: `cargo test --all-features --test unit_tests carddav::client_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/unit/caldav/client_tests.rs tests/unit/carddav/client_tests.rs
git commit -m "test: update client test etag assertions for normalized values"
```

---

### Task 8: Run full verification suite

**Files:**
- No file changes

- [ ] **Step 1: Run formatting**

Run: `cargo fmt`
Expected: no changes (or auto-format)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Run all tests**

Run: `cargo test --all-features`
Expected: all tests pass

- [ ] **Step 4: Run doc tests**

Run: `cargo test --doc`
Expected: all doc tests pass

- [ ] **Step 5: Commit if any formatting changes**

```bash
git add -A
git commit -m "style: format after etag normalization"  # only if fmt made changes
```

---

## Self-Review

### 1. Spec coverage

| Requirement | Task |
|---|---|
| Strip double quotes from ETag at read (headers) | Task 2 |
| Strip double quotes from ETag at read (XML) | Task 3 |
| Strip double quotes from Sync-Token at read (XML, per-item) | Task 4 |
| Strip double quotes from Sync-Token at read (XML, top-level CalDAV) | Task 4 |
| Strip double quotes from Sync-Token at read (XML, top-level CardDAV) | Task 4 |
| Strip double quotes from Sync-Token at read (HTTP header) | Task 5 |
| `if_match_header_value` accepts bare weak etags | Task 6 |
| All existing tests updated | Tasks 2, 3, 6, 7 |
| `cargo fmt` / `clippy` / `test --all-features` / `test --doc` pass | Task 8 |

No gaps identified.

### 2. Placeholder scan

No TBD, TODO, "implement later", "add appropriate error handling", or "similar to Task N" found. All steps contain actual code.

### 3. Type consistency

- `normalize_etag(&str) -> String` — defined Task 1, used Tasks 2, 3. ✓
- `normalize_sync_token(&str) -> String` — defined Task 1, used Tasks 4, 5. ✓
- `etag_from_headers(&HeaderMap) -> Option<String>` — same signature, only return values change. ✓
- `if_match_header_value(&str) -> Result<header::HeaderValue>` — same signature, internal logic changes. ✓
- `validate_opaque_tag(&str) -> Result<()>` — defined Task 6, used Task 6. ✓
- `DavItemCommon.etag: Option<String>` — unchanged type, values now quote-free. ✓
- `DavItemCommon.sync_token: Option<String>` — unchanged type, values now quote-free. ✓
