# Typed Errors Audit Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all findings from the adversarial code review of the `feat/typed-thiserrors-work` branch — broken doc links, missing proxy credential validation, missing `from_client` tests, fragile test helpers, source-chain preservation, and defensive hardening.

**Architecture:** The changes are localized to `src/error.rs`, `src/webdav/builder.rs`, `tests/unit/common/error_tests.rs`, and `src/lib.rs`. No new modules are introduced. The plan is ordered so each task produces an independently testable, committable deliverable.

**Tech Stack:** Rust 2024, `thiserror 2`, `hyper-util 0.1`, `quick-xml 0.41`, `zeroize 1`.

## Global Constraints

- **Rust edition 2024**, MSRV 1.85 (from `Cargo.toml`).
- `cargo fmt --all --check` must pass after every task.
- `cargo clippy --all-targets --all-features -- -D warnings` must pass after every task.
- `cargo test --all-features --test unit_tests` must pass after every task.
- `cargo test --all-features --doc` must pass after every task.
- No new crate dependencies — use only what is already in `Cargo.toml`.
- Follow existing conventions: `#[non_exhaustive]` on `Error`, doc comments on all public items, `snake_case` for functions, `PascalCase` for variants.
- `Error` is `#[non_exhaustive]` — always include a wildcard arm when matching in new tests.

---

## Task 1: Fix broken intra-doc link on `Error` enum

**Files:**
- Modify: `src/error.rs:6`

**Interfaces:**
- No API changes — doc comment only.

- [ ] **Step 1: Fix the doc comment**

In `src/error.rs`, replace line 6:

```rust
/// The enum is [`#[non_exhaustive`][ne]] so that new variants can be added
```

with:

```rust
/// The enum is `#[non_exhaustive]` so that new variants can be added
```

This removes the broken intra-doc link syntax. The `#[non_exhaustive]` attribute is well-known; a plain code span is clearer than a link to the reference page.

Optionally, keep the reference link on a separate line if desired:

```rust
/// The enum is `#[non_exhaustive]` so that new variants can be added
/// without breaking downstream `match` expressions. Always include a
/// wildcard arm (`_ => …`) when matching.
///
/// [non_exhaustive]: https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute
```

- [ ] **Step 2: Verify doc builds cleanly**

Run: `cargo doc --no-deps 2>&1 | grep -i warning`
Expected: No warnings about broken links on `error.rs`.

- [ ] **Step 3: Verify all tests still pass**

Run: `cargo test --all-features --test unit_tests && cargo test --all-features --doc`
Expected: All tests pass (237 unit + 29 doc).

- [ ] **Step 4: Commit**

```bash
git add src/error.rs
git commit -m "fix: repair broken intra-doc link on Error enum"
```

---

## Task 2: Add proxy credential validation in builder

**Files:**
- Modify: `src/webdav/builder.rs:287-301` (existing validation block) and `src/webdav/builder.rs:519-522` (tunnel auth construction)

**Interfaces:**
- Produces: clearer `Error::InvalidInput` messages for malformed proxy credentials, improving debuggability without changing the public API.

### Context

Currently, proxy credentials are validated for emptiness (lines 294-300) but not for invalid characters. If `user:pass` contains characters that produce an invalid `HeaderValue` after base64 encoding (e.g., newlines injected via control characters), the error surfaces as a cryptic `Error::InvalidHeader` from `parse()`. We add an explicit `InvalidInput` check so the caller sees a clear validation message before the base64 round-trip.

- [ ] **Step 1: Write the failing test**

Add to `src/webdav/builder.rs` in the `#[cfg(test)] mod tests` block (after the existing `proxy_auth_without_proxy_errors` test, around line 802):

```rust
    #[test]
    fn proxy_basic_auth_with_newline_user_errors() {
        let result = WebDavClient::builder(BASE)
            .proxy("http://127.0.0.1:9090")
            .proxy_basic_auth("user\ninjected", "pass")
            .build();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidInput(ref msg) if msg.contains("proxy_basic_auth")),
            "should be InvalidInput about proxy_basic_auth, got: {err}"
        );
    }

    #[test]
    fn proxy_basic_auth_with_newline_pass_errors() {
        let result = WebDavClient::builder(BASE)
            .proxy("http://127.0.0.1:9090")
            .proxy_basic_auth("user", "pass\ninjected")
            .build();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidInput(ref msg) if msg.contains("proxy_basic_auth")),
            "should be InvalidInput about proxy_basic_auth, got: {err}"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --all-features --test unit_tests webdav::builder::tests::proxy_basic_auth_with_newline_user_errors && cargo test --all-features --test unit_tests webdav::builder::tests::proxy_basic_auth_with_newline_pass_errors`

Expected: FAIL — the credentials with `\n` are currently accepted into `build_auth_header` and only fail later at `parse()` with a different error variant (`InvalidHeader` or similar), not `InvalidInput` mentioning `proxy_basic_auth`.

- [ ] **Step 3: Add validation logic**

In `src/webdav/builder.rs`, in the `build` method, after the existing proxy credential emptiness check (after line 300), add:

```rust
        if let (Some(user), Some(pass)) = (&self.proxy_basic_user, &self.proxy_basic_pass) {
            for (label, value) in [("user", user.as_str()), ("pass", pass.as_str())] {
                if value.bytes().any(|b| b <= 0x20 || b == 0x7F) {
                    return Err(Error::InvalidInput(format!(
                        "proxy_basic_auth {label} contains control or whitespace characters \
                         which are not allowed in HTTP header values"
                    )));
                }
            }
        }
```

Note: This check is deliberately conservative — it rejects bytes ≤ 0x20 (space, CR, LF, tab, …) and 0x7F (DEL). Valid HTTP header value characters are VCHAR (0x21–0x7E) plus obs-text (0x80–0xFF). Base64 output stays within ASCII alphanumerics and `+`, `/`, `=`, so this input check is stricter than strictly necessary, but it prevents injection attacks.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --all-features --test unit_tests webdav::builder`
Expected: All builder tests pass, including the two new ones.

- [ ] **Step 5: Verify clippy and fmt**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: No warnings, no formatting changes needed.

- [ ] **Step 6: Commit**

```bash
git add src/webdav/builder.rs
git commit -m "feat(builder): validate proxy credentials for control characters"
```

---

## Task 3: Add unit tests for `from_client` (Connection vs Transport classification)

**Files:**
- Modify: `tests/unit/common/error_tests.rs`
- Modify: `tests/unit/common/mod.rs` (if needed for module visibility)
- Modify: `src/error.rs` (expose `from_client` for `#[cfg(test)]`)

**Interfaces:**
- Consumes: `Error::from_client` (currently `pub(crate)`)
- Produces: Test coverage proving `is_connect()` errors map to `Connection` and other errors map to `Transport`.

### Context

`from_client` is the retry-safety-critical classification function. `hyper_util::client::legacy::Error`'s `ErrorKind` enum is private, so we cannot construct errors directly. Instead, we trigger real connection failures by attempting to connect to an unresolvable host (DNS failure → `is_connect() == true` → `Connection`) and by simulating a transport error on an established connection.

Since `from_client` is `pub(crate)`, we need a test-only public accessor. The cleanest approach is to expose it behind `#[cfg(test)]` on the `Error` impl, or test via the public `send` method and assert on the resulting `Error` variant.

Actually, the simplest approach that avoids modifying `error.rs` visibility: test the behavior end-to-end by calling `WebDavClient::send` against a dead port (connection refused → `is_connect()` true → `Error::Connection`) and against a server that closes mid-stream (transport error → `Error::Transport`).

However, for a focused unit test, we can use the fact that `hyper_util::client::legacy::Error` implements `std::error::Error` and can be obtained from real connection attempts. We'll write integration-style tests in the unit test file using a `WebDavClient` pointed at an invalid address.

- [ ] **Step 1: Write the failing test for Connection classification**

Add to `tests/unit/common/error_tests.rs`:

```rust
#[tokio::test]
async fn connection_error_maps_to_connection_variant() {
    use fast_dav_rs::webdav::WebDavClient;
    use hyper::Method;
    use hyper::HeaderMap;

    // Point at a port that refuses connections (localhost:1 is almost always closed).
    let client = WebDavClient::builder("http://127.0.0.1:1/")
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();

    let result = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await;

    let err = result.expect_err("connection should fail");
    assert!(
        matches!(err, Error::Connection(_)),
        "connect-refused should map to Error::Connection, got: {err:?}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test --all-features --test unit_tests common::error_tests::connection_error_maps_to_connection_variant -- --nocapture`
Expected: PASS (connection refused on localhost:1 should reliably produce a connect error). If the test environment has something listening on port 1 (extremely rare), the test may need a different port. Document this in the test comment.

Note: If this passes immediately, that's fine — it confirms the existing behavior is correct. The test is the deliverable; it guards against regressions.

- [ ] **Step 3: Write the test for Transport classification**

Transport errors (non-connect) are harder to trigger without a real server. We can use a mock that accepts then immediately closes, but that requires a dependency. A simpler approach: test `from_client` directly by making it accessible.

Add to `src/error.rs`, inside the `impl Error` block:

```rust
    #[cfg(test)]
    pub(crate) fn from_client_for_test(source: hyper_util::client::legacy::Error) -> Self {
        Self::from_client(source)
    }
```

Then, to construct a non-connect error without a real server, we can use the fact that `Client::request` returns an error when the response body stream is interrupted. However, this is complex for a unit test.

**Alternative approach (simpler):** Instead of testing `from_client` in isolation, test the public behavior — a `Connection` error is produced when the server is unreachable, and any other error variant (e.g., `Timeout`) is produced when the server is reachable but slow. Since we can't easily simulate a transport-only failure without a server, we document this gap:

```rust
// NOTE: A transport-specific error (Error::Transport) requires a server that
// accepts a connection then breaks the response stream mid-flight. This is
// exercised by the e2e test suite against a real DAV server. The unit test
// above covers the connect-path, which is the most common retry-relevant case.
```

Add this comment to the test file.

- [ ] **Step 4: Run all error tests**

Run: `cargo test --all-features --test unit_tests common::error_tests -- --nocapture`
Expected: All error tests pass, including the new `connection_error_maps_to_connection_variant`.

- [ ] **Step 5: Verify clippy and fmt**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: Clean.

- [ ] **Step 6: Commit**

```bash
git add tests/unit/common/error_tests.rs src/error.rs
git commit -m "test(error): add unit test for from_client Connection classification"
```

---

## Task 4: Rename inherent `source()` method to avoid trait shadowing

**Files:**
- Modify: `src/error.rs:191-193`
- Modify: `tests/unit/common/error_tests.rs` (update calls to `source()`)
- Modify: any other callers of `Error::source()` in the codebase (search first)

**Interfaces:**
- Changes public API: `Error::source()` → `Error::source_err()` (or keep `source()` and add a note; see discussion).

### Context

The inherent `pub fn source(&self)` shadows `std::error::Error::source`. While the current implementation delegates correctly, the shadowing is a footgun for generic code. The cleanest fix is to remove the inherent method entirely and let callers use the trait. If the convenience of not importing the trait is desired, document the shadowing explicitly.

**Decision:** Remove the inherent `source()` method. Callers who want the source should `use std::error::Error;` and call `.source()` on the trait. This is the idiomatic Rust pattern and eliminates all ambiguity. Update all callers.

- [ ] **Step 1: Find all callers of the inherent `source()`**

Search: `rg "\.source\(\)" --type rust` in the codebase.
Document each call site here (to be filled by the implementer based on search results).

- [ ] **Step 2: Update test callers**

In `tests/unit/common/error_tests.rs`, update `error.source()` calls to use the trait:

Replace:
```rust
assert!(
    error.source().is_some(),
    "source() must return the inner error"
);
```

with:
```rust
use std::error::Error as _;
assert!(
    error.source().is_some(),
    "source() must return the inner error"
);
```

The `use std::error::Error as _;` brings the trait into scope without polluting the namespace.

Apply this to all `source()` calls in the test file (lines 144, 148, 180, 183).

- [ ] **Step 3: Remove the inherent method from `src/error.rs`**

Delete lines 186-193 in `src/error.rs`:

```rust
    /// Return the underlying cause of this error, if any.
    ///
    /// This is an inherent method so that callers do not need to import the
    /// `std::error::Error` trait. It delegates to the trait implementation
    /// and is therefore equivalent to `<Error as std::error::Error>::source(self)`.
    pub fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        <Self as std::error::Error>::source(self)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --all-features --test unit_tests && cargo test --all-features --doc`
Expected: All tests pass. If any test fails, it means a caller relied on the inherent method without importing the trait — add `use std::error::Error as _;` at the call site.

- [ ] **Step 5: Verify clippy and fmt**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: Clean.

- [ ] **Step 6: Commit**

```bash
git add src/error.rs tests/unit/common/error_tests.rs
git commit -m "refactor(error): remove inherent source() to avoid trait shadowing"
```

---

## Task 5: Preserve source chain in `from_quick_xml` for Syntax/IllFormed

**Files:**
- Modify: `src/error.rs:202-208`

**Interfaces:**
- Changes: `Error::XmlStructure` now carries a structured error instead of a `String` for `Syntax`/`IllFormed` cases. This is a **breaking change** for consumers matching on `XmlStructure(String)`. Since `Error` is `#[non_exhaustive]` and `XmlStructure` is a tuple variant, this is an accepted breaking change within the current pre-1.0 version.

### Context

`from_quick_xml` converts `Syntax` and `IllFormed` errors to `XmlStructure(String)`, losing the original error type. We preserve the source by wrapping it.

**Approach:** Change `XmlStructure` from `String` to a struct variant that holds both a message and an optional source. However, since `quick_xml::Error` variants `Syntax` and `IllFormed` contain a `Cow<str>`, we can keep the `String` but also store the original `quick_xml::Error` as a source.

Actually, the simplest non-breaking approach: keep `XmlStructure(String)` as-is, but wrap the `quick_xml::Error` into a new internal variant or use `Other` with a source. But that loses the semantic distinction.

**Better approach:** Add a new variant `XmlSyntax` with a source field, reserved for `Syntax`/`IllFormed`:

```rust
    /// The XML element hierarchy is malformed or incomplete.
    #[error("XML structure error: {0}")]
    XmlStructure(String),

    /// The XML document has syntax errors or is ill-formed.
    #[error("XML syntax error: {0}")]
    XmlSyntax {
        message: String,
        #[source]
        source: quick_xml::Error,
    },
```

Wait — `quick_xml::Error::Syntax` contains a `Cow<'static, str>`, and `quick_xml::Error` itself is not `Clone`. But we own the `quick_xml::Error` after the match, so we can move it.

Actually, looking at `from_quick_xml` more carefully:

```rust
pub(crate) fn from_quick_xml(error: quick_xml::Error) -> Self {
    match error {
        quick_xml::Error::Syntax(s) => Self::XmlStructure(s.to_string()),
        quick_xml::Error::IllFormed(s) => Self::XmlStructure(s.to_string()),
        other => Self::Xml(other),
    }
}
```

The `Syntax` and `IllFormed` variants contain a `Cow<str>` — just a message. The error chain is not deeper than that. So the "lost source" is just the `Cow<str>` message being stringified. The real loss is that `quick_xml::Error::Syntax` as a type is gone, but since it only carries a string, there's no deeper source chain to preserve.

**Revised approach:** Keep the current behavior but add a doc comment explaining that `XmlStructure` intentionally stringifies because `quick_xml::Syntax`/`IllFormed` carry only a message string, not a source chain. This is a documentation fix, not a code change.

- [ ] **Step 1: Add explanatory doc comment**

In `src/error.rs`, update the `from_quick_xml` doc comment (lines 195-201) to explain why stringification is acceptable:

```rust
    /// Convert a `quick_xml::Error` into the most specific `Error` variant.
    ///
    /// `Syntax` and `IllFormed` errors are mapped to [`XmlStructure`](Self::XmlStructure)
    /// because they indicate a structurally invalid XML document (mismatched tags,
    /// unclosed elements, …). The `quick_xml` message is stringified because
    /// `Syntax` and `IllFormed` carry only a `Cow<str>` message — there is no
    /// deeper source chain to preserve. All other variants (`Io`, `Encoding`,
    /// `Escape`, `InvalidAttr`, `Namespace`) are mapped to [`Xml`](Self::Xml) via the
    /// blanket `#[from]` conversion, which preserves the full error chain.
    pub(crate) fn from_quick_xml(error: quick_xml::Error) -> Self {
```

- [ ] **Step 2: Run tests**

Run: `cargo test --all-features --test unit_tests && cargo test --all-features --doc`
Expected: All pass (doc-only change).

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "docs(error): explain why from_quick_xml stringifies Syntax/IllFormed"
```

---

## Task 6: Make `from_http_error` test more direct

**Files:**
- Modify: `tests/unit/common/error_tests.rs:91-97`

**Interfaces:**
- No API changes — test-only.

- [ ] **Step 1: Replace the fragile test**

Replace the test at lines 91-97:

```rust
#[test]
fn from_http_error() {
    let uri_result: std::result::Result<hyper::http::uri::Uri, _> = "bad uri with spaces".parse();
    let uri_err = uri_result.unwrap_err();
    let http_err: hyper::http::Error = uri_err.into();
    let error: Error = http_err.into();
    assert!(matches!(error, Error::Http(_)));
}
```

with a more direct test:

```rust
#[test]
fn from_http_error() {
    // Construct an http::Error directly from an InvalidUriParts, avoiding
    // dependency on the string-parsing behavior of the URI parser.
    let invalid_parts = hyper::http::uri::Uri::from_parts(
        hyper::http::uri::Parts::default(),
    ).unwrap_err();
    let http_err: hyper::http::Error = invalid_parts.into();
    let error: Error = http_err.into();
    assert!(matches!(error, Error::Http(_)));
}
```

Wait — `Uri::from_parts` with default parts actually succeeds (empty URI is valid). Let me use a known-invalid construction:

```rust
#[test]
fn from_http_error() {
    // hyper::http::Error::from(InvalidUriParts) is the most direct path.
    // InvalidUriParts is produced when parts are inconsistent (e.g. scheme
    // set but authority missing for an absolute URI).
    let mut parts = hyper::http::uri::Parts::default();
    parts.scheme = Some("http".parse().unwrap());
    // No authority — this is invalid for an absolute URI.
    let invalid_uri = hyper::http::uri::Uri::from_parts(parts).unwrap_err();
    let http_err: hyper::http::Error = invalid_uri.into();
    let error: Error = http_err.into();
    assert!(matches!(error, Error::Http(_)));
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test --all-features --test unit_tests common::error_tests::from_http_error -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Verify clippy and fmt**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: Clean.

- [ ] **Step 4: Commit**

```bash
git add tests/unit/common/error_tests.rs
git commit -m "test(error): make from_http_error test independent of URI string parsing"
```

---

## Task 7: Use explicit formatting for `Timeout` Display

**Files:**
- Modify: `src/error.rs:62`
- Modify: `tests/unit/common/error_tests.rs:57-61`

**Interfaces:**
- Changes the `Display` output of `Error::Timeout` from `{:?}` (Debug format) to explicit seconds. This is a minor breaking change for consumers string-matching on the Display output.

### Context

Currently: `#[error("operation timed out after {limit:?}")]` produces `operation timed out after 20s` (Debug format of `Duration`). The test at line 60 asserts `.contains("20s")`. Debug format is not guaranteed stable across Rust versions.

- [ ] **Step 1: Change the Display format**

In `src/error.rs`, change line 62:

```rust
    #[error("operation timed out after {limit:?}")]
```

to:

```rust
    #[error("operation timed out after {limit_secs}s")]
```

And add a helper field or use a method. Since `thiserror` format strings reference fields, we need the field to exist. We can't add a computed field to the variant, so we use a custom Display expression.

Actually, `thiserror` supports `#[error("...", {0.limit.as_secs()})]` syntax? No — `thiserror` only supports field references. We need a different approach.

**Option A:** Keep `{limit:?}` and make the test robust:
```rust
assert!(timeout_error.to_string().contains("20s") || timeout_error.to_string().contains("20.0s"));
```

**Option B:** Implement `Display` manually for the `Timeout` variant. But `thiserror` generates the whole `Display`.

**Option C:** Use `#[error("operation timed out after {}s", limit.as_secs())]`. This is supported by `thiserror` — it allows expressions after the format string, like `format!`.

Let me verify: `thiserror`'s `#[error(...)]` supports the same syntax as `format!`, so `#[error("operation timed out after {}s", limit.as_secs())]` works.

- [ ] **Step 2: Apply the change**

In `src/error.rs`, change the `Timeout` variant:

```rust
    /// An operation exceeded its configured time limit.
    #[error("operation timed out after {}s", limit.as_secs())]
    Timeout {
        /// The configured time limit.
        limit: Duration,
    },
```

- [ ] **Step 3: Update the test**

In `tests/unit/common/error_tests.rs`, the test at line 57-61:

```rust
    let timeout_error = Error::Timeout {
        limit: Duration::from_secs(20),
    };
    assert!(timeout_error.to_string().contains("20s"));
```

This will still pass because `as_secs()` returns `20` and the format produces `operation timed out after 20s`.

But also test sub-second durations to be thorough:

```rust
    let timeout_error = Error::Timeout {
        limit: Duration::from_secs(20),
    };
    assert_eq!(
        timeout_error.to_string(),
        "operation timed out after 20s"
    );
```

- [ ] **Step 4: Run tests**

Run: `cargo test --all-features --test unit_tests common::error_tests -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Verify clippy and fmt**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: Clean.

- [ ] **Step 6: Commit**

```bash
git add src/error.rs tests/unit/common/error_tests.rs
git commit -m "refactor(error): use explicit as_secs() format for Timeout Display"
```

---

## Task 8: Add `default-features = false` for `thiserror`

**Files:**
- Modify: `Cargo.toml:34`

- [ ] **Step 1: Check if thiserror has default features**

Run: `cargo tree -e features -i thiserror 2>/dev/null | head -20`

If `thiserror` has no default features or they are essential, skip this task (mark as "not needed"). If it has default features that are unnecessary, apply the change.

- [ ] **Step 2: Apply change (if applicable)**

In `Cargo.toml`, change line 34:

```toml
thiserror = "2"
```

to:

```toml
thiserror = { version = "2", default-features = false }
```

- [ ] **Step 3: Verify build**

Run: `cargo build --all-features && cargo test --all-features --test unit_tests`
Expected: Build and tests pass.

- [ ] **Step 4: Commit (if change was made)**

```bash
git add Cargo.toml
git commit -m "chore: disable default-features for thiserror"
```

---

## Task 9: Add `#[deprecated]` to legacy re-export modules

**Files:**
- Modify: `src/lib.rs:725-740`

**Interfaces:**
- Adds `#[deprecated]` attributes to `client`, `streaming`, `types`, `compression` legacy modules.

### Context

These modules exist for backward compatibility. Deprecation guides users to the canonical paths.

- [ ] **Step 1: Add deprecation attributes**

In `src/lib.rs`, replace lines 725-740:

```rust
// Legacy module paths kept for compatibility with existing imports.
pub mod client {
    pub use crate::caldav::client::*;
}

pub mod streaming {
    pub use crate::caldav::streaming::*;
}

pub mod types {
    pub use crate::caldav::types::*;
}

pub mod compression {
    pub use crate::common::compression::*;
}
```

with:

```rust
// Legacy module paths kept for compatibility with existing imports.
#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::caldav::client` directly instead"
)]
pub mod client {
    pub use crate::caldav::client::*;
}

#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::caldav::streaming` directly instead"
)]
pub mod streaming {
    pub use crate::caldav::streaming::*;
}

#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::caldav::types` directly instead"
)]
pub mod types {
    pub use crate::caldav::types::*;
}

#[deprecated(
    since = "0.8.0",
    note = "use `fast_dav_rs::common::compression` directly instead"
)]
pub mod compression {
    pub use crate::common::compression::*;
}
```

- [ ] **Step 2: Verify build and tests**

Run: `cargo build --all-features && cargo test --all-features --test unit_tests && cargo test --all-features --doc`
Expected: Build succeeds. Deprecation warnings appear only if internal code uses these paths. If internal code uses them, suppress with `#[allow(deprecated)]` at the call site or update the import.

- [ ] **Step 3: Verify no deprecation warnings in the library itself**

Run: `cargo build --all-features 2>&1 | grep -i "deprecated"`
Expected: No warnings (the library should not use its own deprecated paths).

- [ ] **Step 4: Verify clippy and fmt**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: Clean.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs
git commit -m "chore: deprecate legacy re-export modules"
```

---

## Task 10: Zeroize `extra_root_certs_pem` on Drop

**Files:**
- Modify: `src/webdav/builder.rs:341-349`

### Context

`extra_root_certs_pem` is a `Vec<Vec<u8>>` that may contain PEM-encoded certificates. While certificates are typically public, they could theoretically contain private keys in rare configurations. Zeroizing them on drop is a defensive best practice.

- [ ] **Step 1: Update the Drop impl**

In `src/webdav/builder.rs`, update the `Drop` implementation (lines 341-349):

```rust
impl Drop for WebDavClientBuilder {
    fn drop(&mut self) {
        self.basic_user.zeroize();
        self.basic_pass.zeroize();
        self.bearer_token.zeroize();
        self.proxy_basic_user.zeroize();
        self.proxy_basic_pass.zeroize();
        for mut pem in std::mem::take(&mut self.extra_root_certs_pem) {
            pem.zeroize();
        }
    }
}
```

- [ ] **Step 2: Verify build and tests**

Run: `cargo build --all-features && cargo test --all-features --test unit_tests`
Expected: Build and tests pass. `Vec<u8>` implements `Zeroize`.

- [ ] **Step 3: Verify clippy and fmt**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: Clean.

- [ ] **Step 4: Commit**

```bash
git add src/webdav/builder.rs
git commit -m "security(builder): zeroize extra_root_certs_pem on drop"
```

---

## Final Verification

After all tasks are complete:

- [ ] **Run full CI suite locally**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --locked --test unit_tests
cargo test --all-features --doc
cargo doc --no-deps
```

All must pass with zero warnings.

- [ ] **Verify no regressions in error variant counts**

The `Error` enum should have the same number of variants (or more, if new ones were added). Existing variants should not have been removed or renamed without a major version bump.

- [ ] **Review the full diff**

```bash
git diff main
```

Ensure the diff is clean, focused, and contains only the changes from this plan.
