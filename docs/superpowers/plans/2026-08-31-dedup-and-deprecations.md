# Dedup + Deprecation Cycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut ~1300 duplicated/dead lines (PR1, API-compatible) and stage public-API removals behind `#[deprecated]` (PR1) for actual removal in a stacked PR2.

**Architecture:** Unify the near-identical caldav/carddav streaming + client-wrapper code into `webdav/` behind re-exports (zero API break), dedup webdav-internal loops, then `#[deprecated]` the API-surface items flagged in the audit. PR2 (stacked branch) deletes the deprecated items.

**Tech Stack:** Rust, hyper 1.x, quick-xml, existing unit + doc test suite as the safety net.

## Global Constraints

- Gate commands (must pass before every commit):
  - `cargo fmt --all --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run --all-features --locked --test unit_tests`
  - `cargo test --doc --all-features`
  - `cargo build --examples --all-features`
- PR1 removes **nothing** public. Only `#[deprecated]` + internal refactor.
- No `caldav/` ↔ `carddav/` copy-paste (SonarCloud gate: ≤3% dup on new code). Share via `webdav/` or `common/`.
- All pub APIs keep doc comments; README.md stays in sync.
- Skipped finding #15 (replace `escape_xml` with `quick_xml::escape`) — changes emitted XML bytes (`\n\r\t` become entities); not worth the churn. `ponytail: hand-rolled escaper kept, swap to quick_xml::escape if byte-identical output is confirmed`.

## Branch layout

- PR1: `refactor/dedup-safe` from `main`
- PR2: `refactor/remove-deprecated` **stacked on PR1**

---

## PR1 — Tasks (branch `refactor/dedup-safe`)

### Task 1: Delete dead code

**Files:** Modify `src/webdav/streaming.rs:257-270`, Delete `examples/migration.rs`

- [ ] Delete the `#[macro_export] macro_rules! impl_multistatus_on_end { … }` block (defined, never invoked).
- [ ] Delete `examples/migration.rs` — 243-line anyhow→thiserror tutorial unrelated to the crate's API; lib docs already cover error handling. (`[[test]]`/CI unaffected; `cargo build --examples` builds zero examples afterward.)
- [ ] Verify: `cargo clippy --all-targets --all-features -- -D warnings` + unit tests.
- [ ] Commit: `refactor: drop unused impl_multistatus_on_end macro and migration tutorial example`

### Task 2: Drop `HyperClientConfig` mirror struct

**Files:** Modify `src/webdav/builder.rs:329-339, 505-565`

- [ ] `build_hyper_client` takes `&WebDavClientBuilder`-equivalent params directly — pass the 9 fields positionally or restructure `build_hyper_client(pool_max_idle_per_host, pool_idle_timeout, force_http1, connect_timeout, proxy, proxy_basic_user, proxy_basic_pass, extra_root_certs_pem, danger_accept_invalid_certs)`. Simplest: one arg per field via the builder itself (`fn build_hyper_client(b: &mut WebDavClientBuilder)`) taking `std::mem::take` for the owned ones.
- [ ] Delete `HyperClientConfig`.
- [ ] Verify gates. Commit: `refactor: pass builder fields to build_hyper_client directly`

### Task 3: `webdav/client.rs` internal dedup

**Files:** Modify `src/webdav/client.rs`

- [ ] **3a — send/send_stream shared core:** extract the request construction + compression-retry loop into
  ```rust
  async fn build_and_send(&self, method: &Method, path: &str, base_headers: &HeaderMap, base_body: &Option<Bytes>)
      -> Result<Response<Incoming>>
  ```
  returning the raw response after the compression-retry loop; `send` then decompresses, `send_stream` returns it as-is. Both public fns shrink to ≤15 lines.
- [ ] **3b — batch dedup:** `propfind_many`/`report_many` delegate to one private
  ```rust
  async fn many(&self, method: Method, paths: impl IntoIterator<Item = String>, depth: Depth, xml_body: Arc<Bytes>, max_concurrency: usize) -> Vec<BatchItem<Response<Bytes>>>
  ```
- [ ] **3c — copy/move dedup:** private
  ```rust
  async fn copy_move(&self, method: &str /* "COPY"|"MOVE" */, src_path: &str, dest_absolute_url: &str, overwrite: bool) -> Result<Response<Bytes>>
  ```
  `copy`/`r#move` become one-liners.
- [ ] **3d — `normalize_decompressed_headers` drops `&self`** → free `fn normalize_decompressed_headers(headers: &mut HeaderMap, encodings: &[ContentEncoding], body_len: usize)` (update the 1 call site + tests).
- [ ] Verify gates. Commit: `refactor(webdav): dedup send paths, batch and copy/move helpers`

### Task 4: Shared decompression chain

**Files:** Modify `src/common/compression.rs`, `src/webdav/streaming.rs`

- [ ] In `common/compression.rs` add
  ```rust
  pub(crate) fn stack_decoders(mut reader: Box<dyn AsyncBufRead + Unpin + Send>, encodings: &[ContentEncoding]) -> Box<dyn AsyncBufRead + Unpin + Send>
  ```
  containing the existing 12-line match loop.
- [ ] Use it in `decompress_body` and `decompress_stream`.
- [ ] Also export `pub(crate) fn body_stream_reader(body: Incoming) -> Box<dyn AsyncBufRead + Unpin + Send>` (the BodyStream→filter_map→BufReader plumbing duplicated in streaming) and use it in `webdav/streaming.rs` Task 5.

### Task 5: Unify streaming + `DavItem` into `webdav/` (biggest cut)

**Files:** Rewrite `src/webdav/streaming.rs` (absorb both domain parsers), shrink `src/caldav/streaming.rs` + `src/carddav/streaming.rs` to re-export shims, merge `DavItem` into `src/webdav/types.rs`.

Design decisions:
- **One union `ElementName`** in webdav containing the 16 common + 8 caldav (`Calendar`, `SupportedCalendarComponentSet`, `Comp`, `CalendarData`, `CalendarDescription`, `CalendarTimezone`, `CalendarColor`, `CalendarHomeSet`) + 6 carddav (`Addressbook`, `SupportedAddressData`, `AddressDataType`, `AddressData`, `AddressbookDescription`, `AddressbookColor`, `AddressbookHomeSet`) variants.
- **One union `DavItem`** in `webdav::types` = union of both current structs (both field sets; `is_calendar` + `is_addressbook`, `calendar_data` + `address_data`, …). `apply_common` macro unchanged.
- **One `MultistatusParser`** with both domains' `on_start` branches (Comp attr / AddressDataType attr) and `handle_text` branches. Branches are path-guarded so a CalDAV server never triggers CardDAV paths.
- **One set of public parse fns** (`parse_multistatus_stream`, `_with_timeout`, `_visit`, `_visit_with_timeout`, `parse_multistatus_bytes`, `_visit`, `decode_text`, `STREAM_READ_IDLE_TIMEOUT`, `ElementName`, `element_from_bytes`, `ParseResult`).
- Shims:
  ```rust
  // src/caldav/streaming.rs
  pub use crate::webdav::streaming::*;
  ```
  (same for carddav) — glob re-export keeps `tests/unit/*/streaming_tests.rs` and lib.rs re-exports compiling untouched.
- `caldav::types::DavItem` / `carddav::types::DavItem` become `pub use crate::webdav::types::DavItem;`.

- [ ] Move + merge per design above.
- [ ] Verify gates (streaming_tests for both domains must pass unchanged). Commit: `refactor: unify multistatus streaming parser and DavItem in webdav`

### Task 6: Move `Collation`, `MatchType`, `TextMatch`, `ParamFilter` to `webdav/types.rs`

**Files:** Modify `src/webdav/types.rs`, `src/caldav/types.rs`, `src/carddav/types.rs`

- [ ] Move the four types + impls to `webdav/types.rs` (single `TextMatch::to_xml` = existing `text_match_xml`; single `ParamFilter::to_xml` = existing `param_filter_xml`; both current impls render identical output).
- [ ] `caldav/types.rs` and `carddav/types.rs`: `pub use crate::webdav::types::{Collation, MatchType, ParamFilter, TextMatch};` — all existing import paths keep resolving. CalDAV-only types (`TimeRange`, `PropFilter`, `CalendarQueryFilter`) stay put. `CardDavFilter` stays in carddav.
- [ ] Verify gates. Commit: `refactor: share filter primitives between caldav and carddav via webdav`

### Task 7: Client-wrapper dedup via macro

**Files:** Modify `src/caldav/client.rs`, `src/carddav/client.rs`, add macro in `src/webdav/client.rs`

- [ ] New `#[macro_export] macro_rules! impl_webdav_delegates!` generating, on `$client` (holding `webdav: WebDavClient`):
  - plain delegates: `build_uri`, `send`, `send_stream`, `options`, `head`, `get`, `delete`, `delete_if_match`, `copy`, `r#move`, `propfind`, `proppatch`, `report`, `mkcol`, `discover_current_user_principal`, `set_request_compression_mode`, `request_compression_mode`, `request_compression`, `propfind_many`, `report_many`, `supports_webdav_sync`, `propfind_stream`, `report_stream`
  - deprecated sugar (carry `#[deprecated(note = …)]` on the generated method): `set_request_compression`, `set_request_compression_auto`, `disable_request_compression`, `etag_from_headers`, `normalize_etag`, `normalize_sync_token`
  - Pattern: identical to existing `impl_dav_builder!` in `webdav/builder.rs`.
- [ ] Shared sync-map helpers in `webdav/types.rs` (or webdav/client.rs — pick `webdav::xml`? No: `webdav::types`):
  ```rust
  pub(crate) struct SyncRow { pub href: String, pub etag: Option<String>, pub data: Option<String>, pub status: Option<String>, pub is_deleted: bool }
  pub(crate) fn map_sync_rows(items: Vec<DavItem>, top_level_sync_token: Option<String>, headers: &HeaderMap, data_of: impl FnMut(&mut DavItem) -> Option<String>) -> (Option<String>, Vec<SyncRow>)
  ```
  caldav/carddav `map_sync_response` become ≤12 lines each. `map_calendar_objects`/`map_address_objects` (12 lines each) — leave alone or one generic; **leave alone** (tiny, and touching them re-opens the dup gate for little gain). `ponytail: 12-line twins left, fold only if Sonar flags them`.
- [ ] Keep per-client unique methods where they are (`put*`/`mkcalendar` in caldav, `put*`/`mkaddressbook` + query helpers in carddav).
- [ ] Verify gates. Commit: `refactor: generate client delegates via macro, share sync mapping`

### Task 8: Deprecations + docs sync

**Files:** `src/webdav/client.rs`, `src/caldav/client.rs`, `src/carddav/client.rs` (deprecated via macro in Task 7 where possible), `src/lib.rs`, `README.md`

- [ ] `#[deprecated(since = "0.9.0", note = "use `set_request_compression_mode` instead")]` on the 3 compression sugar methods ×3 clients (via Task 7 macro).
- [ ] `#[deprecated(since = "0.9.0", note = "use the free functions `fast_dav_rs::webdav::client::{normalize_etag, normalize_sync_token, etag_from_headers}` instead")]` on associated methods ×3 clients (via macro). Ensure free `pub fn etag_from_headers(&HeaderMap) -> Option<String>` exists in `webdav/client.rs`.
- [ ] `#[deprecated(since = "0.9.0", note = "use `fast_dav_rs::{caldav,carddav,common}::*` module paths instead")]` on the 4 `#[cfg(feature = "legacy")]` modules in `lib.rs:729-763`.
- [ ] Silence unavoidable internal delegation warnings with `#[allow(deprecated)]` scoped to the delegating items (macro-generated wrappers call deprecated `WebDavClient` methods).
- [ ] Update doc examples that demo deprecated methods (`CalDavClient::set_request_compression` example → `set_request_compression_mode(RequestCompressionMode::Force(...))`); README.md API tables mention the sugar methods → replace with `set_request_compression_mode`; add "Deprecated in 0.9" note listing removed-in-next-major items.
- [ ] Verify gates + `cargo test --doc --all-features`. Commit: `docs+chore: deprecate compression sugar, etag helpers, legacy modules`

### Task 9: Unit-test helper dedup

**Files:** Create `tests/unit/common/dav_helpers.rs` (or `tests/unit/shared/mod.rs`), Modify `tests/unit/caldav/caldav_helpers.rs`, `tests/unit/carddav/carddav_helpers.rs`, `tests/unit/mod.rs`

- [ ] Extract the ~45% mirrored helper code (auth/client fixture builders, multistatus fixtures differing only in tag/prop names) into one parametrized module; caldav/carddav helpers become thin wrappers.
- [ ] Verify gates. Commit: `test: dedup caldav/carddav unit helpers`

### Task 10: PR1 final gate + push

- [ ] Full gate sequence (all 5 commands).
- [ ] `git push -u origin refactor/dedup-safe`, open PR with summary + note on SonarCloud gates.

---

## PR2 — Tasks (branch `refactor/remove-deprecated`, stacked on PR1)

### Task P2-1: Remove deprecated API

**Files:** `src/webdav/client.rs`, `src/webdav/client.rs` (macro arms), `src/caldav/client.rs`, `src/carddav/client.rs`, `src/lib.rs`, `Cargo.toml`, `README.md`

- [ ] Delete from `impl_webdav_delegates!` the 6 deprecated method arms (×3 clients total).
- [ ] Delete the 4 `#[cfg(feature = "legacy")]` modules from `lib.rs`.
- [ ] Remove `legacy = []` from `[features]` in `Cargo.toml` + any README mention.
- [ ] Fix any doc/test references.
- [ ] Full gate. Commit: `feat!: remove APIs deprecated in 0.9`

### Task P2-2: Push + PR (merge after PR1)

- [ ] `git push -u origin refactor/remove-deprecated`, PR notes: "depends on PR1; bump minor version per semver (0.x removals allowed in minor bump)". Merge order: PR1 then PR2.
