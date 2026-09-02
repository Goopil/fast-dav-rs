# Findings — fast-dav-rs 0.9.0 Technical Audit

**Audit date:** 2026-08-31 · **Auditor:** independent external audit · **Scope:** full repository at commit `f68d692` (tag `0.9.0` + 4 untagged commits)

**Method:** 4 parallel evidence passes (HTTP/concurrency, XML/architecture, duplication/API, tests/CI/ops) followed by independent spot-verification of every High finding at source level, then a red-team second pass. Dynamic checks executed: `cargo clippy --all-targets --all-features -- -D warnings` (**PASS**, 0 warnings), `cargo nextest run --test unit_tests --all-features --locked` (**379/379 PASS**), `cargo audit` (**0 advisories**, 141 deps scanned).

**Confidence levels:** `Confirmed` = demonstrated from repository contents (file:line verified by two independent passes or by direct re-reading). `Likely` = strong evidence, one external fact missing. `Needs verification` = credible risk, cannot conclude from repo alone.

**Historical context (AUDIT-009):** the repository already contains a previous audit fix plan (`docs/superpowers/plans/2026-08-20-audit-fixes.md`, 12 tasks) and a dedup design spec (`docs/superpowers/specs/2026-08-20-dedup-macros-design.md`). Several of those tasks were never executed — this is tracked as AUDIT-009 and cross-referenced in individual findings.

---

## AUDIT-001 — `sync-collection` REPORT sent with `Depth: 1`; RFC 6578 mandates `Depth: 0`

- **Severity:** High
- **Confidence:** Confirmed
- **Domain:** Correctness / Interoperability
- **Location:** `src/caldav/client.rs:584`, `src/carddav/client.rs:558`
- **Status:** ✅ Fixed 2026-09-01 (phase 0).

### Problem
Both `sync_collection()` methods send the `sync-collection` REPORT with header `Depth: 1`. RFC 6578 §3.2/§3.3 defines the report **only for `Depth: 0`** ("other values result in a 400 (Bad Request) error response"); scoping is expressed by `<D:sync-level>` (already present in the body, `src/webdav/xml.rs:95`).

### Evidence
```rust
let resp = self.report(calendar_path, Depth::One, &body).await?;   // caldav/client.rs:584
let resp = self.report(addressbook_path, Depth::One, &body).await?; // carddav/client.rs:558
```

### Impact
Every `sync_collection()` call fails with `400` against any RFC-strict server. Lenient servers (Sabre/DAV, Radicale, Nextcloud — the e2e fixture) ignore `Depth` on REPORT, which is why tests pass. This is a **silent time bomb**: it works today against permissive servers and breaks the library's core sync feature against stricter ones (e.g. some Apache Jackrabbit/DAVx test servers).

### Trigger
Any server implementing RFC 6578 strictly.

### Verification
Send `REPORT` with `Depth: 1` vs `Depth: 0` against a strict server (or add an e2e assertion); RFC 6578 §3.2 text is unambiguous.

### Recommended fix
Use `Depth::Zero` for sync-collection (body already carries `sync-level`). Two-line change, covered by existing parser tests.

### Regression risk
None for lenient servers; strict servers start working.

---

## AUDIT-002 — Response body read has no timeout; requests can hang forever despite `default_timeout`

- **Severity:** High
- **Confidence:** Confirmed
- **Domain:** Reliability / Stability
- **Location:** `src/webdav/client.rs:592-609` (aggregated path), `src/webdav/client.rs:619-688` (streaming path), doc claim at `src/error.rs:138-144`
- **Status:** ✅ Fixed 2026-09-01 (phase 0).

### Problem
The per-request timeout wraps only the future up to response **headers**:
```rust
let resp = timeout(limit, fut).await
    .map_err(|_| Error::Timeout { limit })?          // client.rs:594-596
...
let decompressed = decompress_body(body, &encodings).await?;  // client.rs:609 — OUTSIDE any timeout
```
Body reads (`decompress_body`, `client.rs:609`) and raw `send_stream` consumption run with **no timeout at all**. Only the streaming *XML parsers* add a 30 s idle timeout (`src/caldav/streaming.rs:23,351-355`).

### Evidence
Direct read of `send()` (client.rs:539-614): `timeout(limit, fut)` covers `self.client.request(req)` only; `decompress_body` is awaited bare.

### Impact
A server (or middlebox) that sends headers then stalls → `send()` hangs **indefinitely** on every high-level method (`list_calendars`, `calendar_query`, `sync_collection`, …). `Error::Timeout`'s documented contract ("operation exceeded its configured time limit") is false for the body phase. At 3 a.m., this manifests as wedged workers with no error and no log (see AUDIT-010).

### Trigger
Slow/stalled server response bodies, trickling responses, dead keep-alive connections between header and body.

### Verification
Mock server: send `200 OK` + `Content-Length: 100` + 10 bytes, then sleep forever → `send()` never returns; `default_timeout` is not honored.

### Recommended fix
Wrap the body-read/decompress phase in the same (or a second) timeout, or enforce a total-request deadline spanning headers+body. Cover `send_stream` by documenting that callers must apply their own timeout (or return a body wrapper with an enforced read deadline).

### Alternatives
Add `hyper-util`'s built-in timeouts where applicable (connect/keep-alive) — necessary but insufficient; the body-phase timeout must be client-side.

### Regression risk
Low. Slow-but-healthy large downloads need the timeout to be generous and documented (idle-based, not total-based, is the safer semantic — mirroring the streaming parser's 30 s idle timeout).

---

## AUDIT-003 — Unbounded response buffering; no decompression-size cap (decompression bomb)

- **Severity:** High
- **Confidence:** Confirmed
- **Domain:** Security / Performance / Stability
- **Location:** `src/common/compression.rs:187-210` (`decompress_body`), `src/webdav/client.rs:609`, `src/common/compression.rs:216-238` (`decompress_stream`), aggregate parse sinks at `src/caldav/streaming.rs:444` / `src/carddav/streaming.rs:444`

### Problem
Every high-level method funnels through `send()`, which buffers the **entire decompressed response** into a `Vec` with no maximum size and no cross-check against wire `Content-Length`:
```rust
let mut out = Vec::with_capacity(32 * 1024);   // compression.rs:194
decoder.read_to_end(&mut out).await?;          // compression.rs:207 — unbounded
```
`decompress_stream` and the streaming parser have no byte cap either (a `visit` consumer still admits unbounded decompressed bytes). The aggregate parse sink additionally accumulates every `DavItem` — including full `calendar_data`/`address_data` strings — before the caller sees anything (`caldav/streaming.rs:444`).

### Evidence
`decompress_body` implementation read in full; no max-size parameter exists anywhere in the compression or client APIs (grep: `max.*body|body.*size` → no production hits). A previous audit task ("Task 4: Add max response body size guard to `decompress_body`", `docs/superpowers/plans/2026-08-20-audit-fixes.md:471`) was **never implemented**.

### Impact
A malicious or misbehaving server (or MITM when `danger_accept_invalid_certs` is on) can send a small gzip body that expands to gigabytes → OOM kill of the embedding process. `Vec` growth doubles peak memory. With 10× collection sizes, normal (non-hostile) syncs with `include_data = true` have the same failure mode.

### Trigger
`Content-Encoding: gzip` + inflated payload; or simply very large legitimate collections.

### Verification
Serve 10 MB gzip of zeros (~10 GB decompressed) to any high-level method; observe memory climb without any client-side error.

### Recommended fix
Add `max_response_body_size` (default sane, e.g. 256 MB) enforced in `decompress_body` and as a running counter in `decompress_stream`; return a dedicated error variant when exceeded. Optional hardening: reject responses whose decompressed length exceeds the wire `Content-Length` by more than the natural ratio.

### Regression risk
Low if the default is generous and the limit is builder-configurable.

---

## AUDIT-004 — Credentials are sent to whatever absolute URL is passed, including server-controlled hrefs

- **Severity:** High
- **Confidence:** Confirmed
- **Domain:** Security
- **Location:** `src/webdav/client.rs:263-267` (`build_uri` absolute passthrough), `src/webdav/client.rs:559-561` / `639-641` (unconditional `Authorization` attach), discovery returns raw server hrefs at `src/caldav/client.rs:406-412`

### Problem
`build_uri` accepts any `http(s)://…` string verbatim, and `send`/`send_stream` attach the `Authorization` header unconditionally to the resulting URI. There is no origin check against the configured `base`. Standard usage feeds server-supplied hrefs (multistatus `<D:href>`, principal/home-set URLs, `SyncItem.href`) back into `get`/`delete_if_match`/`propfind`.

### Evidence
```rust
if path.starts_with("http://") || path.starts_with("https://") { return path.parse()... }  // client.rs:263-267
if let Some(ref auth_header) = auth { req_builder = req_builder.header(header::AUTHORIZATION, auth_header); } // client.rs:559-561
```
The repo's own test (`client.rs:1521-1527`) demonstrates host replacement works.

### Impact
A hostile or compromised collection (cross-tenant server, attacker-influenced href) causes Basic/Bearer credentials to be transmitted to a different origin. No redirects are followed by the library, so this is the one credential-egress path.

### Trigger
Server returns absolute hrefs pointing at another host, or caller passes attacker-influenced strings.

### Verification
Unit test: client built with base `https://a.example`, call `get("https://evil.example/x")` with a TCP listener on evil origin → Authorization header present.

### Recommended fix
Compare the resolved URI's scheme+host(+port) with `self.base` origin; refuse (or strip credentials for) cross-origin requests unless explicitly allow-listed. Cheap: one check in `build_uri`/`send`.

### Alternatives
Document-only mitigation is insufficient — the failure is silent by construction.

### Regression risk
Servers legitimately serving content from CDN hosts would need the allow-list; default should stay strict.

---

## AUDIT-005 — `publish.yml` publishes any branch via `workflow_dispatch`; no tag/version guard

- **Severity:** High
- **Confidence:** Confirmed
- **Domain:** Operations / Supply chain
- **Location:** `.github/workflows/publish.yml:18`
- **Status:** ✅ Fixed 2026-09-01 (phase 0; protected GitHub environment = maintainer settings action).

### Problem
```yaml
if: github.event_name == 'workflow_dispatch' || startsWith(github.ref, 'refs/tags/')
```
The `workflow_dispatch` arm short-circuits the tag check: dispatching the workflow from **any ref** publishes that ref's code to crates.io with `CARGO_REGISTRY_TOKEN`. There is no `environment:` protection, no version↔tag match, and HEAD currently sits at version `0.9.0` with 4 post-tag commits — a dispatch today would attempt re-publishing `0.9.0` (crates.io rejects it, but the guard is accidental, not designed).

### Evidence
Workflow file read in full (44 lines). Mitigants that do exist: `permissions: contents: read`, `--locked --dry-run` step, no `pull_request_target` anywhere in the repo.

### Impact
One mis-click (or a compromised maintainer token) publishes unreviewed code to crates.io. Not attacker-exploitable from forks (secrets unavailable), but it removes the human gate between "code" and "public release".

### Trigger
Manual workflow dispatch from any branch.

### Verification
`gh workflow run publish --ref <any-branch>` in a test repo with a dummy token; observe the publish job starts.

### Recommended fix
`if: startsWith(github.ref, 'refs/tags/')` only; keep `workflow_dispatch` behind a version-matching guard (`grep "^version" Cargo.toml` == tag) and add a GitHub `environment: crates-io` with required reviewers.

---

## AUDIT-006 — caldav↔carddav duplication (78–92%) with behavioral divergences

- **Severity:** High
- **Confidence:** Confirmed
- **Domain:** Architecture / Maintainability
- **Location:** `src/caldav/client.rs` vs `src/carddav/client.rs` (78% overall; delegation region 95.9%); `src/caldav/streaming.rs` vs `src/carddav/streaming.rs` (92%); `src/caldav/types.rs` vs `src/carddav/types.rs` (5 types 100% identical)

### Problem
The two protocol modules re-implement each other's machinery with pure renames. Measured divergences — one copy has a fix/feature the other lacks:

| Divergence | caldav | carddav |
|---|---|---|
| MKCOL fallback on 501/405 when creating a collection | **absent** (`mkcalendar`, client.rs:362-376) | **present** (`mkaddressbook`, client.rs:379-397) |
| Input validation before network | yes (`validate_component_name`/`validate_utc_datetime`, client.rs:469-533) | **no** — `addressbook_query` splices raw `filter_xml` (client.rs:467-473) |
| Content-Type literal | string literal ×3 (client.rs:241,272,289) | constant `VCARD_CONTENT_TYPE` (client.rs:22) |
| Multi-line text accumulation | `CalendarTimezone` block (streaming.rs:271-283) | absent (latent) |

Additionally `carddav/types.rs:82-93,129-140` re-implements `text_match_xml`/`param_filter_xml` inline instead of sharing `src/webdav/xml.rs:112-132`.

### Evidence
Token-level diff: 33% of all client tokens sit in two ≥70-token identical blocks; `parse_multistatus_stream_with` byte-identical between both streaming files. Git history shows the failure mode: PR #101 fixed CardDAV filtering, PR #103 then re-implemented adjacent machinery on the CalDAV side. The repo's own SonarCloud gate (≤3% duplication on new code) does not reflect this standing reality. A dedup design spec exists (`docs/superpowers/specs/2026-08-20-dedup-macros-design.md`) and was **never executed** (see AUDIT-009).

### Impact
Every bug fix must be applied twice (or silently diverges — it already has). The `mkcalendar` gap is a user-visible interop bug; the `filter_xml` asymmetry is a security-relevant API inconsistency.

### Trigger
Ongoing development; every PR touching either side.

### Verification
`diff <(sed 's/calendar/addressbook/g' src/caldav/streaming.rs) src/carddav/streaming.rs | wc -l` → 132/505 lines.

### Recommended fix
Execute the written dedup spec: parameterize `MultistatusParser` over an element-name trait, share the delegation region via a macro (the `impl_dav_builder!` pattern at `src/webdav/builder.rs:574-670` already proves the approach), unify `TextMatch`/`Collation`/`MatchType`/`ParamFilter` in `webdav/types.rs`, and port the MKCOL fallback + validation to caldav regardless of dedup progress (root-cause fixes for the divergences).

### Regression risk
Medium (large refactor). Mitigate by landing the two divergence fixes **first** (small, independent), then dedup with the existing 379 tests green.

---

## AUDIT-007 — Silent one-shot retry of non-idempotent mutations on 400/415/501

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Data integrity
- **Location:** `src/webdav/client.rs:599-604` (`send`), duplicated at `679-684` (`send_stream`)

### Problem
When a body-carrying request is sent with a negotiated request encoding and the server answers 415/501/**400**, the request is re-sent once uncompressed. `send` is generic: `PUT`, `PROPPATCH`, `MKCALENDAR`, `MKCOL`, `MOVE` all pass through it. If a server produced 400/415 *after* applying a side effect, the mutation is executed twice. The first response body is dropped without being read (pooled HTTP/1.1 connection discarded).

### Evidence
```rust
let should_retry = self.handle_request_compression_outcome(attempted_encoding, resp.status());
if should_retry && attempt == 0 && base_body.is_some() { attempt += 1; continue; }  // client.rs:599-604
```
Bounded to one retry (`attempt == 0`) — this is not a retry storm, and PUT+`If-Match` is conditional (safe). Unconditional PUT is content-idempotent; the residual risk is `PROPPATCH`/`MOVE`/custom bodies against non-strict servers.

### Impact
Low-probability duplicate side effects; also invisible: the caller sees only the second response and cannot learn the first attempt happened.

### Trigger
Server that answers 400/415 after side effect, with request compression negotiated (Auto mode).

### Verification
Mock: first PUT with `Content-Encoding: gzip` → 400 (record side effect), second → 201; observe two writes.

### Recommended fix
Restrict the automatic retry to idempotent methods (`PROPFIND`/`REPORT`/`GET` — note REPORT bodies are read-only) or to PUT only when `If-Match`/`If-None-Match` is present; otherwise surface an `Error` telling the caller to disable request compression.

---

## AUDIT-008 — Weak ETags accepted for `If-Match` → guaranteed `412` on strict servers

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Correctness / Data integrity
- **Location:** `src/webdav/client.rs:62-69` (`if_match_header_value`), used by `put_if_match` (`caldav/client.rs:274`, `carddav/client.rs:275`) and `delete_if_match` (`webdav/client.rs:759-763`)

### Problem
A weak etag `W/"abc"` is validated and re-emitted as `W/"abc"` in `If-Match`. RFC 9110 mandates **strong comparison** for `If-Match`: weak validators never match, so the conditional operation can never succeed on a compliant server.

### Evidence
```rust
if let Some(opaque) = etag.strip_prefix("W/") {
    validate_opaque_tag(opaque)?;
    let value = format!("W/\"{opaque}\"");   // client.rs:62-65
```

### Impact
Silent operation failure with an opaque `412 Precondition Failed`; callers implementing optimistic concurrency with weak etags (common from some servers' `getetag` values) get a permanently broken write path. Missing-etag case is well handled (`Error::InvalidEtag { Empty }`); non-ASCII etags are silently dropped by `etag_from_headers` (client.rs:888-894 — see AUDIT-025).

### Trigger
Server issues `W/"…"` etags; caller feeds them back via `put_if_match`.

### Verification
Unit test: `if_match_header_value("W/\"abc\"")` → currently returns a header; assert instead a distinct error or strong-conversion path.

### Recommended fix
Reject weak etags for `If-Match` with a dedicated `EtagReason::Weak` error, or offer an explicit `if_match_allow_weak` escape hatch. Keep accepting weak tags for informational use.

---

## AUDIT-009 — Previous audit remediation plan written but never executed (process debt)

- **Severity:** High (process) / the individual items are tracked separately
- **Confidence:** Confirmed
- **Domain:** Operations / Process
- **Location:** `docs/superpowers/plans/2026-08-20-audit-fixes.md` (12 tasks) vs current source

### Problem
The repository contains a prior audit's fix plan plus a dedup design spec. Direct comparison against current source shows several tasks were **never implemented**:

| Planned (2026-08-20) | Status at HEAD |
|---|---|
| Task 1: fix `expect("semaphore closed")` | **still present** (`webdav/client.rs:928,979`) |
| Task 2: default `connect_timeout` 10 s | **not done** ("Default: **none**", builder.rs:170-174) |
| Task 4: max response body size guard | **not done** (→ AUDIT-003) |
| Task 6: probe 5 s hardcoded timeout respects config | **not done** (`client.rs:426`) |
| Task 11: default `pool_idle_timeout` | **not done** ("Default: **unbounded**", builder.rs:197) |
| Dedup spec (`specs/2026-08-20-dedup-macros-design.md`) | **not done** (→ AUDIT-006) |

Other rounds (typed errors, PR #69 fixes, strict corrections) *were* executed — the code shows `Operation` enum, `#[non_exhaustive]`, legacy gating, zeroize, proxy validation.

### Impact
Known, documented, planned fixes silently expired. Any audit or review re-discovers them at full cost. This is the highest-leverage process finding in the report: the backlog already exists; it only needs execution.

### Recommended fix
Re-baseline: fold the unexecuted 2026-08-20 tasks into `REMEDIATION_PLAN.md` Phase 0/1 (they overlap with AUDIT-002/003/012/022), and adopt a rule that committed fix plans are either executed or explicitly closed with a decision note.

---

## AUDIT-010 — Zero observability hooks: no `log`/`tracing` support at all

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Observability / Operations
- **Location:** whole `src/` (grep `tracing|log::|env_logger` → 0 hits)

### Problem
The crate emits nothing: no request/response logging, no timing, no retry/probe diagnostics, no debug-level wire traces. The only diagnostics are typed errors (good, but they only fire when something *already failed*).

### Impact
At 3 a.m., a wedged `send()` (AUDIT-002), a compression probe that silently pinned `Identity` (AUDIT-012), or a permanently-`false` `supports_webdav_sync` (AUDIT-013) produce **no signal whatsoever**. Embedding applications cannot see what the client is doing without wrapping every call themselves.

### Recommended fix
Add an optional `tracing` feature (feature-gated dep, `tracing` crate); instrument: request start/finish (method+path+status+duration), timeout hit, compression probe outcome + negotiated encoding, retry taken, decompressed size. ~1 day of work, no breaking change.

---

## AUDIT-011 — `danger_accept_invalid_certs` ungated in release builds; plain `http://` accepted with credentials by default

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Security
- **Location:** `src/webdav/builder.rs:405-466` (`NoVerify`, debug-only `eprintln!` at 455-459), `src/webdav/builder.rs:525,546` (`https_or_http()`)

### Problem
`NoVerify` disables all certificate and hostname verification; the only warning is a `#[cfg(debug_assertions)]` eprintln — **silent in release builds**. Separately, `http://` base URLs are allowed by default while credentials are attached to every request; docs warn (`builder.rs:133-139`) but nothing enforces or warns at runtime.

### Impact
Footguns, not exploits: release builds can ship without TLS verification and with credentials in cleartext, invisibly.

### Recommended fix
In release builds, make `danger_accept_invalid_certs` require an explicit opt-in acknowledgment (e.g. separate builder method `accept_danger_without_tls_verification(true)`), and emit a one-time warning through `log`/`tracing` once AUDIT-010 lands. For `http://` + configured credentials, emit the same. Keep both possible (escape hatches for self-hosted/internal use) but loud.

---

## AUDIT-012 — Compression probe: hardcoded 5 s timeout, head-of-line blocking, failure permanently pins `Identity`

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Reliability / Performance
- **Location:** `src/webdav/client.rs:353-441` (probe, 5 s at :426), `client.rs:490-498` (Mutex held across probe), `client.rs:428-440` (failure → `Identity`)
- **Status:** ✅ Fixed 2026-09-02 (v1.1 audit wave 2): failed probes no longer cache — the negotiation state stays unset so the next request re-probes while the current one proceeds uncompressed; the probe timeout now derives from the client's `default_timeout`; completed probes still cache the server's answer (including `Identity`).

### Problem
On the first body-carrying request in `Auto` mode, all concurrent callers serialize behind a tokio `Mutex` held across a hidden PROPFIND probe (up to 5 s). Any transient probe failure (network, auth, timeout) silently sets `negotiated = Some(Identity)` **permanently** for the client and all clones; recovery requires the caller to know about `set_request_compression_auto()` (client.rs:231) — undocumented.

### Evidence
```rust
let _probe_guard = self.request_compression_probe.lock().await;   // client.rs:490-491
...
_ => { self.set_negotiated_encoding(Some(ContentEncoding::Identity)); }  // client.rs:428-440 (silent)
```

### Impact
Latency cliff (all first-wave body requests blocked ≤5 s behind one probe); permanent silent loss of request compression after one hiccup; per-instance probe cost for serverless patterns that build a client per request.

### Recommended fix
On probe failure, mark the mode "unknown, retry next time" instead of pinning `Identity`; make the probe timeout derive from `default_timeout`; double-check lock scope (probe under the mutex is fine, but the *cache write* should be the only shared mutation). Document the per-instance cost for short-lived clients.

---

## AUDIT-013 — `supports_webdav_sync`: substring match over whole body + swallowed errors → false "unsupported"

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Reliability / Correctness
- **Location:** `src/webdav/client.rs:1023-1055`

### Problem
Primary detection greps `"sync-collection"` (lowercased) over the **entire** PROPFIND body — a displayname, comment, or property value containing the string yields a false positive. All PROPFIND errors are swallowed (`if let Ok(response)`), and the fallback REPORT maps any error to `Ok(false)` — an auth failure or network error is indistinguishable from genuine non-support.

### Impact
Callers gating sync features on this probe get wrong answers silently. Result is also uncached (see AUDIT-027).

### Recommended fix
Parse `supported-report-set` with the existing multistatus parser (look for `<D:sync-collection>` inside `supported-report` elements) instead of substring search; distinguish "transport/auth error" from "not supported" in the return type (or propagate the error).

---

## AUDIT-014 — Crate-root re-export type trap: carddav's `TextMatch`/`Collation`/`MatchType`/`ParamFilter`; `DavItem`/`SyncItem`/`SyncResponse` name collisions

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Developer experience / Architecture
- **Location:** `src/lib.rs:711-720`, type definitions at `src/caldav/types.rs:114,135,162,247` vs `src/carddav/types.rs:11,32,59,99`

### Problem
`Collation`, `MatchType`, `TextMatch`, `ParamFilter` are defined **twice** (separate, non-re-exported types); the crate root re-exports only carddav's. A user writing `use fast_dav_rs::{TextMatch}` and passing it to a `caldav::CalendarQueryFilter` gets a type error. `DavItem`, `SyncItem`, `SyncResponse` are unrelated same-named types in both modules; the root binds caldav's.

### Impact
Guaranteed confusion and compile errors for anyone writing generic CalDAV/CardDAV code against the root namespace; five duplicated type definitions that can drift.

### Recommended fix
Fold these types into `webdav` (or a shared `types` module) and re-export once. Breaking change → do it together with the AUDIT-006 dedup in a 0.10 semver-minor window with deprecation aliases.

---

## AUDIT-015 — Silent partial-failure handling in multistatus flows

- **Severity:** Medium
- **Confidence:** Confirmed (parse_error_body, 507 shape) / Needs verification (per-item propstat failures)
- **Domain:** Data integrity
- **Location:** `src/webdav/streaming.rs:399-407`, `src/caldav/client.rs:837-841` / `src/carddav/client.rs:881-885`, `src/webdav/client.rs:1032` pattern
- **Status:** ✅ Fixed 2026-09-02 (v1.1 audit wave 2, batch 2): both `SyncResponse` types (CalDAV/CardDAV, kept distinct per AUDIT-006 scope) gain `truncated: bool`, set by the shared `map_sync_rows` when any response element carries a `507` status — detection runs before the collection heuristic so the flag is never suppressed; per-item statuses pass through unchanged. `parse_error_body` reports malformed error bodies via the new `webdav::WebDavError::parse_failed` flag instead of a silent default. The part-3 collection heuristic is documented on `map_sync_response`/`SyncResponse` (behavior intentionally unchanged, per audit).

### Problem
Three related silent-degradation paths:
1. `parse_error_body` returns `Ok(WebDavError::default())` for malformed `<D:error>` bodies — documented, but callers cannot distinguish "no error body" from "garbage error body" (a hostile server can suppress precondition diagnostics).
2. RFC 6578 truncation: a server responding `507 Insufficient Storage` for the request URI surfaces only as an ordinary `SyncItem` with `status: Some("HTTP/1.1 507 ...")` — no first-class signal; clients must notice it themselves or silently believe the sync is complete.
3. `map_sync_response` drops items where `sync_token.is_some() && etag.is_none()` as "collection" — a hostile server echoing the collection token on member responses silently removes items from `SyncResponse.items`.

### Impact
"HTTP 200/success with operation not actually complete" — exactly the class the audit brief prioritizes. Sync loops can under-report changes.

### Verification
Unit tests: (a) multistatus with one 403 propstat → does the item surface with its status or vanish? (b) response with `507` status on the request-URI → assert a first-class signal exists. Today: (b) definitely fails.

### Recommended fix
Expose `SyncResponse.truncated: bool` (set when the request-URI carries 507) and keep per-item status; add `WebDavError::parse_failed` marker instead of a silent default. Document the collection heuristic.

---

## AUDIT-016 — Unbounded memory in aggregate parse paths (all items buffered, including data payloads)

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Performance / Scalability
- **Location:** `src/caldav/streaming.rs:444` / `src/carddav/streaming.rs:444` (`Vec::<DavItem>` sink), high-level methods aggregating via `parse_multistatus_bytes` (e.g. `caldav/client.rs:586-594`)

### Problem
Even the "streaming" default (`parse_multistatus_stream`) accumulates every `DavItem` — including full `calendar_data`/`address_data` strings — into a `Vec` before returning. Combined with AUDIT-003 (whole body buffered) a multiget over a large collection holds body + items + data strings simultaneously.

### Impact
Memory ∝ collection size × item size. 10× data ⇒ 10× peak RSS, with the doubling behavior of `Vec`/`String` growth on top. The `_visit` variants are the only true streaming path and are not the advertised default.

### Recommended fix
Document the memory model prominently; make `_visit` the documented default for large syncs; consider a `max_items`/`max_data_bytes` guard. No behavior change required.

---

## AUDIT-017 — Test fidelity gaps: what slips past the suite today

- **Severity:** High (given this is a network library at 0.9)
- **Confidence:** Confirmed
- **Domain:** Testing
- **Location:** `tests/unit/**` (fakes at `caldav/streaming_tests.rs:11-64`, `common/compression_tests.rs:10-48`, `caldav/etag_tests.rs:8-33`), soft e2e asserts (e.g. `tests/e2e/caldav/parallel/parallel_tests.rs:106-112`)

### Problem
Unit tests are ~90% offline parser/builder tests. **No test drives a full request through the real client's pooled hyper-util stack.** Never exercised anywhere: chunked transfer-encoding (zero hits for `chunked` in tests/), HTTP/2 (e2e stack is nginx→fastcgi HTTP/1.1; "✅ HTTP/2" claim at `tests/e2e/caldav/README.md:142` is unproven), redirects, auth/User-Agent headers on the wire (assert never made; the `#[cfg(test)] auth_header()` accessor at `client.rs:213-216` is unused), the Auto-compression probe/negotiation cache via HTTP (every client test first calls `disable_request_compression()`), client-level `default_timeout` firing, connection reuse, compression×streaming combined, concurrent requests against shared `Arc<RwLock>` state, lock-poisoning recovery. E2E asserts are frequently println-and-continue.

**Bugs I could introduce today that no test would catch:** break Auto-mode negotiation; race the compression caches; drop `Authorization` attachment; break client-level timeout; regress chunked/h2 handling; silently degrade e2e coverage via soft asserts.

### Recommended fix
Priority order: (1) one wire-level mock (tokio TcpListener) driving `WebDavClient` end-to-end asserting method/path/headers/auth; (2) chunked + compressed-multistatus response fakes; (3) Auto-probe happy/sad path test; (4) a `tokio::time::pause` timeout test; (5) convert soft e2e asserts to hard ones for sync/compression categories.

---

## AUDIT-018 — E2E harness defects: dead env contract, docker-compose v1, exit-0 masking, lenient readiness

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Testing / Operations
- **Location:** `.github/workflows/e2e-tests.yml:89-94,101-104,117-120`, `sabredav-test/reset-db.sh:1-12`

### Problem
1. CI exports `CALDAV_SERVER_URL/USERNAME/PASSWORD` — **nothing reads them** (grep over tests/: 0 hits; tests hardcode `http://localhost:8080/`, `test`/`test`). Changing the server in CI has no effect.
2. `reset-db.sh` has no `set -e`; the final `echo` masks `DROP DATABASE`/seed failures → CI step can silently no-op. It also calls `docker-compose` (v1 spelling) while CI installs compose v2.
3. Readiness check accepts 200/**401**/207 — a server that 401s everything passes health check, then soft-assert e2e runs green.
4. SabreDAV README claims zstd module; nginx image builds brotli only — response `Content-Encoding` is never asserted in e2e.

### Impact
Green CI with a broken or wrong-target e2e environment; false confidence in the sync/compression matrices.

### Recommended fix
Make tests read the env vars (fallback to current constants); `set -euo pipefail` in reset-db.sh; migrate to `docker compose`; readiness = authenticated PROPFIND returning 207; assert response `Content-Encoding` in compression e2e.

---

## AUDIT-019 — CI gaps: no job timeouts, fork-PR coverage failure, minimal supply-chain scanning, MSRV check-only

- **Severity:** Medium
- **Confidence:** Confirmed
- **Domain:** Operations
- **Location:** all 5 files in `.github/workflows/`

### Problem
1. No `timeout-minutes:` anywhere (default 6 h burn on hang).
2. `coverage.yml` runs on `pull_request` with `SONAR_TOKEN` + `fail_ci_if_error: true` → external fork PRs fail coverage (secrets unavailable).
3. No action SHA-pinning; no `cargo-deny` (licenses/bans); `rustsec/audit-check` only covers RustSec.
4. MSRV job is `cargo check` only (no test run on 1.85).
5. `e2e-tests.yml` duplicates the unit job **weaker** (no `--all-features`, no `--locked`, no nextest).

### Impact
Fork PRs show red coverage (contributor friction); supply-chain posture thinner than the audit-check badge suggests; hangs burn 6 h runners.

### Recommended fix
Add `timeout-minutes: 30` to every job; `continue-on-error` (or tokenless mode) for fork PRs on Sonar; add `cargo-deny` with a small `deny.toml`; run nextest on MSRV; drop or strengthen the duplicate unit job.

---

## AUDIT-020 — Namespace-blind, case-insensitive XML element matching

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Correctness
- **Location:** `src/webdav/streaming.rs:26-32`, `src/caldav/streaming.rs:53-57`, `src/carddav/streaming.rs:52-56`

### Problem
Element identity = local name after stripping the prefix, matched case-insensitively. No namespace URI is ever compared (no `NsReader`). Foreign-namespace elements with colliding local names parse as DAV elements; case-insensitivity accepts `<RESPONSE>` (XML names are case-sensitive).

### Impact
Bounded by the server already being the trust boundary; realistic risk is collision with extension properties (e.g. Apple names) or spoofed nested elements. Also means the parser accepts everything — the inverse (rejecting valid servers) does not occur.

### Recommended fix
Match on resolved namespace + local name (quick-xml `NsReader`), or at minimum case-sensitive local names. Medium-effort, do alongside AUDIT-006 dedup.

---

## AUDIT-021 — Invalid UTF-8 → lossy substitution; interleaved text → last chunk wins

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Correctness
- **Location:** `src/webdav/streaming.rs:272-277`, `src/caldav/streaming.rs:366-368`, `src/carddav/streaming.rs:366-368` (UTF-8); `src/webdav/streaming.rs:173-245` (overwrite pattern)

### Problem
Non-UTF-8 text is silently `from_utf8_lossy`'d (and skips entity unescaping for that chunk). For non-data fields, each `Text` event **overwrites** the previous value — `<D:href>abc<!---->def</D:href>` (valid XML: comment inside element) yields `"def"`, silently dropping `"abc"`. Data elements correctly append.

### Impact
Spec-legal-but-rare server output produces silently wrong values (hrefs/etags/sync-tokens). Low probability, but zero warning when it fires.

### Recommended fix
Append text for all fields (then trim once at `finish()`); propagate an `Error::XmlEncoding` for invalid UTF-8 instead of lossy conversion (or log via AUDIT-010 instrumentation).

---

## AUDIT-022 — Panic-capable `unwrap`/`expect` in production batch paths (previously flagged, unfixed)

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Stability
- **Location:** `src/webdav/client.rs:928,932,940,979,983,991`
- **Status:** ✅ Closed 2026-09-01 (phase 0) — assessed as statically infallible: the two `Method::from_bytes` literals and the enum-controlled `Depth::as_str()` cannot fail, and the batch semaphore is private and never closed. `expect`/`unwrap` retained with `ponytail:` markers documenting each invariant; typed-error conversion requires breaking `Result` signatures — re-evaluate in the 0.10 window.

### Problem
`acquire_owned().await.expect("semaphore closed")`, `HeaderValue::from_str(depth.as_str()).unwrap()`, `Method::from_bytes(b"PROPFIND").unwrap()` — infallible today (static values, semaphore never closed), but they violate the crate's own no-panic discipline and become reachable if `Depth`/methods gain dynamic values. Flagged by the 2026-08-20 audit (Task 1) and never fixed (→ AUDIT-009).

### Recommended fix
Replace with `.map_err(Error::other)?` or `ok_or` — mechanical, zero behavior change.

---

## AUDIT-023 — `build_uri` performs no `..` normalization (client-side path escape)

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Security (defense in depth)
- **Location:** `src/webdav/client.rs:262-314`

### Problem
Relative paths are joined to the base without resolving `.`/`..` segments; `get("../../other-user/cal")` produces a URI outside the base collection. The server's ACLs remain the real boundary; the risk is client-assisted access to sibling resources when callers feed semi-trusted hrefs (compounding AUDIT-004).

### Recommended fix
Normalize dot-segments after join (RFC 3986 §5.2.4) or reject `..` in relative paths. Tiny function, add with AUDIT-004's origin check.

---

## AUDIT-024 — Native root certificates silently dropped when `add()` fails

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Reliability
- **Location:** `src/webdav/builder.rs:470-491`

### Problem
`let _ = roots.add(cert);` discards individual load failures for native roots and user-supplied PEM roots; native-cert branch also discards `result.errors` in release builds. If enough roots fail, the trust store is silently incomplete → cryptic handshake errors far from the cause.

### Recommended fix
Count failures; if `roots.is_empty()` → return a descriptive `Error::Tls` naming the source; surface counts via log/tracing (AUDIT-010).

---

## AUDIT-025 — Non-ASCII ETag silently yields `None` from `etag_from_headers`

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Correctness
- **Location:** `src/webdav/client.rs:888-894`

### Problem
`.and_then(|v| v.to_str().ok())` — an ETag header containing non-ASCII bytes (non-conformant, but seen in the wild) makes the whole lookup return `None` silently; callers then skip conditional operations or re-fetch.

### Recommended fix
Return a typed error, or fall back to `latin1`-tolerant parsing with a warning; at minimum document the silent `None`.

---

## AUDIT-026 — Unknown `Content-Encoding` silently treated as identity / chain dropped

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Reliability
- **Location:** `src/common/compression.rs:50-64`

### Problem
Undecodable header → empty encoding list (identity); unknown token → `return None` removing it from the chain. `Content-Encoding: deflate` (server ignoring the q-value negotiation) → body returned still-compressed, surfacing later as a confusing XML parse error instead of a clear "unsupported encoding" message.

### Recommended fix
Unknown/undecodable encodings on a **response** should produce a dedicated error (or at least a logged warning), not silent identity passthrough.

---

## AUDIT-027 — Discovery/capability probes uncached; every workflow pays 2–3 extra RTTs

- **Severity:** Low-Medium
- **Confidence:** Confirmed
- **Domain:** Performance
- **Location:** `src/webdav/client.rs:1060-1077` (principal), `1023-1055` (sync support), no caching found (grep: only compression state is cached)

### Problem
`discover_current_user_principal` and `supports_webdav_sync` re-probe on every call. A sync loop calling `supports_webdav_sync` per iteration doubles its request count; discovery chains cost 2–3 extra requests per bootstrap. The compression cache (`Arc<RwLock>`, client.rs:140-144) proves the pattern exists in-codebase.

### Recommended fix
Document that callers should hold one client and cache discovery results; optionally add opt-in `discover_*_cached()` helpers. **Do not** auto-cache server topology by default (stale-URL hazard after server-side moves) — see "Do not fix".

---

## AUDIT-028 — Dead exported API: `impl_multistatus_on_end!` macro, pub parser internals, unused pub helpers

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Maintainability
- **Location:** `src/webdav/streaming.rs:257-270` (macro, zero call sites, `#[macro_export]`), `ElementName`/`element_from_bytes` pub in both streaming modules, `decompress_stream` pub with no production caller

### Problem
Publicly exported macro duplicates `MultistatusParser::on_end` logic and silently diverges from it (pops without the bookkeeping `finish()` relies on). Internals leaked `pub` constrain future parser refactors (semver). `decompress_stream` exists only for tests.

### Recommended fix
Delete the macro (dead + misleading); un-export `ElementName`/`element_from_bytes`/`decompress_stream` (or keep `decompress_stream` `pub` if intended for embedders — decide and document). Zero-risk deletions at 0.10.

---

## AUDIT-029 — Stale/incorrect documentation

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Developer experience
- **Location:** `README.md:424` ("version = \"0.7\"" in legacy example; crate at 0.9.0), `examples/migration.rs` (anyhow→thiserror tutorial, nothing to do with module migration), `src/error.rs:138-144` (`Error::Timeout` doc overstates coverage — see AUDIT-002), `tests/e2e/caldav/README.md:142` ("✅ HTTP/2" — unproven, AUDIT-017), `sabredav-test/README.md:44` (zstd module claim — false, AUDIT-018)

### Recommended fix
One documentation sweep; add doc-claim verification to the release checklist (repo rule: stale docs are bugs).

---

## AUDIT-030 — `.env` not gitignored

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Security hygiene
- **Location:** `.gitignore` (11 lines, no `.env` pattern)
- **Status:** ✅ Fixed 2026-09-01 (phase 0).

### Problem
CI defines `CALDAV_PASSWORD` conventions; a local `.env` (or `.envrc.local`) would be commit-ready. No secret is currently committed (checked `.envrc`, `flake.nix`, `sabredav-test/**`).

### Recommended fix
One line: `.env` / `.env.*` / `!.env.example` to `.gitignore`.

---

## AUDIT-031 — LGPL-3.0 license: commercial adoption friction (informational)

- **Severity:** Low
- **Confidence:** Confirmed (fact) / business impact unassessed from repo
- **Domain:** Supply chain / Adoption
- **Location:** `Cargo.toml:7`, `LICENSE`

### Problem
Rust crates are statically linked in practice; LGPL compliance (relinkability) is awkward for commercial embedders, and many corporate dependency policies blacklist LGPL. For a crate whose value proposition is "fast client for your services", this measurably narrows adoption.

### Recommended fix
**Do not fix unilaterally** — relicensing requires all contributors' consent and is a product decision. Flag it to the maintainer; MIT/Apache-2.0 dual-license is the ecosystem norm for client libs.

---

## AUDIT-032 — Internal working notes committed to the public repo (informational)

- **Severity:** Low
- **Confidence:** Confirmed
- **Domain:** Hygiene
- **Location:** `docs/superpowers/` (155 KB+ of plans/specs)

### Problem/Note
Development working notes (fix plans, specs) are committed to a public repository. No secrets found (checked). This is unusual for a published crate but serves as valuable decision history — this audit itself builds on it (AUDIT-009).

### Recommended fix
**Keep** (do not delete): curate into `docs/` over time and keep the AUDIT-ID trail — it is exactly the mechanism that makes future fix/reaudit cycles cheap.

---

## Summary statistics

| Severity | Count | IDs |
|---|---|---|
| High | 8 | AUDIT-001…006, 009, 017 |
| Medium | 11 | AUDIT-007, 008, 010…016, 018, 019 |
| Low / informational | 13 | AUDIT-020…032 |
| **Total** | **32** | |

Confidence: 30 Confirmed · 1 Confirmed/Needs-verification mix (AUDIT-015 part 3) · 1 Confirmed/business-unassessed (AUDIT-031).
