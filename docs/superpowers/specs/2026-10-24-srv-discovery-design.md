# Opt-in `srv` feature — DNS SRV/TXT service discovery (RFC 6764 §3/§6) — 0.20.0 design

## Goal

Close the last RFC coverage gap (issue #254, audit 2026-10-23): ship DNS-based
service discovery — SRV record lookup (RFC 6764 §3) and TXT `path=` context
(§6) — as an **opt-in cargo feature**, keeping the default dependency tree
lean (no DNS resolver compiled unless requested).

## Decisions (validated with maintainer, 2026-10-24)

- **Dedicated discovery functions**, not implicit SRV inside the existing
  `discover_caldav`/`discover_carddav`: existing callers keep zero DNS hops
  even with the feature enabled, and the SRV domain is explicit — RFC 6764
  §3 takes the domain from user input (e.g. an account domain), which is not
  always the base URL's host.
- **Resolver injected via a builder option** (`.srv_resolver(Arc<dyn
  SrvResolver>)`), default = hickory-resolver (system config, built once).
  The trait exists for test mocks and custom resolvers.
- **Failure semantics — graceful when absent, strict when present:** an SRV
  lookup with no records (or any DNS error: NXDOMAIN, timeout, …) falls
  through to today's behavior (`.well-known` probe on the client's base
  URL); a *found* SRV record makes the resolved URL authoritative — a probe
  failure there is a real error, never silently masked by a fallback.
- **Non-TLS SRV records honored:** `_caldavs._tcp` then `_caldav._tcp`
  (and the carddav pair); an `http://` URL is only constructed when the
  client did not set `require_https(true)`. Record selection: lowest SRV
  priority wins, highest weight first among ties (deterministic, documented).

## Cargo

```toml
[features]
srv = ["dep:hickory-resolver"]

[dependencies]
hickory-resolver = { version = "…", optional = true, default-features = false, features = ["tokio"] }
```

Default tree unchanged (`cargo tree` shows no hickory without the feature).

## Public API

**Adjustment (implementation):** `SrvRecord`, `SrvResolver` and the builder
option compile always (pure std, zero dependency cost) so `from_parts` and
the builder carry no `#[cfg]` scatter; only the hickory default impl and the
`discover_*_srv` functions are `#[cfg(feature = "srv")]`. The Cargo sketch
below uses hickory's default features (`system-config` + `tokio`), which
`Resolver::builder_tokio()` requires.

**MSRV note:** hickory is pinned to the `0.25` line (rust-version 1.71.1) —
the whole `0.26` line requires rustc 1.88, which would break the crate's
1.85 MSRV. The typed lookups (`SrvLookup`/`TxtLookup` iterators) and
`builder_tokio()` API are identical for this crate's usage.

```rust
/// Injectable DNS resolver for SRV/TXT service discovery.
pub trait SrvResolver: Send + Sync {
    fn lookup_srv(&self, query: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<SrvRecord>>> + Send + '_>>;
    fn lookup_txt(&self, query: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;
}

pub struct SrvRecord {
    pub target: String,   // hostname, trailing dot stripped
    pub port: u16,
    pub priority: u16,
    pub weight: u16,
}

/// RFC 6764 §3/§6 discovery for CalDAV: SRV → TXT `path=` → `.well-known`
/// probe on the resolved authority → base-URL fallback (existing §6 logic).
pub async fn discover_caldav_srv(client: &WebDavClient, domain: &str) -> Result<String>;
/// Same for CardDAV.
pub async fn discover_carddav_srv(client: &WebDavClient, domain: &str) -> Result<String>;
```

Boxed futures: `Arc<dyn SrvResolver>` requires dyn-compatible traits
(native `async fn` in traits is not dyn-compatible).

## Flow of `discover_caldav_srv(client, domain)`

1. Validate `domain` non-empty (trimmed) → `Error::InvalidInput` otherwise.
2. Lookup `_caldavs._tcp.{domain}`; empty/error → try `_caldav._tcp.{domain}`
   (skipped entirely when the client has `require_https(true)`).
3. Still nothing → **fallback**: return the result of the existing
   `.well-known` probe against the client's base URL (identical to
   `discover_caldav` today).
4. Record found → authority `https://{target}:{port}/` (or `http://` for the
   plain record). TXT lookup on the SRV owner name: first `path=…` value
   found across TXT strings becomes the context path; no TXT/`path=` → root.
   The candidate base is `{scheme}://{target}:{port}{path}`.
5. Probe `{candidate-base}/.well-known/{service}` with the existing pipeline
   (redirects, auth, userinfo stripping). **The candidate base is
   authoritative**: any probe failure propagates as-is. A `404` on the probe
   returns the candidate base unchanged (existing RFC 6764 §6 fallback).

Internal plumbing: private `discover_well_known` gains an explicit
`base: &Uri` parameter (today it reads `client.base()`); `strip_userinfo`
applies to the SRV-resolved URL too (defense in depth). New module
`src/webdav/srv.rs`; re-exports under the feature in `src/webdav/mod.rs`
and `src/lib.rs`. Builder: `srv_resolver(Option<Arc<dyn SrvResolver>>)`
stored on the client (`from_parts` +1 crate-private param). No new
`Error` variants.

## Tests (unit, `--features srv`, mocked resolver + mock HTTP server)

- SRV + TXT `path=` → the well-known probe hits
  `{https://target:port}{path}/.well-known/caldav`, redirect answer
  resolves the final URL.
- SRV absent → probe hits the original base (graceful fallback).
- DNS lookup error (mock returns Err) → same graceful fallback.
- SRV found, probe 404 → candidate base returned unchanged.
- SRV found, probe non-success (e.g. 500) → `Error::UnexpectedStatus`
  (strict, no masking).
- `require_https(true)` → `_caldav._tcp` never consulted/constructed.
- Priority/weight selection: lowest priority wins, highest weight among ties.
- TXT without `path=`, multiple TXT strings → root context; first `path=`
  wins.
- `discover_caldav` (no `_srv`) behavior unchanged with the feature enabled.
- Default-tree gate: `cargo tree` (default features) contains no hickory.

No e2e: real DNS cannot be fixtures-reproducible; the mocked-resolver unit
tier covers the acceptance criteria from the issue.

## Docs

- README: feature table entry (`srv`) + `no_run` example.
- `docs/advanced-configuration.md`: the current SRV/TXT exclusion note
  becomes the opt-in feature documentation (order, failure semantics,
  custom resolver).
- `src/webdav/discovery.rs` + `srv.rs` module docs; rustdoc on the new API
  with `no_run` examples.
- CHANGELOG under `[Unreleased]` (Added, behind the `srv` feature).

## Compatibility

100% additive, feature-gated: no existing signature changes, default
feature set byte-identical. Targets 0.20.0 (second half, after the raw
property capture PR #266). `cargo semver-checks` clean; `cargo build`
(default), clippy/test `--all-features` gates unchanged.
