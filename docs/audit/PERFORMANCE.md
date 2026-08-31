# Performance Review — fast-dav-rs 0.9.0

**Audit date:** 2026-08-31 · Companion to `FINDINGS.md`. No micro-optimizations here; only issues with measurable end-to-end impact.

## 1. Memory model (the headline)

| Path | Peak memory |
|---|---|
| High-level methods (`list_calendars`, `calendar_query`, `sync_collection`, …) | wire body (decompressed, full) + parsed items `Vec` + all `calendar_data`/`address_data` strings — **everything at once** (AUDIT-003, AUDIT-016) |
| `parse_multistatus_stream` (default "streaming") | still buffers all items + data strings; only the *network read* is incremental (`caldav/streaming.rs:444`) |
| `parse_multistatus_stream_visit` | item-at-a-time — the only true streaming path; no decompressed-byte cap either |
| Batch helpers (`propfind_many`/`report_many`) | `Vec<BatchItem>` of fully-aggregated responses; memory ∝ paths × response size (`webdav/client.rs:919-952`) |

**10× data:** a sync with `include_data = true` over a 10× collection multiplies peak RSS ~10×, plus `Vec`/`String` growth doubling. No cap exists anywhere (AUDIT-003), so "10× data" and "hostile server" converge on the same OOM.

## 2. Latency cliffs

1. **Compression probe head-of-line blocking** (AUDIT-012): first wave of body-carrying requests in `Auto` mode serializes behind one hidden PROPFIND (up to **5 s hardcoded**, `client.rs:426,490-498`). Worse under concurrency: all callers wait on one probe.
2. **No body-read timeout** (AUDIT-002): a stalled server body hangs the request forever — the worst latency is infinite, and the client-level `default_timeout` does not cover it.
3. **Per-instance probe cost** (`client.rs:353-441`): serverless patterns building a client per request pay the probe every time. Documented nowhere.

## 3. Request amplification

- **Discovery/support probes uncached** (AUDIT-027): `discover_current_user_principal` + `discover_*_home_set` + `list_*` = 3 sequential RTTs per bootstrap, repeated by any caller that re-discovers; `supports_webdav_sync` uncached doubles request count in sync loops. The compression negotiation cache (`Arc<RwLock>`, shared by clones) proves the pattern exists — it was simply not applied here.
- **One silent retry** on 415/501/400 with body (AUDIT-007): bounded, but invisible.

## 4. What is genuinely fast (do not "fix")

- Streaming XML parser: incremental 8 KB buffer, iterative (no recursion → no stack risk), events decoded whole (`caldav/streaming.rs:344-376`). Buffer capacity retained for parser lifetime — peak ≈ largest single element, acceptable.
- Cheap `Clone` clients sharing one pooled hyper-util connection (pool 32/host, h2 adaptive window, `builder.rs:544-558`) — correct design.
- Request compression (gzip/br/zstd) with one-time negotiation cache — good design, rescue the failure-stickiness (AUDIT-012) rather than the mechanism.
- Macro-generated builders, zero-copy `Bytes` bodies.

## 5. Recommended benchmarks

The repo has **zero benchmarks**. Before/after any Phase 2 work (`REMEDIATION_PLAN.md`), add three `criterion`-style scenarios (or simple `Instant` harnesses in `benches/`):

1. `sync_collection` over 1k/10k synthetic items, `include_data` on/off — assert memory ceiling after AUDIT-003/016 fixes.
2. First-request latency in `Auto` mode, 32 concurrent callers — assert probe HOL elimination.
3. Aggregated vs `_visit` parse throughput on a 50 MB multistatus — document the real delta to justify the "use `_visit` for large syncs" guidance.
