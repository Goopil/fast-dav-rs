# Executive Summary — fast-dav-rs 0.9.0 Technical Audit

**Date:** 2026-08-31 · **Commit audited:** `f68d692` (0.9.0 + 4 untagged commits) · **Full detail:** `FINDINGS.md` (32 findings, IDs `AUDIT-001`…`AUDIT-032`)

## Overall score

| Domain | /10 | | Domain | /10 |
|---|---|---|---|---|
| Correctness | 6 | Data integrity | 6 |
| Architecture | 6 | Testing | 5 |
| Performance | 6 | Maintainability | 6 |
| Scalability | 6 | Observability | **3** |
| Stability | 5 | Operations | 6 |
| Resilience | 5 | Developer experience | 7 |
| Security | 6 | **Global** | **5.6 / 10** |

**Risk level: Moderate-Elevated.** No critical exploitable vulnerability; the crate is well-built in its core disciplines (XML escaping, secrets hygiene, error taxonomy, streaming parser). The risk concentrates in **failure modes under stress** (hang, OOM), **one credential-egress path**, and **divergent duplication** that is already shipping behavioral bugs.

**Verified at audit time:** clippy clean · 379/379 unit tests pass · `cargo audit` 0 advisories.

## Top 10 problems

1. **AUDIT-002** — Response-body read has no timeout → requests hang **forever** despite `default_timeout` (headers-only coverage).
2. **AUDIT-003** — Unbounded buffering/decompression → decompression bomb or large collection = OOM. A fix was planned in 2026-08 and never executed.
3. **AUDIT-004** — Credentials sent to any absolute URL (server-controlled hrefs) — no origin check.
4. **AUDIT-001** — `sync_collection` sent with `Depth: 1`, RFC 6578 requires `Depth: 0` → guaranteed 400 on strict servers. Works today only because common servers are lenient.
5. **AUDIT-006** — caldav↔carddav duplication (78–92%) with shipped behavioral divergences (MKCOL fallback missing in CalDAV; raw-XML injection surface only in CardDAV).
6. **AUDIT-009** — The previous audit's fix plan (`docs/superpowers/plans/2026-08-20-audit-fixes.md`) was written and committed but **partially never executed** (5 tasks verified still open at HEAD).
7. **AUDIT-008 + AUDIT-025** — Weak etags accepted for `If-Match` (guaranteed 412 on strict servers) and non-ASCII etag headers silently dropped: the optimistic-concurrency safety net silently disappears.
8. **AUDIT-015** — Sync truncation (507) and item-dropping heuristics are silent → users can believe a sync completed when it did not.
9. **AUDIT-005** — `publish.yml` can publish any branch via `workflow_dispatch` (tag check short-circuited).
10. **AUDIT-010 + AUDIT-017** — Zero observability hooks, and the test suite never drives a request through the real client stack (no chunked, no h2, no auth-on-wire, no Auto-probe, no concurrency tests).

## Critical risks (would page you at 3 a.m.)

- Wedged workers from stalled bodies (AUDIT-002) with **no log to explain why** (AUDIT-010).
- OOM in embedding processes (AUDIT-003).
- Credential exfiltration via hostile-server hrefs (AUDIT-004).
- Silent sync-state corruption (AUDIT-015/008/025).

## State of the system

Strong foundations: clean layering, disciplined XML escaping (23/23 interpolation sites verified), real streaming parser, zeroize + redacting Debug, good error taxonomy, thorough CI for build/lint, honest internal working notes. Weak flank: the library is **silent** (no logging), its timeout promise is half-true, its two protocol clients drift apart, and its test depth stops at the parser boundary. The 2026-08-20 audit already identified several of these; the gap is execution, not discovery.

## Immediate recommendations

1. Execute Phase 0 of `REMEDIATION_PLAN.md` (six small diffs, ≤1 day total): Depth fix, body timeout, publish gate, `.gitignore`, de-panic, close the 2026-08-20 backlog.
2. Land the `tracing` feature before 1.0 — it converts four silent failure modes into observable ones.
3. Adopt the rule: a committed fix plan is either executed or explicitly closed.

## Verdict

**5.6/10 — a good library core carrying unmanaged operational risk.** The code that exists is largely right; the code that is missing (timeouts on bodies, size caps, observability, wire-level tests) is where the incidents will come from. Everything needed to fix it is already documented in this repository — including, now, this audit.

## Risk matrix (top findings)

| Finding | Probability | Impact | Severity | Confidence | Priority |
|---|---|---|---|---|---|
| AUDIT-002 hang on stalled body | High (any flaky server/proxy) | High (wedged workers) | **High** | Confirmed | P0 |
| AUDIT-003 OOM (bomb / big collections) | Medium | High (process death) | **High** | Confirmed | P0 |
| AUDIT-004 credential egress | Low-Medium (needs hostile server) | High (secret leak) | **High** | Confirmed | P1 |
| AUDIT-001 sync 400 on strict servers | Medium (grows as strictness grows) | High (core feature breaks) | **High** | Confirmed | P0 |
| AUDIT-006 duplication drift | High (already happening) | High (double bugs) | **High** | Confirmed | P2 |
| AUDIT-008/025 silent loss of conditional safety | Medium | High (data overwrite) | Medium | Confirmed | P1 |
| AUDIT-015 silent sync truncation | Medium | High (corrupted sync state) | Medium | Confirmed | P1 |
| AUDIT-005 publish gate hole | Low (needs maintainer action) | High (unreviewed release) | **High** | Confirmed | P0 |
| AUDIT-010 zero observability | Certain (absence) | Medium (blind incidents) | Medium | Confirmed | P1 |
| AUDIT-009 expired fix plans | Certain (verified) | Medium (repeat audit cost) | **High** (process) | Confirmed | P0 |

*Effort note: P0 items are all ≤1 day total (see `REMEDIATION_PLAN.md` Phase 0). "Easy to fix" ≠ "not serious" — AUDIT-001 is a two-line fix hiding a feature-killing interop bug.*

## Top 10 actions

1. Depth:Zero for sync-collection (AUDIT-001).
2. Timeout the body phase; fix `Error::Timeout` docs (AUDIT-002).
3. Restrict publish.yml to tags + version match + protected environment (AUDIT-005).
4. Close the 2026-08-20 backlog: body-size cap, connect_timeout default, probe timeout, pool idle timeout, de-panic (AUDIT-003/009/022).
5. `.gitignore` `.env` (AUDIT-030) — one line.
6. Cross-origin credential guard in `build_uri`/`send` (AUDIT-004).
7. `tracing` feature seam (AUDIT-010).
8. Restrict compression retry to idempotent methods; reject weak etags for `If-Match`; first-class sync truncation signal (AUDIT-007/008/015).
9. First wire-level integration test + chunked/compressed fakes + Auto-probe test (AUDIT-017).
10. Execute the dedup spec; unify root-re-exported types (AUDIT-006/014) in the 0.10 window.

## Three things I would fix tomorrow

1. **AUDIT-001** — two lines, kills a guaranteed interop failure of the library's headline feature.
2. **AUDIT-002 + AUDIT-003 together** — the body-phase timeout and the size cap are the same 30 lines in the same function; they remove the two most realistic production incidents (hang and OOM).
3. **AUDIT-005 + AUDIT-030** — the release gate and the `.gitignore`: five minutes of YAML and one line, both close real holes.

## What I would NOT change

- **hyper-util legacy client, tower-service, quick-xml push parser** — boring, proven choices; no concrete failure attributable to them.
- **No retry/backoff/circuit-breaker machinery** — retry policy belongs to the embedding application; adding it would surprise callers.
- **No library-owned discovery cache** — stale-URL hazard after server-side moves; document caller-owned caching, offer opt-in helpers only.
- **The `Arc<RwLock<Copy>>` compression state** — simple, lock-ordered, poisoning-safe; the fix needed is behavioral (probe stickiness), not structural.
- **The LGPL-3.0 license** — a real adoption consideration, but a maintainer/product decision requiring contributor consent; not the auditor's call.
- **`docs/superpowers/` working notes in the repo** — unusual for a published crate, but this decision trail is exactly what made this audit (and the previous one) cheap. Keep and curate.
- **A big-bang split of `webdav/client.rs`** — extract the probe/discovery modules opportunistically when touched; a 10-file reorganization solves no concrete incident.

---

*Read next: `FINDINGS.md` (evidence) → `REMEDIATION_PLAN.md` (actions) → domain deep-dives (`SECURITY.md`, `RELIABILITY.md`, `PERFORMANCE.md`, `ARCHITECTURE.md`, `TESTING.md`, `OPERATIONS.md`).*