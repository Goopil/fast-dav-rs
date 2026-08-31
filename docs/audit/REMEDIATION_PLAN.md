# Remediation Plan — fast-dav-rs 0.9.0 audit (2026-08-31)

Prioritization: impact × risk × urgency ÷ effort. Each item references `FINDINGS.md` IDs. Work in a 0.10 minor window where marked breaking.

## Phase 0 — Immediate (this week; all small diffs)

- [ ] Fix AUDIT-001 — `sync_collection` → `Depth::Zero` in `caldav/client.rs:584` and `carddav/client.rs:558` (2 lines + tests). *Highest correctness ROI in the whole report.*
- [ ] Fix AUDIT-002 — wrap body read/decompress in the per-request timeout (or add total-request deadline); correct `Error::Timeout` docs.
- [ ] Fix AUDIT-005 — publish.yml: tag-only trigger, version↔tag match, protected `environment:`.
- [ ] Fix AUDIT-030 — add `.env` patterns to `.gitignore` (1 line).
- [ ] Fix AUDIT-022 — replace `expect`/`unwrap` in `webdav/client.rs:928-991` with typed errors (mechanical; also closes 2026-08-20 plan Task 1).
- [ ] Close AUDIT-009 — execute or explicitly close every unexecuted task of `docs/superpowers/plans/2026-08-20-audit-fixes.md` (Tasks 2, 4, 6, 11 + dedup spec) and adopt the "plan = executed or closed" rule.

## Phase 1 — Stabilization (2–3 weeks)

- [ ] Fix AUDIT-003 — `max_response_body_size` guard (builder-configurable, generous default) in `decompress_body`/`decompress_stream` (2026-08-20 plan Task 4).
- [ ] Fix AUDIT-007 — restrict compression retry to idempotent methods / conditional PUTs.
- [ ] Fix AUDIT-008 — reject weak etags for `If-Match` with `EtagReason::Weak` (breaking-ish → 0.10).
- [ ] Fix AUDIT-012 — probe: derive timeout from `default_timeout`; transient failure → retryable "unknown" state instead of pinned `Identity`; document HOL behavior.
- [ ] Fix AUDIT-013 — parse `supported-report-set` with the multistatus parser; distinguish error from unsupported.
- [ ] Fix AUDIT-015 — first-class `SyncResponse.truncated` (507); stop swallowing malformed `<D:error>`; document the collection heuristic; add unit tests.
- [ ] Fix AUDIT-011 — release-build loudness for `danger_accept_invalid_certs` and http://+credentials (log/tracing once AUDIT-010 lands).
- [ ] Fix AUDIT-018 — e2e reads `CALDAV_*` env vars; `set -euo pipefail` in reset-db.sh; `docker compose` v2; authenticated readiness check.
- [ ] Testing seeds for Phase 1: first wire-level integration test + chunked/compressed fakes + probe test (TESTING.md §6.1–6.3).

## Phase 2 — Performance (measurable, after Phase 1)

- [ ] Fix AUDIT-016 — document memory model; make `_visit` the documented path for large syncs; optional `max_items` guard.
- [ ] Fix AUDIT-027 — document client-holding/caching guidance; add opt-in cached discovery helpers.
- [ ] Fix AUDIT-010 — `tracing` feature (prereq for several Phase 1 loudness items; do it here at latest).
- [ ] Add the three benchmarks (PERFORMANCE.md §5) and record before/after numbers.

## Phase 3 — Architecture (0.10 window, planned)

- [ ] Fix AUDIT-006 — execute the dedup spec (`docs/superpowers/specs/2026-08-20-dedup-macros-design.md`); port MKCOL fallback to `mkcalendar` and validation to CardDAV raw path **first** (independent, small).
- [ ] Fix AUDIT-014 — unify `TextMatch`/`Collation`/`MatchType`/`ParamFilter` in webdav; resolve root re-export collisions with deprecation aliases (breaking → 0.10).
- [ ] Fix AUDIT-020 — namespace-aware element matching (with the parser unification from AUDIT-006).
- [ ] Fix AUDIT-019 — job timeouts, fork-PR coverage handling, cargo-deny, MSRV test run, action SHA-pinning.
- [ ] Fix AUDIT-017 — remaining test-fidelity items (h2, concurrency, poisoning, timeout via `tokio::time::pause`).

## Phase 4 — Long term (non-urgent)

- [ ] AUDIT-021 — append-text + UTF-8 strictness in parsers.
- [ ] AUDIT-023 — dot-segment normalization in `build_uri` (can ride along with AUDIT-004's origin check).
- [ ] AUDIT-024 — root-store load failure reporting.
- [ ] AUDIT-025/026 — non-ASCII etag + unknown `Content-Encoding` handling.
- [ ] AUDIT-028 — delete dead macro, un-export parser internals.
- [ ] AUDIT-029 — documentation sweep + doc-claim verification in the release checklist.
- [ ] AUDIT-004 — cross-origin credential guard (see note below — could be Phase 1 if security posture demands).

## Quick wins

AUDIT-001 (2 lines), AUDIT-030 (1 line), AUDIT-022 (mechanical), AUDIT-005 (YAML if), AUDIT-029 (doc sweep), `set -euo pipefail` (AUDIT-018).

## Structural fixes

AUDIT-006 dedup execution, AUDIT-002 timeout rework, AUDIT-003 size caps, AUDIT-014 type unification, AUDIT-010 tracing seam.

## Long-term debt

AUDIT-020 namespace parsing, AUDIT-016/027 memory/caching ergonomics, AUDIT-031 license decision (maintainer call).

## Do not fix (deliberate)

| Item | Rationale |
|---|---|
| Auto-retry/backoff/circuit-breaker policy | Belongs to the embedding app; adding it would surprise callers. Document instead. (RELIABILITY §6) |
| Auto-caching discovery results | Stale-URL hazard after server-side moves; caller-owned caching is safer. Opt-in helpers only (ADR-2). |
| Replacing hyper-util legacy client / tower-service / quick-xml push parser | Boring, proven, correct choices. No concrete problem. |
| `Arc<RwLock<Copy>>` compression state | Simple, adequate, lock-ordered, poisoning-safe. |
| Sonar coverage carve-out mechanics for e2e | The carve-out is intentional and documented (AGENTS.md); fix the *behavior* gaps with tests, not the gate. |
| AUDIT-031 relicense unilaterally | Requires all contributors' consent + product decision; flag to maintainer only. |
| AUDIT-032 removing `docs/superpowers/` | The plan/spec trail is what made this audit cheap; keep and curate. |
| Splitting `webdav/client.rs` into 10 files immediately | ARCH-2 is a smell, not a wound; extract probe/discovery modules when touched (boy-scout), no big-bang split. |
