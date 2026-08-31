# Testing Review — fast-dav-rs 0.9.0

**Audit date:** 2026-08-31 · Companion to `FINDINGS.md`.
**Dynamic results at audit time:** `cargo nextest run --test unit_tests --all-features --locked` → **379/379 PASS** (5.3 s) · `cargo clippy --all-targets --all-features -- -D warnings` → **clean** · `cargo audit` → **0 advisories**.

## 1. What the tests actually are

- ~90% offline: hand-written XML fed to parsers/builders (`tests/unit/caldav/caldav_helpers.rs:92-208`). For what they cover, these are **high quality**: XML-injection escaping, sync-token priority chains, 4040-vs-404 discrimination, ETag normalization (weak/bare/empty), truncated-XML rejection, multi-layer decompression round-trips, streaming idle-timeout.
- Two narrow HTTP fakes: `tokio::io::duplex` pipes feeding a raw HTTP/1.1 response into a **direct** `http1::handshake` (bypassing `WebDavClient` entirely, `streaming_tests.rs:44`); a raw `TcpListener` capturing request bytes for ETag tests (`etag_tests.rs:8-33`).
- Client-method tests stop at input validation before any request is sent (`client_tests.rs:301-390`).
- E2E: docker-compose nginx→php-fpm/SabreDAV→MySQL, HTTP/1.1, hardcoded `test`/`test`, many println-style soft asserts.

## 2. Coverage claim vs reality

SonarCloud's 80%-on-new-code gate runs on `tests/unit` only (`coverage.yml:53-61`); `tests/e2e/**` and `sabredav-test/**` are excluded from analysis (`sonar-project.properties:14`). Real gaps are not line-coverage gaps — they are **behavior-coverage gaps**:

## 3. Untested behaviors (the list that matters)

| Behavior | Status |
|---|---|
| Full request through the real client's pooled hyper-util stack | **Never exercised in unit tests** |
| `Authorization`/`User-Agent` actually attached on the wire | Never asserted (accessor at `client.rs:213-216` unused) |
| Auto-compression probe + negotiation cache via HTTP | Unreachable in tests (every test disables it first) |
| Client-level `default_timeout` firing (`Error::Timeout`) | Untested (only streaming idle timeout, wall-clock) |
| Chunked transfer-encoding | Zero occurrences in tests/ |
| HTTP/2 | Zero; e2e stack is HTTP/1.1 (README claim at `tests/e2e/caldav/README.md:142` unproven) |
| Redirects (3xx) | Never faked |
| Connection reuse / pool behavior | Every fake serves exactly one request |
| Compressed body → `parse_multistatus_stream` (combined) | Halves tested separately, combination never |
| Concurrency: parallel requests vs shared `Arc<RwLock>` state; probe mutex contention; poisoning recovery | Untested (`builder_tests.rs:77-90` is single-threaded) |
| `danger_accept_invalid_certs` / TLS cert loading | Construction-only checks (`builder_tests.rs:132-138`) |
| Per-item 4xx propstat inside a 207 multistatus | Not asserted end-to-end (AUDIT-015) |
| `tokio::time::pause/advance` anywhere | Absent |

## 4. Bugs I could introduce today that no test would catch

1. Break Auto-mode compression negotiation (cache never written / probe re-fires per request).
2. Race the compression caches (TOCTOU between cache read and probe).
3. Stop attaching `Authorization` (or attach it to the wrong header name).
4. Make client-level `default_timeout` a no-op.
5. Break chunked or h2 handling.
6. Corrupt connection reuse (e.g., drop bodies without draining everywhere).
7. Break the e2e env contract — nothing reads `CALDAV_*` env vars (AUDIT-018); rename them and CI stays green.
8. Silently regress e2e via its println-and-continue asserts (~20 sites).

## 5. Test-architecture gaps beyond behavior

- caldav/carddav test files are copy-paste twins (`etag_tests.rs` differs by 26 cosmetic substitutions) — same duplication tax as the source (AUDIT-006); a divergence fixed in one file's tests will not exist in the other.
- The CI `e2e-tests.yml` unit job is *weaker* than `ci.yml` (no `--all-features`, no `--locked`, no nextest) — the `legacy` feature never compiles in that job.
- MSRV (1.85) is check-only — no tests run on the declared minimum.

## 6. Recommended additions (priority order)

1. **One wire-level integration test** (tokio `TcpListener` or `duplex` through the real client): assert method, path, `Authorization`, `User-Agent`, `Depth`, `Content-Type` on the wire; assert connection reuse across two sequential requests.
2. **Response-shape fakes:** chunked encoding; gzip/brotli/zstd body into `parse_multistatus_stream`; 3xx passthrough; per-item propstat failures inside 207.
3. **Auto-probe test:** mock server rejecting gzip-encoded REPORT with 415 once, then accepting — assert retry + cache state; plus probe-failure → `Identity` stickiness test (AUDIT-012).
4. **Timeout test with `tokio::time::pause`** for the body phase once AUDIT-002 is fixed.
5. **Concurrency test:** 32 concurrent `put` through one cloned client against a stateful mock; assert no interleaving corruption and poison-recovery path.
6. **Hard-assert sweep of e2e** for sync/compression categories (AUDIT-018), and make tests read `CALDAV_*` env vars.
