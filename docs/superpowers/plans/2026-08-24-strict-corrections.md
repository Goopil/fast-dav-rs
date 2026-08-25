# Strict Correction Plan — Adversarial Review Findings

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all 15 findings from the adversarial code review (1 HIGH, 6 MEDIUM, 8 LOW).

**Architecture:** Each fix is surgical — correcting comments, adding tests, enriching error context, and hardening CI. No architectural changes.

**Tech Stack:** Rust, thiserror, hyper, rustls, quick-xml, GitHub Actions

## Global Constraints

- `cargo fmt --all --check` must pass
- `cargo clippy --all-targets --all-features -- -D warnings` must pass
- `cargo nextest run --all-features --locked --test unit_tests` must pass
- `cargo test --doc --all-features` must pass
- `cargo build --examples --all-features` must pass
- No comments in code unless explicitly requested
- Follow existing naming conventions

---

## Finding Summary

| # | Severity | Finding | Action |
|---|----------|---------|--------|
| 1 | HIGH | Migration example & README claim `#[from]` but code uses `#[source]` + `.map_err()` | Fix comments to explain the trade-off correctly |
| 2 | MEDIUM | `InvalidInput(String)` is dead code — nothing constructs it | Document as escape-hatch for external code |
| 3 | MEDIUM | `validate_utc_datetime` loses start vs end context | Use `context` parameter in error reason |
| 4 | MEDIUM | `Tls { source: None }` documented but never constructed | Fix docs to describe reality |
| 5 | MEDIUM | TLS errors split across `TlsRustls` and `Tls` without guidance | Add doc note to match both |
| 6 | MEDIUM | Breaking module path changes not in migration guide | Add section to README |
| 7 | LOW | `validate_component_name` doesn't report which character was invalid | Include the invalid char in reason |
| 8 | LOW | `from_quick_xml` stringifies Syntax/IllFormed | Already documented — no action |
| 9 | LOW | Builder tests only assert `is_err()` without checking error type | Add type assertions |
| 10 | LOW | Missing `#[from]` conversion test for `TlsRustls` | Add test |
| 11 | LOW | Connection test on port 1 is fragile | Already documented — no action |
| 12 | LOW | Proxy control-char tests only cover `\n` | Add tests for other control chars |
| 13 | LOW | `examples/` not in `include` of Cargo.toml | Add `examples/` to include |
| 14 | LOW | CI doesn't use `--locked` for examples build and doc tests | Add `--locked` |

---

## Task 1: Fix migration example and README `#[from]` claims (HIGH #1)

**Rationale:** The migration example and README "Key patterns at a glance" section claim `#[from]` generates `From` impls, but the actual `ConfigError::InvalidPort` variant has an extra `raw: String` field that `#[from]` cannot populate. The code correctly uses `.map_err()` with manual construction, but the comments contradict the code. Users following this guide will be confused.

The correct explanation: `#[from]` works for newtype/tuple variants (no extra fields) — it generates `From<E>` automatically. For struct variants with extra context fields (like `raw: String`), you must use `#[source]` and `.map_err()` to populate the extra fields manually.

**Files:**
- Modify: `examples/migration.rs` lines 49-73 (section 2 comments)
- Modify: `README.md` lines 361-387 ("Key patterns at a glance" section)

- [ ] **Step 1: Fix migration.rs section 2 comments**

In `examples/migration.rs`, replace the section 2 comment block (lines 49-73):

```rust
// ──────────────────────────────────────────────────────────────────────────
// 2. Using `?` with automatic conversions
// ──────────────────────────────────────────────────────────────────────────

// BEFORE — `?` works because `anyhow::Error: From<E> for E: std::error::Error`.
// But the conversion is *opaque*: the caller can't distinguish error types.
//
// ```ignore
// fn parse_config(raw: &str) -> Result<u16> {
//     let port: u16 = raw.parse()?;          // anyhow converts ParseIntError
//     Ok(port)
// }
// ```

// AFTER — For tuple/newtype variants (no extra fields), `#[from]` generates
// a `From<E>` impl so `?` converts automatically. But `InvalidPort` has an
// extra `raw: String` field that `#[from]` cannot populate — so we use
// `#[source]` and `.map_err()` to construct the variant with the input value.
//
// If the variant had NO extra fields, it would look like this:
//
// ```ignore
// #[derive(Debug, thiserror::Error)]
// pub enum SimpleError {
//     #[error("parse failed: {0}")]
//     Parse(#[from] ParseIntError),  // ? works: From<ParseIntError> auto-generated
// }
// fn parse(raw: &str) -> Result<u16, SimpleError> {
//     let port: u16 = raw.parse()?;  // ParseIntError -> SimpleError::Parse via #[from]
//     Ok(port)
// }
// ```

fn parse_port(raw: &str) -> ConfigResult<u16> {
    let port: u16 = raw.parse().map_err(|source| ConfigError::InvalidPort {
        raw: raw.to_owned(),
        source,
    })?;
    Ok(port)
}
```

- [ ] **Step 2: Fix README "Key patterns at a glance" section**

In `README.md`, replace lines 361-387:

```rust
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
```

- [ ] **Step 3: Verify examples and doc tests**

Run: `cargo build --examples --all-features`
Run: `cargo test --doc --all-features`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add examples/migration.rs README.md
git commit -m "fix: correct #[from] vs #[source] explanation in migration docs

The migration example and README claimed #[from] generates From impls
for struct variants with extra fields, but #[from] only works for
newtype/tuple variants. Struct variants need #[source] + .map_err().
Clarifies the trade-off with examples of both patterns. Fixes HIGH #1."
```

---

## Task 2: Document `InvalidInput(String)` as escape-hatch (MEDIUM #2)

**Rationale:** `InvalidInput(String)` is never constructed by the library but remains public. It should be documented as an escape-hatch for external code, similar to `Other`.

**Files:**
- Modify: `src/error.rs:51-53` — Update doc comment on `InvalidInput`

- [ ] **Step 1: Update InvalidInput doc comment**

In `src/error.rs`, replace the `InvalidInput` variant doc (lines 51-53):

```rust
    /// A caller-provided value failed validation.
    ///
    /// This is a **catch-all** variant for caller-side validation errors that
    /// don't fit a more specific variant. The library itself uses
    /// [`InvalidEtag`](Self::InvalidEtag), [`InvalidComponentName`](Self::InvalidComponentName),
    /// [`InvalidDateTime`](Self::InvalidDateTime), and [`InvalidConfig`](Self::InvalidConfig)
    /// for known validation cases. This variant is kept for external code
    /// that needs to return a validation error without a dedicated variant.
    #[error("invalid input: {0}")]
    InvalidInput(String),
```

- [ ] **Step 2: Verify**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo test --doc --all-features`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "docs: document InvalidInput as external escape-hatch

InvalidInput(String) is never constructed by the library but kept for
external code that needs a validation catch-all. Clarifies when to use
it vs the specific variants. Fixes MEDIUM #2."
```

---

## Task 3: Preserve start/end context in `validate_utc_datetime` (MEDIUM #3)

**Rationale:** The `_context` parameter is unused. When both start and end are invalid, the user can't tell which one failed. The fix uses the context in the error by changing `reason` from `&'static str` to `String` so it can include the context.

Wait — `InvalidDateTime` has `reason: &'static str`. To include dynamic context, we'd need to change the type to `String`. That's a breaking change to the variant's field type. Since the variant is `#[non_exhaustive]`, adding a field is fine, but changing a field type is breaking.

Alternative: Add a `context: String` field to `InvalidDateTime`. Since it's `#[non_exhaustive]`, this is additive. The constructor `invalid_datetime` already takes `reason: &'static str` — we'd add a `context` parameter.

Better alternative: Keep `reason: &'static str` but add a `context: String` field. Update the constructor. Update call sites to pass context. This preserves backward compat on the `reason` field.

**Files:**
- Modify: `src/error.rs:77-85` — Add `context` field to `InvalidDateTime`
- Modify: `src/error.rs:328-333` — Update `invalid_datetime` constructor
- Modify: `src/webdav/xml.rs:59-72` — Use `context` parameter in error
- Modify: `src/caldav/client.rs:496,499` — Already passing context strings
- Modify: `tests/unit/common/error_tests.rs` — Update test if needed
- Modify: `tests/unit/caldav/client_tests.rs` — Update test if needed

- [ ] **Step 1: Add `context` field to `InvalidDateTime`**

In `src/error.rs`, update the `InvalidDateTime` variant:

```rust
    /// A date-time value did not match the expected iCalendar UTC format.
    #[error("{context}: invalid UTC date-time `{value}`: {reason}")]
    #[non_exhaustive]
    InvalidDateTime {
        /// Where the invalid date-time was encountered (e.g. "calendar-query start").
        context: String,
        /// The value that failed validation.
        value: String,
        /// Why it was rejected.
        reason: &'static str,
    },
```

- [ ] **Step 2: Update `invalid_datetime` constructor**

In `src/error.rs`, update the constructor:

```rust
    /// Create an [`InvalidDateTime`](Self::InvalidDateTime) error.
    ///
    /// This is the public constructor for the `InvalidDateTime` variant, which
    /// is `#[non_exhaustive]` and therefore cannot be constructed with a struct
    /// expression outside this crate.
    pub fn invalid_datetime(
        context: impl Into<String>,
        value: impl Into<String>,
        reason: &'static str,
    ) -> Self {
        Self::InvalidDateTime {
            context: context.into(),
            value: value.into(),
            reason,
        }
    }
```

- [ ] **Step 3: Update `validate_utc_datetime` in xml.rs**

In `src/webdav/xml.rs`, update the function signature and body:

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
            context: context.to_owned(),
            value: value.to_owned(),
            reason: "expected iCalendar format YYYYMMDDTHHMMSSZ (e.g. 20240101T000000Z)",
        });
    }
    Ok(())
}
```

Note: Remove the `_` prefix from `_context` — it's now used.

- [ ] **Step 4: Verify call sites pass meaningful context**

The call sites in `src/caldav/client.rs` already pass context strings:
- Line 496: `validate_utc_datetime(s, "invalid calendar-query start")?;`
- Line 499: `validate_utc_datetime(e, "invalid calendar-query end")?;`

These are good. Check if there are other call sites (carddav, etc.).

- [ ] **Step 5: Update tests**

In `tests/unit/common/error_tests.rs` and `tests/unit/caldav/client_tests.rs`, update any tests that construct or match on `InvalidDateTime` to include the `context` field.

- [ ] **Step 6: Verify**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo nextest run --all-features --locked --test unit_tests`
Run: `cargo test --doc --all-features`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/error.rs src/webdav/xml.rs tests/
git commit -m "fix: preserve start/end context in InvalidDateTime errors

Adds a context field to InvalidDateTime so callers can distinguish
which date-time (start vs end) failed validation. Fixes MEDIUM #3."
```

---

## Task 4: Fix `Tls { source: None }` docs to match reality (MEDIUM #4)

**Rationale:** The docs say `source` is `None` "when native certificate loading returns no roots" but in practice the code silently falls back to webpki roots without creating an error. The `tls()` constructor always sets `source: Some`. The `None` case is forward-looking — it's for a future where we might want to emit an error when native cert loading fails completely.

**Files:**
- Modify: `src/error.rs:161-182` — Update `Tls` doc comment

- [ ] **Step 1: Update Tls variant docs**

In `src/error.rs`, update the `Tls` variant doc comment:

```rust
    /// A TLS, certificate, or PKI operation failed.
    ///
    /// Covers PEM parsing errors, rustls configuration failures, and
    /// native certificate store errors. The `context` string describes
    /// where or why the error occurred; the underlying cause is
    /// accessible via `source()`.
    ///
    /// `source` is `Some` for most TLS errors — it wraps the underlying
    /// error from rustls, `rustls_pemfile`, or `rustls_native_certs`.
    /// `source` is `None` when the error has no underlying cause (e.g.
    /// a configuration error that is purely descriptive). The [`tls`](Self::tls)
    /// constructor always sets `source: Some`; `source: None` is only
    /// reachable via internal construction for edge cases.
    #[error("TLS error: {context}")]
    #[non_exhaustive]
    Tls {
        /// Human-readable context describing the TLS failure.
        context: String,
        /// The underlying error, if any. `Some` for most errors;
        /// `None` only when there is no deeper cause to chain.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
```

- [ ] **Step 2: Verify**

Run: `cargo test --doc --all-features`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "docs: fix Tls source:Option docs to match actual behavior

The None case is not currently produced by the library. Clarifies that
source is Some for most errors and None is for edge cases only.
Fixes MEDIUM #4."
```

---

## Task 5: Document `TlsRustls` vs `Tls` split (MEDIUM #5)

**Rationale:** TLS errors can appear as either `TlsRustls(rustls::Error)` (via `#[from]`) or `Tls { context, source }` (manual wrapping). Consumers checking "is this a TLS error?" must match both. Neither the README nor the variant docs mention this.

**Files:**
- Modify: `src/error.rs:157-159` — Add doc note on `TlsRustls`
- Modify: `README.md:204-206` — Add note in error table

- [ ] **Step 1: Add cross-reference doc on TlsRustls**

In `src/error.rs`, update the `TlsRustls` variant doc:

```rust
    /// A rustls TLS operation failed.
    ///
    /// This variant is used when a `rustls::Error` is propagated via `?`
    /// (automatic `#[from]` conversion). For manually-wrapped TLS errors
    /// that carry additional context (e.g. PEM parsing failures), see
    /// [`Tls`](Self::Tls). Consumers checking for TLS errors should match
    /// both `TlsRustls(_)` and `Tls { .. }`.
    #[error("rustls error: {0}")]
    TlsRustls(#[from] rustls::Error),
```

- [ ] **Step 2: Add note in README error table**

In `README.md`, after the error variants table (after line 211), add:

```markdown
> **Note:** TLS errors may appear as either `TlsRustls` (automatic
> `rustls::Error` propagation via `?`) or `Tls` (manually wrapped with
> context, e.g. PEM parsing). Consumers checking for TLS errors should
> match both variants.
```

- [ ] **Step 3: Verify**

Run: `cargo test --doc --all-features`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/error.rs README.md
git commit -m "docs: document TlsRustls vs Tls split for TLS error matching

Consumers checking for TLS errors must match both variants. Adds
cross-reference docs and a README note. Fixes MEDIUM #5."
```

---

## Task 6: Document module path changes in migration guide (MEDIUM #6)

**Rationale:** Legacy modules (`client`, `streaming`, `types`, `compression`) are now behind `#[cfg(feature = "legacy")]`. Users importing `fast_dav_rs::client::CalDavClient` will get a compile error. The README migration section doesn't mention this.

**Files:**
- Modify: `README.md` — Add section after "Migrating from `anyhow`" (after line 247)

- [ ] **Step 1: Add module path migration section**

In `README.md`, after line 247 (end of "Migrating from anyhow" section), add:

```markdown
### Migrating module paths

The deprecated top-level modules (`client`, `streaming`, `types`,
`compression`) are now gated behind the `legacy` Cargo feature
(default-off). Update your imports to the canonical paths:

```rust
// Before (deprecated, requires `legacy` feature)
use fast_dav_rs::client::CalDavClient;
use fast_dav_rs::streaming::parse_multistatus_stream;

// After (canonical paths)
use fast_dav_rs::caldav::client::CalDavClient;
use fast_dav_rs::caldav::streaming::parse_multistatus_stream;
```

If you need temporary backward compatibility, enable the `legacy`
feature in your `Cargo.toml`:

```toml
[dependencies]
fast-dav-rs = { version = "0.8", features = ["legacy"] }
```

The `legacy` feature will be removed in a future major release.
```

- [ ] **Step 2: Verify**

Run: `cargo test --doc --all-features`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add module path migration guide for legacy feature

Documents the breaking change from legacy top-level modules to
canonical paths and how to enable the legacy feature for backward
compat. Fixes MEDIUM #6."
```

---

## Task 7: Include invalid character in `InvalidComponentName` (LOW #7)

**Rationale:** The old code reported which character was invalid (`{bad:?}`). The new code uses `_bad` (unused) and a static reason. The user loses diagnostic detail.

Since `InvalidComponentName` has `reason: &'static str`, we can't include dynamic data there. But we can add it to the `name` field's Display or add a `bad_char: Option<char>` field. Since the variant is `#[non_exhaustive]`, adding a field is additive.

**Files:**
- Modify: `src/error.rs:67-75` — Add `bad_char` field
- Modify: `src/error.rs:316-321` — Update constructor
- Modify: `src/webdav/xml.rs:29-44` — Use `bad_char` in error
- Modify: `tests/unit/common/error_tests.rs` — Update tests

- [ ] **Step 1: Add `bad_char` field to `InvalidComponentName`**

In `src/error.rs`:

```rust
    /// A calendar or addressbook component name failed validation.
    #[error("invalid component name `{name}`: {reason}")]
    #[non_exhaustive]
    InvalidComponentName {
        /// The component name that was rejected.
        name: String,
        /// Why it was rejected.
        reason: &'static str,
        /// The invalid character that caused the rejection, if applicable.
        bad_char: Option<char>,
    },
```

- [ ] **Step 2: Update constructor**

```rust
    pub fn invalid_component_name(
        name: impl Into<String>,
        reason: &'static str,
    ) -> Self {
        Self::InvalidComponentName {
            name: name.into(),
            reason,
            bad_char: None,
        }
    }

    pub fn invalid_component_name_with_char(
        name: impl Into<String>,
        reason: &'static str,
        bad_char: char,
    ) -> Self {
        Self::InvalidComponentName {
            name: name.into(),
            reason,
            bad_char: Some(bad_char),
        }
    }
```

- [ ] **Step 3: Update `validate_component_name` in xml.rs**

```rust
pub(crate) fn validate_component_name(name: &str, _context: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::InvalidComponentName {
            name: name.to_owned(),
            reason: "component name must not be empty",
            bad_char: None,
        });
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
    {
        return Err(Error::InvalidComponentName {
            name: name.to_owned(),
            reason: "only ASCII letters, digits and '-' are allowed (e.g. VEVENT, X-CUSTOM)",
            bad_char: Some(bad),
        });
    }
    Ok(())
}
```

- [ ] **Step 4: Update tests**

Update any tests that construct or match on `InvalidComponentName` to include the `bad_char` field.

- [ ] **Step 5: Verify**

Run: `cargo fmt --all --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Run: `cargo nextest run --all-features --locked --test unit_tests`
Run: `cargo test --doc --all-features`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/error.rs src/webdav/xml.rs tests/
git commit -m "fix: include invalid character in InvalidComponentName errors

Restores diagnostic detail lost during the typed error migration.
Adds bad_char field to report which character was rejected.
Fixes LOW #7."
```

---

## Task 8: Add type assertions to builder tests (LOW #9)

**Rationale:** Several builder tests only assert `result.is_err()` without checking the error type. They should verify the error is `Error::InvalidConfig` with the expected message.

**Files:**
- Modify: `src/webdav/builder.rs` — Update inline tests

- [ ] **Step 1: Update builder tests with type assertions**

In `src/webdav/builder.rs`, update these inline tests:

`invalid_url_errors` — keep as-is (this is `InvalidUrl`, not `InvalidConfig`)

`empty_basic_user_errors`:
```rust
    #[test]
    fn empty_basic_user_errors() {
        let result = WebDavClient::builder(BASE).basic_auth("", "pass").build();
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("basic_auth requires both user and pass to be non-empty")),
            "should be InvalidConfig about basic_auth, got: {err}"
        );
    }
```

`empty_basic_pass_errors`:
```rust
    #[test]
    fn empty_basic_pass_errors() {
        let result = WebDavClient::builder(BASE).basic_auth("user", "").build();
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("basic_auth requires both user and pass to be non-empty")),
            "should be InvalidConfig about basic_auth, got: {err}"
        );
    }
```

`empty_bearer_token_errors`:
```rust
    #[test]
    fn empty_bearer_token_errors() {
        let result = WebDavClient::builder(BASE).bearer_token("").build();
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("bearer_token must not be empty")),
            "should be InvalidConfig about bearer_token, got: {err}"
        );
    }
```

`invalid_bearer_chars_errors`:
```rust
    #[test]
    fn invalid_bearer_chars_errors() {
        let result = WebDavClient::builder(BASE)
            .bearer_token("has space")
            .build();
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("bearer_token contains invalid characters")),
            "should be InvalidConfig about bearer_token chars, got: {err}"
        );
    }
```

`proxy_auth_without_proxy_errors`:
```rust
    #[test]
    fn proxy_auth_without_proxy_errors() {
        let result = WebDavClient::builder(BASE)
            .proxy_basic_auth("user", "pass")
            .build();
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("proxy_basic_auth requires a proxy")),
            "should be InvalidConfig about proxy_basic_auth, got: {err}"
        );
    }
```

`invalid_url_errors` — this should assert `InvalidUrl`:
```rust
    #[test]
    fn invalid_url_errors() {
        let result = WebDavClient::builder("not a valid url").build();
        assert!(matches!(result.unwrap_err(), Error::InvalidUrl { .. }));
    }
```

- [ ] **Step 2: Verify**

Run: `cargo nextest run --all-features --locked --test unit_tests`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/webdav/builder.rs
git commit -m "test: add error type assertions to builder tests

Replaces bare is_err() assertions with matches! on the expected
Error variant and message. Fixes LOW #9."
```

---

## Task 9: Add `#[from]` conversion test for `TlsRustls` (LOW #10)

**Rationale:** All `#[from]` conversions are tested except `TlsRustls` (`From<rustls::Error>`).

**Files:**
- Modify: `tests/unit/common/error_tests.rs` — Add test

- [ ] **Step 1: Add TlsRustls conversion test**

In `tests/unit/common/error_tests.rs`, add:

```rust
#[test]
fn from_rustls_error() {
    let rustls_error = rustls::Error::General("test TLS failure".to_owned());
    let error: Error = rustls_error.into();
    assert!(matches!(error, Error::TlsRustls(_)));
    assert!(
        error.to_string().contains("test TLS failure"),
        "display should contain the rustls message, got: {error}"
    );
}
```

Note: Check the `rustls` crate's public API for constructing an error. `rustls::Error::General(String)` is a common variant. If it's not available, find another public constructor. Read `rustls` docs or source to confirm.

- [ ] **Step 2: Verify**

Run: `cargo nextest run --all-features --locked --test unit_tests`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/unit/common/error_tests.rs
git commit -m "test: add #[from] conversion test for TlsRustls

Closes the last gap in #[from] conversion test coverage.
Fixes LOW #10."
```

---

## Task 10: Add broader proxy control-char tests (LOW #12)

**Rationale:** Tests only cover `\n` (0x0A). The validation checks `b <= 0x20 || b == 0x7F`. Tests should cover null byte, other control chars, space, and DEL.

**Files:**
- Modify: `src/webdav/builder.rs` — Add tests

- [ ] **Step 1: Add broader control-char tests**

In `src/webdav/builder.rs`, add after the existing proxy auth tests:

```rust
    #[test]
    fn proxy_basic_auth_with_null_byte_user_errors() {
        let result = WebDavClient::builder(BASE)
            .proxy(Uri::from_str("http://127.0.0.1:9090").unwrap())
            .proxy_basic_auth("user\0", "pass")
            .build();
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("proxy_basic_auth")),
            "should be InvalidConfig about proxy_basic_auth, got: {err}"
        );
    }

    #[test]
    fn proxy_basic_auth_with_del_char_pass_errors() {
        let result = WebDavClient::builder(BASE)
            .proxy(Uri::from_str("http://127.0.0.1:9090").unwrap())
            .proxy_basic_auth("user", "pass\x7F")
            .build();
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("proxy_basic_auth")),
            "should be InvalidConfig about proxy_basic_auth, got: {err}"
        );
    }

    #[test]
    fn proxy_basic_auth_with_space_user_errors() {
        let result = WebDavClient::builder(BASE)
            .proxy(Uri::from_str("http://127.0.0.1:9090").unwrap())
            .proxy_basic_auth("user with space", "pass")
            .build();
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(
            matches!(err, Error::InvalidConfig(ref msg) if msg.contains("proxy_basic_auth")),
            "should be InvalidConfig about proxy_basic_auth, got: {err}"
        );
    }
```

- [ ] **Step 2: Verify**

Run: `cargo nextest run --all-features --locked --test unit_tests`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/webdav/builder.rs
git commit -m "test: add broader proxy control-char validation tests

Adds tests for null byte (0x00), DEL (0x7F), and space (0x20) in
proxy credentials. Fixes LOW #12."
```

---

## Task 11: Add `examples/` to `Cargo.toml` include (LOW #13)

**Rationale:** `Cargo.toml` `include` doesn't list `examples/`, so `cargo run --example migration` only works from a git checkout, not from crates.io.

**Files:**
- Modify: `Cargo.toml:14`

- [ ] **Step 1: Update Cargo.toml include**

In `Cargo.toml` line 14, add `examples/` to the include list:

```toml
include = ["Cargo.toml", "LICENSE", "README.md", "src/**/*.rs", "examples/**/*.rs"]
```

- [ ] **Step 2: Verify**

Run: `cargo build --examples --all-features`
Run: `cargo package --allow-dirty --list 2>&1 | grep examples` (verify examples are included)
Expected: examples listed

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "fix: include examples/ in published crate

cargo run --example migration now works from crates.io, not just
git checkouts. Fixes LOW #13."
```

---

## Task 12: Add `--locked` to CI examples and doc tests (LOW #14)

**Rationale:** The nextest step uses `--locked` but the examples build and doc tests do not, allowing `Cargo.lock` to drift.

**Files:**
- Modify: `.github/workflows/ci.yml:54-58`

- [ ] **Step 1: Add --locked to CI steps**

In `.github/workflows/ci.yml`, update:

```yaml
      - name: Cargo build examples
        run: cargo build --examples --all-features --locked

      - name: Cargo doc tests
        run: cargo test --doc --all-features --locked
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add --locked to examples build and doc tests

Ensures Cargo.lock does not drift in CI. Fixes LOW #14."
```

---

## Task 13: Final verification

- [ ] **Step 1: Full verification**

Run all checks:
```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --locked --test unit_tests
cargo test --doc --all-features --locked
cargo build --examples --all-features --locked
```

All must pass.

- [ ] **Step 2: Verify test count increased**

The test count should be higher than 242 (the previous count) due to new tests added in Tasks 9 and 10.

- [ ] **Step 3: Post update on PR**

Post a summary comment on PR #69 listing all fixes from the adversarial review.

---

## Self-Review

### Spec Coverage

| Finding | Task | Status |
|---|---|---|
| #1 HIGH: #[from] claims | Task 1 | Covered |
| #2 MEDIUM: InvalidInput dead code | Task 2 | Covered (docs) |
| #3 MEDIUM: datetime context lost | Task 3 | Covered (add context field) |
| #4 MEDIUM: Tls source: None docs | Task 4 | Covered (docs) |
| #5 MEDIUM: TlsRustls vs Tls split | Task 5 | Covered (docs) |
| #6 MEDIUM: module path migration | Task 6 | Covered (docs) |
| #7 LOW: lost invalid char | Task 7 | Covered (add bad_char field) |
| #8 LOW: from_quick_xml stringifies | — | No action (already documented) |
| #9 LOW: builder tests is_err() | Task 8 | Covered |
| #10 LOW: missing TlsRustls test | Task 9 | Covered |
| #11 LOW: port 1 test fragile | — | No action (already documented) |
| #12 LOW: proxy char tests narrow | Task 10 | Covered |
| #13 LOW: examples not in include | Task 11 | Covered |
| #14 LOW: CI --locked missing | Task 12 | Covered |

### Dependency Order

Tasks 1-6 are independent docs/comment fixes. Task 3 and 7 modify `src/error.rs` struct variants — do them sequentially. Tasks 8-12 are independent. Task 13 must be last.

**Recommended order:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13
