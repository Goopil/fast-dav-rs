# Audit Phase 0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix audit findings AUDIT-001, -002, -005, -022 (partial), -030 and close AUDIT-009, per `docs/superpowers/specs/2026-09-01-audit-phase0-design.md` and `docs/audit/REMEDIATION_PLAN.md` Phase 0.

**Architecture:** Small independent fixes on branch `audit/phase-0` (already created from `main`): two-line Depth header fix, timeout wrap on body read, YAML publish guard, gitignore line, graceful no-panic batch paths. One shared HTTP test helper added under `tests/unit/common/`.

**Tech Stack:** Rust (edition 2024), tokio, hyper 1.x, nextest.

## Global Constraints

- Gates (AGENTS.md, mandatory before pushing): `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo nextest run --all-features --locked --test unit_tests`, `cargo test --doc --all-features`, `cargo build --examples --all-features`.
- SonarCloud gates on new code: coverage ≥ 80%, duplication ≤ 3%.
- Error creation: `Error::other(...)` for escape-hatch errors (`src/error.rs:425`).
- No new dependencies. No comments in code except `ponytail:` corner markers.
- Test target: `tests/unit_tests` (`tests/unit/` tree); shared helpers go in `tests/unit/common/http_helpers.rs`, importable as `crate::common::http_helpers::…`.

---

### Task 1: Shared HTTP test helpers

**Files:**
- Create: `tests/unit/common/http_helpers.rs`
- Modify: `tests/unit/common/mod.rs` (add `pub mod http_helpers;`)
- Modify: `tests/unit/webdav/streaming_tests.rs` (delete private `serve_once` + `response_head`, use shared `response_head`)

**Interfaces:**
- Produces: `pub async fn serve_capture(head: String, body: Vec<u8>) -> (String, Arc<Mutex<Vec<u8>>>)`, `pub async fn serve_stalled(head: String, partial_body: &[u8]) -> String`, `pub fn response_head(extra_headers: &str, body_len: usize) -> String`

- [ ] **Step 1: Write `tests/unit/common/http_helpers.rs`**

```rust
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// HTTP/1.1 response head with `Content-Length` and `Connection: close`.
pub fn response_head(extra_headers: &str, body_len: usize) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\n{extra_headers}Connection: close\r\n\r\n"
    )
}

/// Serve exactly one HTTP/1.1 exchange on an ephemeral port: read the full
/// request (headers + `Content-Length` body), capture it, respond, close.
pub async fn serve_capture(head: String, body: Vec<u8>) -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = captured.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let mut seen = Vec::new();
        let mut content_len = 0usize;
        loop {
            let n = socket.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
            if let Some(pos) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
                if content_len == 0 {
                    let headers = String::from_utf8_lossy(&seen[..pos]);
                    content_len = headers
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse().ok())
                        })
                        .unwrap_or(0);
                }
                if seen.len() >= pos + 4 + content_len {
                    break;
                }
            }
        }
        *cap.lock().unwrap() = seen;
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
    });
    (format!("http://127.0.0.1:{port}/"), captured)
}

/// Serve response head plus a partial body, then hold the connection open
/// (the response never completes). Used to exercise read timeouts.
pub async fn serve_stalled(head: String, partial_body: &[u8]) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        loop {
            let n = socket.read(&mut buf).await.unwrap();
            if n == 0 || buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(partial_body).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    });
    format!("http://127.0.0.1:{port}/")
}
```

- [ ] **Step 2: Register module and dedupe webdav tests**

In `tests/unit/common/mod.rs` add `pub mod http_helpers;`. In `tests/unit/webdav/streaming_tests.rs` delete the private `serve_once` and `response_head` fns (lines 29-57), add `use crate::common::http_helpers::{response_head, serve_once};` — move `serve_once` (GET-only variant) into `http_helpers.rs` verbatim as `pub fn` so call sites are unchanged.

- [ ] **Step 3: Compile check**

Run: `cargo nextest run --all-features --locked --test unit_tests webdav::streaming_tests`
Expected: PASS (no behavior change).

- [ ] **Step 4: Commit**

```bash
git add tests/unit
git commit -m "test: share one-shot HTTP server helpers across unit tests"
```

---

### Task 2: AUDIT-001 — `sync_collection` sends `Depth: 0`

**Files:**
- Modify: `src/caldav/client.rs:352`, `src/carddav/client.rs:349`
- Test: `tests/unit/caldav/client_tests.rs`, `tests/unit/carddav/client_tests.rs`

**Interfaces:**
- Consumes: `serve_capture`, `response_head` (Task 1)
- Produces: none (behavior fix)

- [ ] **Step 1: Write the failing test (caldav)**

Append to `tests/unit/caldav/client_tests.rs`:

```rust
#[tokio::test]
async fn sync_collection_sends_depth_zero() {
    let body = b"<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"><D:sync-token>tok-1</D:sync-token></D:multistatus>".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;
    let client = CalDavClient::new(&base, None, None).unwrap();

    let sync = client.sync_collection("cal/", None, None, true).await.unwrap();
    assert_eq!(sync.sync_token.as_deref(), Some("tok-1"));

    let req = String::from_utf8_lossy(&captured.lock().unwrap());
    assert!(req.contains("Depth: 0"), "expected 'Depth: 0' in request: {req}");
}
```

Note: check how `CalDavClient::new` is called in existing tests (`(&base, None, None)` per `tests/unit/caldav/client_tests.rs:3`) and that `sync_collection("cal/", None, None, true)` matches the signature `(path, sync_token: Option<&str>, limit: Option<u32>, include_data: bool)`.

- [ ] **Step 2: Same test for carddav**

Append the identical test to `tests/unit/carddav/client_tests.rs` (replace `CalDavClient` with `CardDavClient`, call `client.sync_collection("contacts/", None, None, true)`, assert token too).

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo nextest run --all-features --locked --test unit_tests sync_collection_sends_depth_zero`
Expected: FAIL — assertion message shows `Depth: 1`.

- [ ] **Step 4: Fix both call sites**

`src/caldav/client.rs:352`: `let resp = self.report(calendar_path, Depth::Zero, &body).await?;`
`src/carddav/client.rs:349`: `let resp = self.report(addressbook_path, Depth::Zero, &body).await?;`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo nextest run --all-features --locked --test unit_tests sync_collection_sends_depth_zero`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add src/caldav/client.rs src/carddav/client.rs tests/unit
git commit -m "fix: send Depth: 0 on sync-collection REPORT (RFC 6578, AUDIT-001)"
```

---

### Task 3: AUDIT-002 — timeout on body read

**Files:**
- Modify: `src/webdav/client.rs:644-655` (`send`), `src/webdav/client.rs:660-670` (`send_stream` doc), `src/error.rs:138`
- Test: `tests/unit/webdav/streaming_tests.rs`

**Interfaces:**
- Consumes: `serve_stalled` (Task 1)
- Produces: none (behavior fix; `Error::Timeout` now also covers the body phase)

- [ ] **Step 1: Write the failing test**

Append to `tests/unit/webdav/streaming_tests.rs`:

```rust
#[tokio::test]
async fn send_returns_timeout_when_response_body_stalls() {
    let head = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n";
    let base = crate::common::http_helpers::serve_stalled(head.to_string(), b"partial").await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let err = client
        .send(
            Method::GET,
            "",
            HeaderMap::new(),
            None,
            Some(Duration::from_millis(200)),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Timeout { .. }), "expected Timeout, got: {err:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run --all-features --locked --test unit_tests send_returns_timeout_when_response_body_stalls`
Expected: FAIL (hangs then errors with an IO/IncompleteMessage error, not `Error::Timeout`; test runtime ~30 s due to stalled server hold).

- [ ] **Step 3: Implement**

In `src/webdav/client.rs`, `send()` (around line 651), replace:

```rust
let decompressed = decompress_body(body, &encodings).await?;
```

with:

```rust
let limit = per_req_timeout.unwrap_or(self.default_timeout);
let decompressed = timeout(limit, decompress_body(body, &encodings))
    .await
    .map_err(|_| Error::Timeout { limit })??;
```

(`timeout` is already imported — used at line 619.)

In `send_stream` doc comment, append: `The caller must enforce its own read deadline on the returned body; the per-request timeout covers headers only.`

In `src/error.rs` (line 138), replace the doc comment of `Timeout` with:

```rust
    /// A request phase exceeded its configured time limit.
    ///
    /// Covers receiving response headers and reading/decompressing an aggregated
    /// body, each bounded by the limit. Streaming responses (`send_stream`) and
    /// stream parsing enforce their own timeouts (30 s idle by default).
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run --all-features --locked --test unit_tests send_returns_timeout_when_response_body_stalls`
Expected: PASS (fast, ~200 ms).

- [ ] **Step 5: Commit**

```bash
git add src/webdav/client.rs src/error.rs tests/unit/webdav/streaming_tests.rs
git commit -m "fix: bound aggregated body read by the per-request timeout (AUDIT-002)"
```

---

### Task 4: AUDIT-022 — no-panic batch paths (partial)

**Files:**
- Modify: `src/webdav/client.rs:955-962` (`many`)
- Modify: `docs/superpowers/specs/2026-09-01-audit-phase0-design.md`

**Interfaces:**
- Produces: none. Deviation from spec: the two `Method::from_bytes(b"…").unwrap()` stay — the literals are compile-time known (`http` 1.5 has no const `Method::from_static`; a no-panic replacement requires changing the `propfind_many`/`report_many` return types, i.e. a breaking change deferred to the 0.10 window). Fix only the semaphore `expect` and the depth-header `unwrap`.

- [ ] **Step 1: Replace `expect`/`unwrap` inside the `many()` task block**

Replace lines 956-962:

```rust
                let _permit: OwnedSemaphorePermit =
                    sem_clone.acquire_owned().await.expect("semaphore closed");
                let mut h = HeaderMap::new();
                h.insert(
                    "Depth",
                    header::HeaderValue::from_str(depth.as_str()).unwrap(),
                );
```

with:

```rust
                let _permit: OwnedSemaphorePermit = match sem_clone.acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        return BatchItem { pub_path: p, result: Err(Error::other("semaphore closed")) };
                    }
                };
                let Ok(depth_value) = header::HeaderValue::from_str(depth.as_str()) else {
                    return BatchItem { pub_path: p, result: Err(Error::other("invalid depth value")) };
                };
                let mut h = HeaderMap::new();
                h.insert("Depth", depth_value);
```

At both `Method::from_bytes(b"PROPFIND").unwrap()` / `Method::from_bytes(b"REPORT").unwrap()` call sites (lines 911, 929) keep the `unwrap` and add: `// ponytail: static literal cannot fail; no-panic needs Result signatures (0.10 window)`.

- [ ] **Step 2: Note the deviation in the spec**

In `docs/superpowers/specs/2026-09-01-audit-phase0-design.md`, item 5, replace the sentence starting "Replace the 4 production `unwrap`/`expect`" with: "Replace the semaphore `expect` and depth-header `unwrap` with graceful `BatchItem` errors; keep the two `Method::from_bytes` unwraps on compile-time literals (removal requires `Result` signatures — breaking, deferred to 0.10)."

- [ ] **Step 3: Verify**

Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo nextest run --all-features --locked --test unit_tests`
Expected: PASS (existing batch-path tests cover the happy path).

- [ ] **Step 4: Commit**

```bash
git add src/webdav/client.rs docs/superpowers/specs/2026-09-01-audit-phase0-design.md
git commit -m "fix: replace panicking expect/unwrap in batch paths with typed errors (AUDIT-022)"
```

---

### Task 5: AUDIT-005 + AUDIT-030 — publish workflow and gitignore

**Files:**
- Modify: `.github/workflows/publish.yml`
- Modify: `.gitignore`

- [ ] **Step 1: Tag-only publish with version↔tag check**

In `publish.yml`: remove `  workflow_dispatch:` from `on:` (lines 3-6), remove the job-level `if:` (line 18), and add after the checkout step:

```yaml
      - name: Verify tag matches crate version
        run: |
          tag="${GITHUB_REF_NAME#v}"
          version=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
          if [ "$tag" != "$version" ]; then
            echo "tag v$tag does not match crate version $version" >&2
            exit 1
          fi
```

Note: `jq` is preinstalled on `ubuntu-latest`; the repo is a single-crate workspace so `packages[0]` is this crate.

- [ ] **Step 2: Gitignore env files**

Append to `.gitignore`:

```
.env
.env.*
```

(`.envrc` stays tracked — it contains only direnv config.)

- [ ] **Step 3: Validate YAML**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/publish.yml'))"` (or `actionlint` if available).
Expected: no error.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/publish.yml .gitignore
git commit -m "fix(security): tag-only publish with version check, gitignore .env (AUDIT-005, AUDIT-030)"
```

---

### Task 6: Audit docs update

**Files:**
- Modify: `docs/audit/FINDINGS.md`
- Modify: `docs/audit/REMEDIATION_PLAN.md`

- [ ] **Step 1: Mark findings as fixed**

In `FINDINGS.md`, add under each finding's metadata block (after the `Location:` line):

- AUDIT-001: `- **Status:** ✅ Fixed 2026-09-01 (phase 0).`
- AUDIT-002: `- **Status:** ✅ Fixed 2026-09-01 (phase 0).`
- AUDIT-005: `- **Status:** ✅ Fixed 2026-09-01 (phase 0; protected GitHub environment = maintainer settings action).`
- AUDIT-022: `- **Status:** ⚠️ Partially fixed 2026-09-01 (phase 0): semaphore + depth header no longer panic; the two static `Method::from_bytes` unwraps remain (infallible literals; removal needs breaking signature change, 0.10 window).`
- AUDIT-030: `- **Status:** ✅ Fixed 2026-09-01 (phase 0).`

- [ ] **Step 2: Update REMEDIATION_PLAN.md Phase 0**

Check off the six boxes with short annotations:
- AUDIT-001 ✅, AUDIT-002 ✅, AUDIT-005 ✅, AUDIT-030 ✅, AUDIT-022 ⚠️ partial (see FINDINGS), AUDIT-009 ✅ closed — add after the AUDIT-009 line:
  `Closed 2026-09-01: dedup spec executed by #111; Task 2 closed (connect already bounded by the request-level timeout); Task 4 → #79; Task 6 → AUDIT-012 (Phase 1); Task 11 closed (hyper-util legacy client defaults pool_idle_timeout to 90 s).`
- Add at the bottom of the Phase 0 section: `Note: AUDIT-006/014/028 are largely resolved by #111 (dedup executed, types unified in webdav/) and #112 (dead macro removed) — confirm at re-audit.`

- [ ] **Step 3: Commit**

```bash
git add docs/audit
git commit -m "docs(audit): mark phase 0 findings resolved, close AUDIT-009"
```

---

### Task 7: Gates, push, PR

- [ ] **Step 1: Run all gates**

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --locked --test unit_tests
cargo test --doc --all-features
cargo build --examples --all-features
cargo semver-checks --baseline-rev origin/main  # informational; expect clean (no API change)
```

Expected: all green. If `cargo semver-checks` is unavailable, skip (no public API change in this PR).

- [ ] **Step 2: Push and open PR**

```bash
git push -u origin audit/phase-0
gh pr create --title "fix(audit): phase 0 quick wins — Depth: 0 sync, body-read timeout, tag-only publish" --body "$(cat <<'EOF'
## Summary
- AUDIT-001 (High): `sync_collection` now sends `Depth: 0` per RFC 6578 §3.3 (was `Depth: 1` → 400 on strict servers)
- AUDIT-002 (High): aggregated body read/decompress is now bounded by the per-request timeout; `Error::Timeout` doc corrected; `send_stream` doc notes caller-owned deadline
- AUDIT-005 (High): `publish.yml` is tag-only with a tag↔Cargo.toml version check (protected environment = maintainer settings action)
- AUDIT-022: batch paths no longer panic on semaphore close / bad depth value (2/4 sites; the 2 static `Method::from_bytes` unwraps are infallible and need a breaking signature change → 0.10)
- AUDIT-030: `.env` patterns gitignored
- AUDIT-009 closed: dedup spec executed by #111; Task 2/11 closed as covered-by-defaults; Task 4 → #79, Task 6 → AUDIT-012; AUDIT-006/014/028 largely resolved by #111/#112

Tests: `Depth: 0` asserted via one-shot TCP server (caldav + carddav); stalled-body test returns `Error::Timeout`.

Ref #109, ref #93 (Phase 0 before Wave 3)
EOF
)"
```

- [ ] **Step 3: Comment on the tracker**

```bash
gh issue comment 109 --body "Phase 0 shipped in #<PR_NUMBER>: AUDIT-001, -002, -005, -030 fixed; AUDIT-022 partial (details in PR); AUDIT-009 closed (dedup spec executed by #111, Task 2/11 closed, Task 4 → #79, Task 6 → AUDIT-012). Audit docs updated."
```
