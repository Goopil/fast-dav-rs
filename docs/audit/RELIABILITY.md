# Reliability Review — fast-dav-rs 0.9.0

**Audit date:** 2026-08-31 · Companion to `FINDINGS.md`.

## 1. Failure-mode matrix

| # | Dependency/condition fails | Behavior today | Finding |
|---|---|---|---|
| 1 | Server sends headers, then stalls | **Hangs forever** — timeout covers headers only | AUDIT-002 |
| 2 | Server sends gzip bomb | Unbounded decompress → OOM | AUDIT-003 |
| 3 | Server 400/415/501 after side effect | One silent re-send of the mutation | AUDIT-007 |
| 4 | Probe request fails transiently | Request compression permanently disabled, silently | AUDIT-012 |
| 5 | Auth/network error during capability probe | Reported as "sync unsupported" (`Ok(false)`) | AUDIT-013 |
| 6 | RFC-strict server | `sync_collection` → guaranteed 400 (`Depth: 1`) | AUDIT-001 |
| 7 | Server issues weak etags | `put_if_match` → permanent 412 | AUDIT-008 |
| 8 | Server truncates sync (507) | Surfaces as ordinary item; caller believes sync complete | AUDIT-015 |
| 9 | Malformed `<D:error>` body | Precondition silently lost | AUDIT-015 |
| 10 | Native roots partially fail to load | Silent incomplete trust store → cryptic handshake error | AUDIT-024 |
| 11 | Non-ASCII ETag header | Silent `None` → caller skips conditional safety | AUDIT-025 |
| 12 | Unknown `Content-Encoding: deflate` | Compressed body surfaces as confusing XML error | AUDIT-026 |
| 13 | Comment interleaved in `<D:href>` | First text chunk silently dropped | AUDIT-021 |
| 14 | Process dies mid-request | No state: client is stateless except compression cache — **safe by design** | — |
| 15 | Same operation executed twice concurrently | Safe: no shared mutation except Copy-valued compression state (lock-consistent, poisoning recovered) | — |

## 2. Idempotency and "what if it runs twice?"

The only automatic re-send is the compression retry (AUDIT-007), bounded to one attempt, on 400/415/501, for body-carrying requests. `PUT` with `If-Match` is conditional (safe); unconditional `PUT` is content-idempotent; residual risk is `PROPPATCH`/`MOVE`/custom REPORT bodies. `hyper-util`'s internal pool behavior on dead connections (possible silent re-send of replayable requests) lives outside this repo — classified **Needs verification** in `FINDINGS.md` (e).

## 3. "Process dies at each step" analysis

The client holds no durable state (no topology cache, no queue, no disk) — crash-safety is inherited from statelessness, which is the right call for a library. The one cached mutable value (negotiated request compression) self-heals via setters and degrades to identity, i.e., to *slower*, not to *wrong* — except its stickiness after a transient failure (AUDIT-012), which degrades to slower *permanently*.

## 4. Timeouts — coverage matrix

| Phase | Covered? |
|---|---|
| TCP connect | Optional (`connect_timeout`, default none) |
| TLS handshake | Bounded only by header-phase timeout |
| Request → response headers | Yes (`default_timeout` 20 s, per-request override) |
| **Response body read/decompress** | **No** (AUDIT-002) |
| Streaming XML idle | 30 s fixed (parsers), raw `send_stream` none |
| Total request (headers+body) | **No** |
| Pool idle | Unbounded default (2026-08-20 plan Task 11 unexecuted) |

## 5. Data-integrity specifics

- Conditional-write safety net (If-Match) is solid *provided* callers get strong etags; weak-etag acceptance makes the net silently absent on strict servers (AUDIT-008) and non-ASCII etag headers silently skip it (AUDIT-025).
- Sync results can silently omit items (AUDIT-015.3 heuristic) or hide truncation (AUDIT-015.2). These are the findings most likely to corrupt a user's sync state *without any error*.
- No partial-write risk inside the library: single-request operations only; no batching of mutations.

## 6. Resilience posture (deliberate "do not fix" notes)

- **No retries/backoff/circuit breaker:** correct for a client library — retry policy belongs to the embedding application. Document it; do not add it (see `REMEDIATION_PLAN.md` → Do not fix).
- **No redirects followed:** safe (no cross-host credential forwarding by the library); document that 3xx surfaces to the caller.
- **Rate limiting / backpressure:** delegated to the caller via `max_concurrency` on batch helpers (bounded `Semaphore`, `FuturesOrdered`) — adequate.
