# Audit Phase 0 — quick wins design (2026-09-01)

Scope: REMEDIATION_PLAN.md Phase 0 (issue #109), items AUDIT-001, -002, -005, -022, -030 + AUDIT-009 closure. One PR (`audit/phase-0`), nothing else touched.

## Fixes

1. **AUDIT-001** — `sync_collection` sends `Depth::One`; RFC 6578 §3.3 mandates `Depth: 0`. Change `self.report(path, Depth::One, &body)` → `Depth::Zero` in `caldav/client.rs` and `carddav/client.rs`. Body already carries `<D:sync-level>`.
2. **AUDIT-002** — in `WebDavClient::send`, the body read (`decompress_body`) runs outside any timeout. Wrap it in `timeout(limit, …)` with the same limit as the headers phase (per-phase semantics, as recommended by the audit). Fix `Error::Timeout` doc (`error.rs`) which overstates coverage.
3. **AUDIT-005** — `publish.yml`: publish condition becomes tag-only (`startsWith(github.ref, 'refs/tags/')`), plus a version↔tag match check. (Repository-level protected environment is a GitHub settings change — maintainer action, out of repo.)
4. **AUDIT-030** — add `.env` to `.gitignore`.
5. **AUDIT-022** — Superseded by a coverage-gate ruling: the four `unwrap`/`expect` sites are statically infallible (static method literals, enum-controlled depth string, private never-closed semaphore), so the graceful-`BatchItem` guards would have been dead code and are not applied; all four sites keep `expect`/`unwrap` with `ponytail:` invariant markers. Typed-error conversion requires breaking `Result` signatures — re-evaluate in 0.10.

## Tests

- `sync_collection` sends `Depth: 0` — request captured by one-shot TCP server (`serve_once` pattern from #111), caldav + carddav.
- Stalled body → `Error::Timeout`: server sends headers then keeps the connection open; `send()` with a short per-request timeout must return `Error::Timeout` instead of hanging.

## AUDIT-009 closure (process debt)

Unexecuted tasks of `docs/superpowers/plans/2026-08-20-audit-fixes.md`:

| Task | Decision |
|---|---|
| Dedup spec | Executed by #111 |
| Task 2 (connect_timeout default) | Closed, no action: `timeout(limit)` already wraps `client.request()` including connect (bounded by default 20 s) |
| Task 4 (body size guard) | Re-pointed to #79 (Phase 1) |
| Task 6 (probe 5 s timeout) | Re-pointed to AUDIT-012 (Phase 1, same fix) |
| Task 11 (pool_idle_timeout default) | Closed, no action: hyper-util legacy client defaults to 90 s |

Side note for the re-audit: AUDIT-006/014/028 are largely resolved by #111 (dedup executed, types unified in `webdav/`) and #112 (dead macro removed).

## Non-goals

Everything else (Phase 1+), protected GitHub environment settings, `tracing` (AUDIT-010), size caps (AUDIT-003 → #79).
