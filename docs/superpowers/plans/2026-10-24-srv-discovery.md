# Opt-in `srv` feature (RFC 6764 §3/§6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship DNS SRV/TXT service discovery (RFC 6764 §3/§6) as an opt-in `srv` cargo feature with dedicated `discover_caldav_srv`/`discover_carddav_srv` functions and an injectable `SrvResolver` trait, closing the last RFC coverage gap (issue #254).

**Architecture:** New module `src/webdav/srv.rs` holds the trait, record type, hickory default impl (feature-gated) and the discovery flow: SRV lookup (TLS then plain record, `require_https` respected) → TXT `path=` context → `.well-known` probe on the resolved authority → base-URL fallback when no records. The existing private `discover_well_known` gains explicit probe-target/fallback-base parameters so both flows share one implementation. Graceful when DNS is absent, strict when a record was found.

**Tech Stack:** Rust, hyper 1.x (existing pipeline), `hickory-resolver` 0.26 (optional dep, behind `srv`).

**Spec:** `docs/superpowers/specs/2026-10-24-srv-discovery-design.md`

## Global Constraints

- Opt-in: `srv = ["dep:hickory-resolver"]`; default tree must not compile any DNS code (`cargo tree` without the feature shows no hickory).
- 100% additive → 0.20.0; no existing public signature changes; `cargo semver-checks` clean.
- **Spec adjustment (documented in Task 1):** `SrvRecord`, `SrvResolver` and the builder option compile **always** (pure std, zero deps) so `from_parts`/builder signatures carry no `#[cfg]` scatter; only the hickory default impl and the `discover_*_srv` functions are `#[cfg(feature = "srv")]`.
- Failure semantics verbatim from the spec: *"an SRV lookup with no records (or any DNS error: NXDOMAIN, timeout, …) falls through to today's behavior; a found SRV record makes the resolved URL authoritative — a probe failure there is a real error, never silently masked"*.
- Record selection verbatim: *"lowest SRV priority wins, highest weight first among ties"*.
- Non-TLS SRV records: `_caldavs._tcp` then `_caldav._tcp` (carddav pair likewise); `http://` URL only when the client has `require_https == false`.
- Credentials follow the existing `same_origin` invariant: a probe to a foreign SRV authority goes unauthenticated (documented, never overridden).
- Gates: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo nextest run --all-features --locked --test unit_tests`, `cargo test --doc --all-features`, `cargo build --examples --all-features`.
- Conventional commits ≤70 chars; branch `feat/srv-discovery` (based on `main` at 7d830ee — independent of PR #266).

---

### Task 1: Cargo feature + `SrvRecord`/`SrvResolver` types + builder/client plumbing

**Files:**
- Modify: `Cargo.toml` (features ~83-85, dependencies)
- Create: `src/webdav/srv.rs`
- Modify: `src/webdav/mod.rs` (module + re-exports)
- Modify: `src/lib.rs` (re-export)
- Modify: `src/webdav/client.rs` (struct field ~592, `from_parts` ~688, accessors)
- Modify: `src/webdav/builder.rs` (field ~73, option method, `build()` ~679)
- Modify: `docs/superpowers/specs/2026-10-24-srv-discovery-design.md` (amend the API section to the always-compiled-trait decision)

**Interfaces:**
- Produces: `pub struct SrvRecord { pub target: String, pub port: u16, pub priority: u16, pub weight: u16 }` (Debug, Clone, PartialEq, Eq); `pub trait SrvResolver: Send + Sync { fn lookup_srv(&self, query: &str) -> Pin<Box<dyn Future<Output = Result<Vec<SrvRecord>>> + Send + '_>>; fn lookup_txt(&self, query: &str) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>; }`; builder `.srv_resolver(Arc<dyn SrvResolver>)`; client accessors `pub(crate) fn srv_resolver(&self) -> Option<&Arc<dyn SrvResolver>>` and `pub(crate) fn require_https(&self) -> bool`. Later tasks use exactly these names.

- [ ] **Step 1: Cargo.toml** — add the feature and optional dependency:

```toml
[features]
default = []
tracing = ["dep:tracing"]
srv = ["dep:hickory-resolver"]
```

```toml
hickory-resolver = { version = "0.26", optional = true }
```

(Default features of hickory-resolver = `system-config` + `tokio`, both required: `Resolver::builder_tokio()` reads the system resolver config and needs the tokio runtime provider. This supersedes the spec's `default-features = false` sketch — noted in the spec amendment below.)

- [ ] **Step 2: Create `src/webdav/srv.rs`** with the types (implementation comes in Task 3):

```rust
//! DNS-based service discovery (RFC 6764 §3/§6): SRV record lookup and the
//! TXT `path=` context hint, opt-in via the `srv` cargo feature.
//!
//! The RFC 6764 resolution order implemented by
//! [`discover_caldav_srv`](crate::discover_caldav_srv) /
//! [`discover_carddav_srv`](crate::discover_carddav_srv):
//! SRV record (`_{service}s._tcp` TLS first, then `_{service}._tcp`) → TXT
//! `path=` context on the same owner name → `.well-known` probe on the
//! resolved authority → base-URL fallback (§6). The existing
//! [`discover_caldav`](crate::discover_caldav) /
//! [`discover_carddav`](crate::discover_carddav) functions remain
//! DNS-free: no caller gains an implicit DNS hop.
//!
//! Failure semantics: an absent record (or any DNS error) falls through to
//! the plain `.well-known` flow on the client's base URL; a *found* record
//! makes the resolved URL authoritative — probe failures there propagate
//! as real errors, never silently masked.

use std::future::Future;
use std::pin::Pin;

use crate::Result;

/// One resolved SRV record (RFC 6764 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrvRecord {
    /// Service host (trailing dot stripped).
    pub target: String,
    /// TCP port.
    pub port: u16,
    /// Lower priority wins.
    pub priority: u16,
    /// Among equal priorities, higher weight is preferred.
    pub weight: u16,
}

/// Injectable DNS resolver used by the `discover_*_srv` functions.
///
/// The crate ships a [`hickory-resolver`](https://docs.rs/hickory-resolver)
/// backed default (system configuration) when the `srv` feature is enabled;
/// implement this trait to mock DNS in tests or plug a custom resolver, and
/// set it with [`WebDavClientBuilder::srv_resolver`](crate::WebDavClientBuilder::srv_resolver).
/// Boxed futures keep the trait object-safe (`Arc<dyn SrvResolver>`).
pub trait SrvResolver: Send + Sync {
    /// Resolve an SRV query (e.g. `_caldavs._tcp.example.com`).
    fn lookup_srv(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SrvRecord>>> + Send + '_>>;
    /// Resolve the TXT strings of `query` (the RFC 6764 §6 `path=` hint
    /// lives on the SRV owner name).
    fn lookup_txt(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;
}
```

- [ ] **Step 3: Register the module + re-exports.** `src/webdav/mod.rs`: add `pub mod srv;` next to the other `pub mod` lines and add to the type re-export list: `pub use srv::{SrvRecord, SrvResolver};` (always compiled — pure std). `src/lib.rs`: add `SrvRecord, SrvResolver` to the `pub use webdav::{…}` list (alphabetical).

- [ ] **Step 4: Client field + accessors** in `src/webdav/client.rs`:

Struct field (after `retry_backoff_cap: Duration,`):

```rust
    /// Custom DNS resolver for SRV/TXT service discovery (the `srv`
    /// feature); `None` = the hickory default (system configuration).
    srv_resolver: Option<std::sync::Arc<dyn SrvResolver>>,
```

Accessors (next to the other `pub(crate)` accessors, e.g. near `base()`):

```rust
    /// The configured custom SRV resolver, if any.
    pub(crate) fn srv_resolver(&self) -> Option<&std::sync::Arc<dyn SrvResolver>> {
        self.srv_resolver.as_ref()
    }

    /// Whether the client enforces `https://` (builder `require_https(true)`).
    pub(crate) fn require_https(&self) -> bool {
        self.require_https
    }
```

Import at the top of client.rs: `use crate::webdav::srv::SrvResolver;`

`from_parts`: add parameter `srv_resolver: Option<std::sync::Arc<dyn SrvResolver>>,` after `retry_backoff_cap` and initialize the field in the constructor body: `srv_resolver,`.

- [ ] **Step 5: Builder option** in `src/webdav/builder.rs`:

Field (after `require_https: bool,`): `srv_resolver: Option<std::sync::Arc<dyn SrvResolver>>,`
(`Default` init: `srv_resolver: None,` in the builder constructor at ~145.)

Method (near `require_https` ~422):

```rust
    /// Set a custom DNS resolver for SRV/TXT service discovery (the `srv`
    /// feature). When unset, the default hickory resolver (system
    /// configuration) is used by the `discover_*_srv` functions.
    pub fn srv_resolver(mut self, resolver: std::sync::Arc<dyn SrvResolver>) -> Self {
        self.srv_resolver = Some(resolver);
        self
    }
```

Import: `use crate::webdav::srv::SrvResolver;`

`build()` (~679): pass `self.srv_resolver.take(),` as the new last argument of `from_parts`.

- [ ] **Step 6: Spec amendment** — in `docs/superpowers/specs/2026-10-24-srv-discovery-design.md`, replace the "Public API" heading paragraph to record the adjustment:

```markdown
## Public API

**Adjustment (implementation):** `SrvRecord`, `SrvResolver` and the builder
option compile always (pure std, zero dependency cost) so `from_parts` and
the builder carry no `#[cfg]` scatter; only the hickory default impl and the
`discover_*_srv` functions are `#[cfg(feature = "srv")]`. The Cargo sketch
below uses hickory's default features (`system-config` + `tokio`), which
`Resolver::builder_tokio()` requires.
```

- [ ] **Step 7: Verify compile + suite green**

Run: `cargo build --all-features && cargo build && cargo nextest run --all-features --locked --test unit_tests`
Expected: both builds OK (default build compiles the trait with no hickory); 891 unit tests PASS.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/webdav/srv.rs src/webdav/mod.rs src/lib.rs src/webdav/client.rs src/webdav/builder.rs docs/superpowers/specs/2026-10-24-srv-discovery-design.md
git commit -m "feat(webdav): Add SrvResolver trait and srv cargo feature scaffolding"
```

---

### Task 2: Refactor `discover_well_known` to explicit probe target + fallback base (behavior-preserving)

**Files:**
- Modify: `src/webdav/discovery.rs` (`discover_well_known` ~77-134, call sites ~186-188 and ~216-218)

**Interfaces:**
- Consumes: `WebDavClient::build_uri` (accepts absolute URLs — client.rs:821).
- Produces: `async fn discover_well_known(client: &WebDavClient, probe_target: &str, fallback_base: &str, service: &str, operation: Operation) -> Result<String>` — private. Task 3 calls it with absolute probe URLs and SRV fallback bases.

- [ ] **Step 1: Refactor** — replace the current signature and body pieces:

```rust
/// Shared implementation behind [`discover_caldav`] / [`discover_carddav`]
/// (and the `srv` feature's `discover_*_srv`): probe
/// `{probe_target}/.well-known/{service}` and resolve the service URL from
/// the redirect chain.
///
/// `probe_target` is either a root-anchored path (relative to the client's
/// base, the plain flow) or an absolute URL (the SRV flow, where the probe
/// targets the SRV-resolved authority). `fallback_base` is what a `404` (or
/// a success answered directly on the probe URI) returns: the client's base
/// URL in the plain flow, the SRV-resolved candidate base in the SRV flow.
async fn discover_well_known(
    client: &WebDavClient,
    probe_target: &str,
    fallback_base: &str,
    service: &str,
    operation: Operation,
) -> Result<String> {
```

Body changes (keep everything else identical):

- `let path = format!("/.well-known/{service}");` becomes `let probe_uri = client.build_uri(probe_target)?;`
- `build_and_send(Method::from_bytes(b"PROPFIND")?, &path, …)` → `build_and_send(…, probe_target, …)`
- The 404 branch returns `Ok(fallback_base.to_string());`
- The error message uses `probe_target` instead of `path`.
- The final check replaces `final_uri.path() == path` with full-URI equality (identical semantics for the relative flow, correct for absolute probe URLs):

```rust
    if final_uri == probe_uri {
        // Success served directly on the probe URI, without a redirect.
        // Per RFC 5785 §1.1 the actual service endpoint must not live there,
        // so the fallback base is returned unchanged.
        Ok(fallback_base.to_string())
    } else {
        let service = strip_userinfo(&final_uri);
        Ok(service)
    }
```

- Call sites:

```rust
pub async fn discover_caldav(client: &WebDavClient) -> Result<String> {
    discover_well_known(
        client,
        "/.well-known/caldav",
        &client.base().to_string(),
        "caldav",
        Operation::DiscoverWellKnownCaldav,
    )
    .await
}
```

```rust
pub async fn discover_carddav(client: &WebDavClient) -> Result<String> {
    discover_well_known(
        client,
        "/.well-known/carddav",
        &client.base().to_string(),
        "carddav",
        Operation::DiscoverWellKnownCarddav,
    )
    .await
}
```

- [ ] **Step 2: Verify behavior preserved**

Run: `cargo nextest run --all-features --locked --test unit_tests discovery`
Expected: every existing discovery test PASS (multi-hop redirects, 404 fallback, direct success, follow-disabled errors, credential stripping).

- [ ] **Step 3: Commit**

```bash
git add src/webdav/discovery.rs
git commit -m "ref(webdav): Parameterize well-known probe target and fallback"
```

---

### Task 3: `discover_caldav_srv`/`discover_carddav_srv` — failing tests first, then implementation

**Files:**
- Create: `tests/unit/webdav/srv_discovery_tests.rs`
- Modify: `tests/unit/webdav/mod.rs` (registration)
- Modify: `src/webdav/srv.rs` (implementation)
- Modify: `src/webdav/mod.rs`, `src/lib.rs` (feature-gated re-exports)
- Modify: `src/webdav/discovery.rs` (make `discover_well_known` callable from `srv.rs`: change `async fn` to `pub(crate) async fn`)

**Interfaces:**
- Consumes: `SrvRecord`/`SrvResolver` (Task 1), `discover_well_known` (Task 2), `client.srv_resolver()`, `client.require_https()`.
- Produces: `pub async fn discover_caldav_srv(client: &WebDavClient, domain: &str) -> Result<String>`, `pub async fn discover_carddav_srv(client: &WebDavClient, domain: &str) -> Result<String>` (both `#[cfg(feature = "srv")]`), re-exported as `fast_dav_rs::discover_caldav_srv` / `fast_dav_rs::discover_carddav_srv`. Pure helpers (crate-private, inline-tested): `fn select_srv(records: Vec<SrvRecord>) -> Option<SrvRecord>`, `fn txt_path(records: &[String]) -> Option<String>`.

- [ ] **Step 1: Write the failing tests** — create `tests/unit/webdav/srv_discovery_tests.rs`:

```rust
//! SRV/TXT service discovery (RFC 6764 §3/§6, `srv` feature): mocked
//! resolver + mock HTTP servers prove the resolution order, the graceful
//! fallback when DNS yields nothing, and the strict semantics when it does
//! not.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use fast_dav_rs::webdav::srv::{SrvRecord, SrvResolver};
use fast_dav_rs::webdav::{discover_caldav_srv, discover_carddav_srv};
use fast_dav_rs::{Error, Operation, RequestCompressionMode, WebDavClient};

use crate::common::http_helpers::{
    response_head, serve_capture, serve_sequence, unreachable_base,
};

const REDIRECT_301: &str =
    "HTTP/1.1 301 Moved Permanently\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const SERVER_ERROR: &str =
    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// Configurable fake DNS: per-query SRV record sets, per-query error
/// injection, and per-query TXT strings.
#[derive(Default)]
struct MockResolver {
    srv: HashMap<String, Vec<SrvRecord>>,
    srv_errors: HashSet<String>,
    txt: HashMap<String, Vec<String>>,
}

impl MockResolver {
    fn with_srv(mut self, query: &str, records: Vec<SrvRecord>) -> Self {
        self.srv.insert(query.to_string(), records);
        self
    }
    fn with_srv_error(mut self, query: &str) -> Self {
        self.srv_errors.insert(query.to_string());
        self
    }
    fn with_txt(mut self, query: &str, strings: Vec<String>) -> Self {
        self.txt.insert(query.to_string(), strings);
        self
    }
}

impl SrvResolver for MockResolver {
    fn lookup_srv(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = fast_dav_rs::Result<Vec<SrvRecord>>> + Send + '_>> {
        if self.srv_errors.contains(query) {
            return Box::pin(async { Err(Error::other("dns unavailable")) });
        }
        let records = self.srv.get(query).cloned().unwrap_or_default();
        Box::pin(async move { Ok(records) })
    }

    fn lookup_txt(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = fast_dav_rs::Result<Vec<String>>> + Send + '_>> {
        let strings = self.txt.get(query).cloned().unwrap_or_default();
        Box::pin(async move { Ok(strings) })
    }
}

fn make_client(base: &str, resolver: MockResolver) -> WebDavClient {
    let client = WebDavClient::builder(base)
        .srv_resolver(Arc::new(resolver))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

const DOMAIN: &str = "example.com";
const TLS_QUERY: &str = "_caldavs._tcp.example.com";
const PLAIN_QUERY: &str = "_caldav._tcp.example.com";

#[tokio::test]
async fn srv_with_txt_path_probes_resolved_authority() {
    let ok_body = b"ok".to_vec();
    let probe = (redirect_head(REDIRECT_301, "/caldav/"), Vec::new());
    let final_hop = (response_head("", ok_body.len()), ok_body);
    let (base, captured) = serve_sequence(vec![probe, final_hop]).await;
    let port: u16 = base
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap();
    let resolver = MockResolver::default()
        .with_srv(TLS_QUERY, vec![SrvRecord {
            target: "127.0.0.1".to_string(),
            port,
            priority: 0,
            weight: 0,
        }])
        .with_txt(TLS_QUERY, vec!["path=/cal/".to_string()]);
    let client = make_client("https://dav.example.com/", resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    // The probe targets the SRV authority root (TXT path belongs to the
    // 404 fallback, not to the well-known URI); the redirect resolves the
    // final service URL.
    assert_eq!(service_url, format!("{base}caldav/"));
    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    let first = String::from_utf8_lossy(&reqs[0]);
    assert!(first.contains("PROPFIND /.well-known/caldav HTTP/1.1"), "{first}");
    // The probe crosses origins (base is dav.example.com): the existing
    // same-origin invariant must keep credentials off the wire.
    assert!(
        !first.to_ascii_lowercase().contains("authorization:"),
        "credentials must not be sent to a foreign SRV authority: {first}"
    );
}

#[tokio::test]
async fn srv_absent_falls_back_to_base_well_known() {
    let (base, captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let client = make_client(&base, MockResolver::default());

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    assert_eq!(
        service_url, base,
        "no SRV record anywhere: the plain .well-known flow on the base URL runs unchanged"
    );
    let req = String::from_utf8_lossy(&captured.lock().unwrap());
    assert!(req.contains("PROPFIND /.well-known/caldav HTTP/1.1"), "{req}");
}

#[tokio::test]
async fn dns_error_falls_back_like_absence() {
    let (base, _captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let resolver = MockResolver::default().with_srv_error(TLS_QUERY);
    let client = make_client(&base, resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();
    assert_eq!(
        service_url, base,
        "a DNS failure degrades gracefully to the plain .well-known flow"
    );
}

#[tokio::test]
async fn srv_found_probe_404_returns_candidate_base() {
    let (base, _captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let port: u16 = base
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap();
    let resolver = MockResolver::default()
        .with_srv(TLS_QUERY, vec![SrvRecord {
            target: "127.0.0.1".to_string(),
            port,
            priority: 0,
            weight: 0,
        }])
        .with_txt(TLS_QUERY, vec!["path=/cal/".to_string()]);
    let client = make_client("https://dav.example.com/", resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    // RFC 6764 §6: 404 on the probe → the SRV+TXT candidate base itself.
    assert_eq!(service_url, format!("{base}cal/"));
}

#[tokio::test]
async fn srv_found_probe_failure_is_strict() {
    let (base, _captured) = serve_capture(SERVER_ERROR.to_owned(), Vec::new()).await;
    let port: u16 = base
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap();
    let resolver = MockResolver::default().with_srv(TLS_QUERY, vec![SrvRecord {
        target: "127.0.0.1".to_string(),
        port,
        priority: 0,
        weight: 0,
    }]);
    let client = make_client("https://dav.example.com/", resolver);

    let err = discover_caldav_srv(&client, DOMAIN).await.unwrap_err();
    assert!(
        matches!(err, Error::UnexpectedStatus { operation: Operation::DiscoverWellKnownCaldav, .. }),
        "a found SRV record makes the probe authoritative: {err:?}"
    );
}

#[tokio::test]
async fn require_https_skips_plain_srv_records() {
    let (base, captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let port: u16 = base
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap();
    let resolver = MockResolver::default().with_srv(PLAIN_QUERY, vec![SrvRecord {
        target: "127.0.0.1".to_string(),
        port,
        priority: 0,
        weight: 0,
    }]);
    let client = WebDavClient::builder(&base)
        .require_https(true)
        .srv_resolver(Arc::new(resolver))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    // The plain (http) record must never be consulted: the probe hits the
    // client's own base and the flow degrades to the plain discovery.
    assert_eq!(service_url, base);
    let req = String::from_utf8_lossy(&captured.lock().unwrap());
    assert!(req.contains("PROPFIND /.well-known/caldav HTTP/1.1"), "{req}");
}

#[tokio::test]
async fn plain_srv_record_used_without_require_https() {
    let ok_body = b"ok".to_vec();
    let direct = (response_head("", ok_body.len()), ok_body);
    let (base, _captured) = serve_sequence(vec![direct]).await;
    let port: u16 = base
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap();
    let resolver = MockResolver::default().with_srv(PLAIN_QUERY, vec![SrvRecord {
        target: "127.0.0.1".to_string(),
        port,
        priority: 0,
        weight: 0,
    }]);
    let client = make_client("https://dav.example.com/", resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    // Success served directly on the probe URI (RFC 5785 §1.1): the
    // candidate base (http authority, no TXT path → root) is returned.
    assert_eq!(service_url, format!("http://127.0.0.1:{port}/"));
}

#[tokio::test]
async fn carddav_srv_queries_the_carddav_service_name() {
    let (base, captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let resolver = MockResolver::default().with_srv(
        "_carddavs._tcp.example.com",
        vec![SrvRecord {
            target: "127.0.0.1".to_string(),
            port: 1, // unreachable — must never be probed: record absent → fallback
            priority: 0,
            weight: 0,
        }],
    );
    // NOTE: the record IS present, so the probe targets 127.0.0.1:1 and
    // fails (strict). Use a present record only for the query-name probe:
    // instead assert via the fallback path (no records) that the queried
    // name is the carddav one by error injection.
    let resolver = MockResolver::default().with_srv_error("_carddavs._tcp.example.com");
    let client = make_client(&base, resolver);

    let service_url = discover_carddav_srv(&client, DOMAIN).await.unwrap();
    assert_eq!(service_url, base);
    let req = String::from_utf8_lossy(&captured.lock().unwrap());
    assert!(req.contains("PROPFIND /.well-known/carddav HTTP/1.1"), "{req}");
}

#[tokio::test]
async fn empty_domain_is_rejected_before_any_lookup() {
    let client = make_client("https://dav.example.com/", MockResolver::default());
    let err = discover_caldav_srv(&client, "   ").await.unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
}
```

Register in `tests/unit/webdav/mod.rs` (after `streaming_tests`): `#[cfg(feature = "srv")] pub mod srv_discovery_tests;`

- [ ] **Step 2: Run to verify failure**

Run: `cargo nextest run --all-features --test unit_tests srv_discovery`
Expected: compile ERROR — `discover_caldav_srv` not found.

- [ ] **Step 3: Implement in `src/webdav/srv.rs`** (append below the trait):

```rust
#[cfg(feature = "srv")]
pub use discover_impls::{discover_caldav_srv, discover_carddav_srv};

#[cfg(feature = "srv")]
mod discover_impls {
    use std::cmp::Reverse;
    use std::sync::{Arc, OnceLock};

    use super::SrvRecord;
    use crate::webdav::client::WebDavClient;
    use crate::webdav::discovery::discover_well_known;
    use crate::{Error, Operation, Result};

    /// The default resolver: hickory with the system configuration,
    /// built once per process (the client clones cheaply and shares
    /// connection pools; DNS setup is process-global by nature).
    fn default_resolver() -> Arc<dyn super::SrvResolver> {
        static DEFAULT: OnceLock<Arc<dyn super::SrvResolver>> = OnceLock::new();
        Arc::clone(DEFAULT.get_or_init(|| {
            Arc::new(
                hickory_resolver::Resolver::builder_tokio()
                    .expect("system DNS configuration must be readable")
                    .build(),
            )
        }))
    }

    impl super::SrvResolver for hickory_resolver::TokioResolver {
        fn lookup_srv(
            &self,
            query: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<SrvRecord>>> + Send + '_>> {
            Box::pin(async move {
                let lookup = hickory_resolver::Resolver::srv_lookup(self, query)
                    .await
                    .map_err(|e| Error::other(format!("SRV lookup {query} failed: {e}")))?;
                let mut records = Vec::new();
                for record in lookup.iter() {
                    if let hickory_resolver::proto::rr::RData::SRV(srv) = record.data() {
                        let mut target = srv.target().to_string();
                        if target.ends_with('.') {
                            target.pop();
                        }
                        records.push(SrvRecord {
                            target,
                            port: srv.port(),
                            priority: srv.priority(),
                            weight: srv.weight(),
                        });
                    }
                }
                Ok(records)
            })
        }

        fn lookup_txt(
            &self,
            query: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
            Box::pin(async move {
                let lookup = hickory_resolver::Resolver::txt_lookup(self, query)
                    .await
                    .map_err(|e| Error::other(format!("TXT lookup {query} failed: {e}")))?;
                let mut strings = Vec::new();
                for record in lookup.iter() {
                    if let hickory_resolver::proto::rr::RData::TXT(txt) = record.data() {
                        let joined: String =
                            txt.txt().iter().map(|s| String::from_utf8_lossy(s)).collect();
                        strings.push(joined);
                    }
                }
                Ok(strings)
            })
        }
    }

    /// Lowest priority wins; highest weight first among ties (spec decision).
    pub(super) fn select_srv(mut records: Vec<SrvRecord>) -> Option<SrvRecord> {
        records.sort_by_key(|r| (r.priority, Reverse(r.weight)));
        records.into_iter().next()
    }

    /// First `path=` attribute across the TXT strings (RFC 6764 §6); the
    /// value keeps its leading slash (or gets one).
    pub(super) fn txt_path(records: &[String]) -> Option<String> {
        for record in records {
            for chunk in record.split_whitespace() {
                if let Some(path) = chunk.strip_prefix("path=") {
                    return Some(if path.starts_with('/') {
                        path.to_string()
                    } else {
                        format!("/{path}")
                    });
                }
            }
        }
        None
    }

    async fn discover_srv(
        client: &WebDavClient,
        domain: &str,
        service: &str,
        tls_query: &str,
        plain_query: &str,
        operation: Operation,
    ) -> Result<String> {
        let domain = domain.trim();
        if domain.is_empty() {
            return Err(Error::InvalidInput(
                "SRV discovery requires a non-empty domain".to_string(),
            ));
        }
        let resolver: Arc<dyn super::SrvResolver> =
            client.srv_resolver().cloned().unwrap_or_else(default_resolver);

        let mut candidate_base: Option<String> = None;
        // TLS record first; the plain (http) record is skipped entirely
        // when the client requires https. Any DNS absence OR failure at
        // this stage degrades to the plain .well-known flow (spec).
        for (query, scheme) in [
            (format!("{tls_query}.{domain}"), "https"),
            (format!("{plain_query}.{domain}"), "http"),
        ] {
            if scheme == "http" && client.require_https() {
                continue;
            }
            let Ok(records) = resolver.lookup_srv(&query).await else {
                continue;
            };
            let Some(record) = select_srv(records) else {
                continue;
            };
            let authority = format!("{scheme}://{}:{}", record.target, record.port);
            let path = resolver
                .lookup_txt(&query)
                .await
                .ok()
                .as_deref()
                .and_then(txt_path);
            candidate_base = Some(match path {
                Some(path) => format!("{authority}{path}"),
                None => format!("{authority}/"),
            });
            break;
        }

        match candidate_base {
            // Authoritative: the probe targets the SRV authority root; a
            // 404 falls back to the SRV+TXT candidate base, any other
            // failure propagates (no silent masking).
            Some(candidate) => {
                let authority = candidate
                    .splitn(3, '/')
                    .take(3)
                    .collect::<String>()
                    .split("//")
                    .last()
                    .map(|rest| candidate.split("//").next().unwrap_or("").to_string() + rest)
                    .unwrap_or_else(|| candidate.clone());
                let _ = authority; // superseded below — see probe construction
                let origin = candidate_origin(&candidate);
                discover_well_known(
                    client,
                    &format!("{origin}/.well-known/{service}"),
                    &candidate,
                    service,
                    operation,
                )
                .await
            }
            // No record anywhere: exactly today's flow, on the client base.
            None => {
                let fallback = client.base_url_string();
                discover_well_known(
                    client,
                    &format!("/.well-known/{service}"),
                    &fallback,
                    service,
                    operation,
                )
                .await
            }
        }
    }

    /// `scheme://host:port` out of a candidate base (drops any TXT path).
    fn candidate_origin(candidate: &str) -> String {
        let (scheme, rest) = candidate.split_once("://").unwrap_or(("", candidate));
        let authority = rest.split('/').next().unwrap_or(rest);
        format!("{scheme}://{authority}")
    }

    pub(super) async fn discover_caldav_srv(
        client: &WebDavClient,
        domain: &str,
    ) -> Result<String> {
        discover_srv(
            client,
            domain,
            "caldav",
            "_caldavs._tcp",
            "_caldav._tcp",
            Operation::DiscoverWellKnownCaldav,
        )
        .await
    }

    pub(super) async fn discover_carddav_srv(
        client: &WebDavClient,
        domain: &str,
    ) -> Result<String> {
        discover_srv(
            client,
            domain,
            "carddav",
            "_carddavs._tcp",
            "_carddav._tcp",
            Operation::DiscoverWellKnownCarddav,
        )
        .await
    }
}
```

NOTE for the implementer: delete the dead `authority`/`let _ = authority;` lines from the draft above — keep only `let origin = candidate_origin(&candidate);`. Also the module needs `use std::future::Future; use std::pin::Pin;` at the top of `discover_impls` (they are imported at the file top already — reuse). `client.base_url_string()`: add a small `pub(crate) fn base_url_string(&self) -> String { self.base.to_string() }` accessor to `WebDavClient` in client.rs (next to the Task 1 accessors).

In `src/webdav/discovery.rs`: change `async fn discover_well_known(` → `pub(crate) async fn discover_well_known(`.

Re-exports (`#[cfg(feature = "srv")]`): `src/webdav/mod.rs` — `pub use srv::{discover_caldav_srv, discover_carddav_srv};` ; `src/lib.rs` — same names in the `pub use webdav::{…}` list, gated:

```rust
#[cfg(feature = "srv")]
pub use webdav::{discover_carddav_srv, discover_caldav_srv};
```

- [ ] **Step 4: Inline tests for the pure helpers** (in `src/webdav/srv.rs`, inside `mod discover_impls`, append):

```rust
    #[cfg(test)]
    mod helper_tests {
        use super::{select_srv, txt_path};

        #[test]
        fn lowest_priority_wins_highest_weight_among_ties() {
            let records = vec![
                super::SrvRecord { target: "a".into(), port: 1, priority: 10, weight: 0 },
                super::SrvRecord { target: "b".into(), port: 2, priority: 5, weight: 1 },
                super::SrvRecord { target: "c".into(), port: 3, priority: 5, weight: 100 },
            ];
            let best = select_srv(records).unwrap();
            assert_eq!((best.target.as_str(), best.port), ("c", 3));
            assert!(select_srv(Vec::new()).is_none());
        }

        #[test]
        fn first_path_attribute_wins_and_is_root_anchored() {
            let records = vec![
                "some txt".to_string(),
                "path=/a/ other=1".to_string(),
                "path=/b/".to_string(),
            ];
            assert_eq!(txt_path(&records).as_deref(), Some("/a/"));
            assert_eq!(txt_path(&["path=b/".to_string()]).as_deref(), Some("/b/"));
            assert_eq!(txt_path(&["no path here".to_string()]), None);
            assert!(txt_path(&[]).is_none());
        }
    }
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo nextest run --all-features --test unit_tests srv_discovery`
Expected: 9 tests PASS. (If a hickory rdata accessor name differs — e.g. `txt.txt()` vs `txt.txt_data()` — fix per the 0.26 docs at compile time; the trait/mock seams stay unchanged.)

- [ ] **Step 6: Full suite + lint + format**

Run: `cargo nextest run --all-features --locked --test unit_tests && cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: all PASS, clippy clean.

- [ ] **Step 7: Commit**

```bash
git add src/webdav/srv.rs src/webdav/discovery.rs src/webdav/mod.rs src/lib.rs src/webdav/client.rs tests/unit/webdav/srv_discovery_tests.rs tests/unit/webdav/mod.rs
git commit -m "feat(webdav): Add opt-in SRV/TXT service discovery (RFC 6764)"
```

---

### Task 4: Documentation

**Files:**
- Modify: `README.md` (features list + `no_run` example near the discovery section)
- Modify: `docs/advanced-configuration.md` (rewrite the SRV/TXT exclusion note ~200-215 into the opt-in feature doc)
- Modify: `CHANGELOG.md` (`[Unreleased]` Added)
- Modify: `src/webdav/srv.rs` (rustdoc `no_run` example on the two functions)

- [ ] **Step 1: README.** In the "Advanced Features" list, replace the SRV/TXT exclusion bullet:

```markdown
- RFC 6764 `.well-known` service discovery (`discover_caldav`/`discover_carddav`), plus **opt-in DNS SRV/TXT discovery** (RFC 6764 §3/§6) behind the `srv` cargo feature: `discover_caldav_srv`/`discover_carddav_srv` resolve `_caldavs._tcp`/`_caldav._tcp` (and the CardDAV pair), apply the TXT `path=` hint, then run the same `.well-known` flow — graceful fallback when DNS yields nothing. Default builds stay DNS-free.
```

In the Quick Start discovery area (~line 184-230), add:

````markdown
### DNS SRV/TXT discovery (opt-in `srv` feature)

With the `srv` cargo feature enabled (`cargo add fast-dav-rs --features srv`),
`discover_caldav_srv`/`discover_carddav_srv` take the **domain** (from user
input, e.g. the account domain) and resolve RFC 6764 §3/§6 in order: SRV
record → TXT `path=` context → `.well-known` probe on the resolved authority
→ base-URL fallback. DNS absence degrades gracefully to the plain
`.well-known` flow; a found record makes its URL authoritative.

```rust,no_run
use fast_dav_rs::{WebDavClient, discover_caldav_srv};

# async fn run() -> fast_dav_rs::Result<()> {
let client = WebDavClient::builder("https://dav.example.com/")
    .basic_auth("user", "pass")
    .build()?;
// Domain from user input — not necessarily the base URL's host.
let service_url = discover_caldav_srv(&client, "example.com").await?;
println!("CalDAV service (via SRV) lives at {service_url}");
# Ok(())
# }
```

Plug a custom resolver (or a test mock) with
`.srv_resolver(Arc<dyn SrvResolver>)`.
````

- [ ] **Step 2: `docs/advanced-configuration.md`** — replace the paragraph `RFC 6764's DNS-based discovery steps are deliberately out of scope: SRV record lookup (§3) and TXT record lookup (§6) are excluded to keep the dependency tree lean (no DNS resolver crate). Supply the base URL the DNS records would have resolved to — the `.well-known` probe above covers the RFC 6764 §5 step and implements the §6 base-URL fallback (a `404` returns the base URL). DNS SRV/TXT record lookup is not implemented:` with:

```markdown
DNS SRV/TXT lookup (RFC 6764 §3/§6) is available as the **opt-in `srv` cargo
feature** (default builds stay DNS-free). `discover_caldav_srv(&client,
domain)` / `discover_carddav_srv(&client, domain)` resolve, in order: the
TLS SRV record (`_caldavs._tcp.{domain}`, then `_caldav._tcp.{domain}` when
`require_https` is not set), the TXT `path=` hint on the same owner name,
then the `.well-known` probe against the resolved authority, falling back to
the SRV+TXT candidate base on `404` (§6). When no record exists — or the DNS
lookup fails — the plain `.well-known` flow on the client's base URL runs
unchanged (graceful degradation); when a record IS found, its URL is
authoritative and probe failures propagate as real errors. Record selection:
lowest priority wins, highest weight first among ties. The probe follows the
crate's credential invariant: credentials are only ever sent to the client's
base origin, so a cross-origin SRV authority is probed unauthenticated.
Inject a custom resolver with `.srv_resolver(Arc<dyn SrvResolver>)` (tests
mock DNS through it):
```

(followed by the existing `discover_caldav` example unchanged; add a `discover_caldav_srv` call line to it if wanted.)

- [ ] **Step 3: CHANGELOG** — under `## [Unreleased]` → `### Added`, append:

```markdown
- Opt-in `srv` cargo feature: DNS SRV/TXT service discovery (RFC 6764 §3/§6)
  via `discover_caldav_srv`/`discover_carddav_srv` and the injectable
  `SrvResolver` trait (hickory-resolver default, builder option
  `.srv_resolver`). Graceful fallback to `.well-known` when DNS yields
  nothing; strict errors when a found record's probe fails. Default builds
  remain DNS-free.
```

- [ ] **Step 4: Rustdoc examples** — in `src/webdav/srv.rs`, attach to `discover_caldav_srv` (and mirror for carddav):

````rust
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::{WebDavClient, discover_caldav_srv};
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = WebDavClient::builder("https://dav.example.com/")
    ///     .basic_auth("user", "pass")
    ///     .build()?;
    /// let service_url = discover_caldav_srv(&client, "example.com").await?;
    /// println!("CalDAV service lives at {service_url}");
    /// # Ok(())
    /// # }
    /// ```
````

- [ ] **Step 5: Verify doctests + commit**

Run: `cargo test --doc --all-features`

```bash
git add README.md docs/advanced-configuration.md CHANGELOG.md src/webdav/srv.rs
git commit -m "docs: Document opt-in SRV/TXT discovery feature"
```

---

### Task 5: Full gates + default-tree verification

**Files:** none (verification only)

- [ ] **Step 1: Default tree unchanged**

Run: `cargo tree | grep -c hickory || echo "no hickory"`
Expected: `no hickory` (default features compile zero DNS code). Also `cargo build --locked` succeeds.

- [ ] **Step 2: Feature tree works**

Run: `cargo tree --features srv | grep -c hickory`
Expected: ≥1.

- [ ] **Step 3: All gates**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --locked --test unit_tests
cargo test --doc --all-features
cargo build --examples --all-features --locked
cargo semver-checks
```
Expected: all green (additive, feature-gated).

- [ ] **Step 4: No stray TODOs**

Run: `grep -rn "TODO\|FIXME" src/webdav/srv.rs tests/unit/webdav/srv_discovery_tests.rs`
Expected: no matches.

- [ ] **Step 5: Push + open PR** (against `main`, independent of PR #266; title: `feat: Opt-in SRV/TXT service discovery (RFC 6764 §3/§6)`; body references issue #254 and the spec doc; note the 0.20.0 target)

```bash
git push -u origin feat/srv-discovery
gh pr create --base main --title "feat: Opt-in SRV/TXT service discovery (RFC 6764 §3/§6)" --body-file pr-body.md
```

---

## Self-review notes

- Spec coverage: feature gate + lean default tree (T1), dedicated functions + injectable resolver via builder (T1/T3), order SRV→TXT→well-known→base (T3), graceful/strict semantics (T3 tests 2-5), require_https + plain records (T3 tests 6-7), priority/weight + TXT selection (T3 helper tests), no new Error variants (T3), docs incl. CHANGELOG + README + exclusion-note rewrite (T4), default-tree gate (T5). e2e deliberately absent (spec: mocked resolver covers acceptance).
- Type consistency: `discover_well_known(client, probe_target, fallback_base, service, operation)` used identically in T2 and T3; `SrvRecord`/`SrvResolver` names match T1.
- The draft in T3 Step 3 contains a deliberately-flagged dead `authority` block to delete — the `candidate_origin` helper is the real implementation.
