# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security
- Bearer tokens and proxy credentials are zeroized in the intermediate
  `Authorization` header strings built by the client builder, so plaintext
  credentials no longer linger in freed heap memory (issue #79)
- Decompressed response bodies are capped at 256 MiB to prevent decompression
  bombs: `decompress_body` fails with the new `Error::BodyTooLarge` variant and
  `decompress_stream` errors once the cap is exceeded (issue #79, AUDIT-003)

### Changed
- **Breaking:** `supports_webdav_sync` (and its CalDAV/CardDAV delegates) now returns
  `Result<SyncCapability>` instead of `Result<bool>`, with `SyncCapability` being
  `Supported`, `Unsupported` or `Unknown`: a transport or timeout error is reported as
  `SyncCapability::Unknown` instead of being silently swallowed as "unsupported"
  (issue #80, audit AUDIT-013)
- The request-compression caches (`request_compression_mode`,
  `negotiated_request_compression`) migrated from `std::sync::RwLock` to
  `parking_lot::RwLock` (new dependency), removing the `PoisonError` recovery shims
  (issue #80)
- The CardDAV `mkcol` body builder now extracts the `D:prop` element with `quick-xml`
  instead of a string search: any namespace prefix (including a default-namespace-bound
  unprefixed `prop`) and any attributes on the element are handled, nested elements are
  captured correctly, and self-closing elements yield an empty inner body (issue #80)

## [0.9.2] - 2026-09-01

### Fixed
- `sync_collection` (CalDAV and CardDAV) now sends `Depth: 0` as mandated by RFC 6578 §3.3 —
  `Depth: 1` returned 400 on strict servers; the `supports_webdav_sync` probe was fixed the same
  way (external audit AUDIT-001)
- The aggregated body read/decompress phase is now bounded by the per-request timeout instead of
  hanging forever on stalled servers; `Error::Timeout` and `send_stream` docs state the exact
  phase-scoped semantics (external audit AUDIT-002)

### Security
- The publish workflow only triggers on version tags and verifies the tag matches the crate
  version before publishing (external audit AUDIT-005)
- `.env` / `.env.*` added to `.gitignore` (external audit AUDIT-030)

### Changed
- Batch helpers keep their infallible `expect`/`unwrap` calls with documented invariants;
  the audit finding AUDIT-022 is closed as statically infallible (typed-error conversion
  deferred to the 0.10 breaking window)

## [0.9.1] - 2026-09-01

### Added
- Unified multistatus parser shared by CalDAV and CardDAV, exposed as the new public path
  `fast_dav_rs::webdav::streaming` (unified `ElementName` marked `#[non_exhaustive]`,
  `DavItem` field superset, `parse_multistatus_*` family and `parse_error_body`)
- Free functions `etag_from_headers`, `normalize_etag`, `normalize_sync_token` re-exported
  at the crate root (canonical replacements for the deprecated associated helpers)

### Deprecated
- `set_request_compression` / `set_request_compression_auto` / `disable_request_compression`
  on `WebDavClient`, `CalDavClient`, `CardDavClient` — use the builder `request_compression(...)`
  or `set_request_compression_mode`
- Associated helpers `etag_from_headers` / `normalize_etag` / `normalize_sync_token` on all
  three clients — use the crate-root free functions
- `caldav::streaming::ElementName` / `element_from_bytes` and their CardDAV counterparts —
  use `fast_dav_rs::webdav::streaming::{ElementName, element_from_bytes}`
- `impl_multistatus_on_end!` macro (internal code-generation helper)
- Legacy root module paths (`fast_dav_rs::{client,streaming,types,compression}`, behind the
  `legacy` cargo feature) — removal scheduled in 0.10

### Changed
- ~1,050 lines of duplicated CalDAV/CardDAV client and streaming code unified behind shared
  `webdav/` helpers (parser, delegate macro, sync mapping, compression stack). Public API
  verified non-breaking by `cargo-semver-checks`; parser outputs verified byte-identical
  against 0.9.0 on a differential fixture corpus.

## [0.9.0] - 2026-08-26

### Added
- `#[non_exhaustive]` on all public response types for semver stability
- `DavCapabilities` struct and `capabilities()` method for parsing the `DAV` response header (RFC 4918 §10.1)
- `PropStat` struct to distinguish multiple `<D:propstat>` groups per response (RFC 4918 §13.1)
- `WebDavError` struct for parsing `<D:error>` precondition/postcondition codes (RFC 4918 §14.12)
- `parse_dav_header()` function for parsing comma-separated DAV header values
- `parse_error_body()` function for extracting `<D:error>` child element names
- `Collation` and `MatchType` enums for CardDAV text-match filtering (RFC 6352 §7.3)
- `TextMatch`, `ParamFilter`, and `CardDavFilter` types for CardDAV addressbook-query filters
- `is-not-defined` support in CardDAV prop-filter (RFC 6352 §7.1)
- `negate-condition` attribute on CardDAV text-match
- MSRV verification job (Rust 1.85) in CI
- `cargo audit` security scanning job in CI
- `CHANGELOG.md` (Keep a Changelog format)

### Changed
- Default `User-Agent` header is now `fast-dav-rs/{version}` instead of none
- Default `pool_idle_timeout` is now 90 seconds instead of unbounded
- CardDAV PUT Content-Type now includes `version=4.0` parameter (RFC 6352 §6.2.1)
- `build_addressbook_query_filter` now accepts configurable collation, match-type, and negate parameters
- Streaming parser now tracks multiple propstat groups and response-level status separately
- `DavItemCommon` gains `propstats` and `response_status` fields
- CalDAV/CardDAV `DavItem` gains `propstats` and `response_status` fields

### Removed
- Duplicate `integration_tests.rs` files in caldav and carddav test modules

### Fixed
- `let_chains` replaced with MSRV 1.85 compatible code
- Shared `on_end` and `apply_common` logic deduplicated via macros

## [0.8.0] - 2025-01-01

### Added
- Typed error handling via `thiserror` — `Error` enum replaces `anyhow`
- `Error::InvalidConfig` variant for builder configuration validation
- `Error::InvalidEtag` with `EtagReason` for ETag validation errors
- `Error::InvalidComponentName` for component name validation errors
- `Error::InvalidDateTime` for date-time validation errors
- `Error::UnexpectedStatus { operation, status }` with `Operation` enum
- `Error::Timeout { limit }` for timeout errors
- `Error::Tls` for manually-wrapped TLS/certificate/PEM errors
- Comprehensive unit tests improving SonarCloud coverage to >= 80%
- Codecov and SonarCloud coverage integration in CI
- Client builder pattern with fluent API for `WebDavClient`, `CalDavClient`, and `CardDavClient`
- Request compression support (gzip, brotli, zstd) with `RequestCompressionMode`
- Connection pool configuration (`pool_max_idle_per_host`, `pool_idle_timeout`)
- Proxy support with optional Basic authentication
- Additional PEM-encoded trust roots for debugging (Proxyman/Charles/mitmproxy)
- `danger_accept_invalid_certs` option for testing/debug scenarios
- `connect_timeout` option for the TCP connector
- `force_http1` option to disable HTTP/2 negotiation
- Bearer token authentication (OAuth 2.0)
- Streaming XML parsing for large multistatus responses
- Backpressure-aware streaming operations

### Changed
- Migrated from `anyhow` to `thiserror` for typed error handling
- Bumped to Rust 2024 edition
- Minimum Supported Rust Version (MSRV) set to 1.85
- HTTP client now uses `hyper` 1.x with `hyper-util`
- TLS stack uses `rustls` with native cert loading and webpki fallback
- ETag extraction and formatting hardened before sending requests
- Sync-collection token handling normalized (unquoted in request bodies)
- Multistatus parsing hardened against malformed and stalled responses
- Calendar query inputs escaped and validated to prevent XML injection
- Sync-support detection and deletion status parsing corrected
- Lock poisoning handling improved
- Probe negotiation corrected
- `webpki-roots` bumped to 1.0
- Dev-dependencies version constraints relaxed

### Fixed
- Default feature correctly set on `thiserror`
- ETag and CTag reading normalized
- Streaming multistatus parsing hardened against malformed and stalled responses
- Sync-support detection and deletion status parsing corrected
- Lock poisoning handling improved
- Probe negotiation corrected
- Calendar query inputs escaped and validated against XML injection
- Credentials hardened against leakage

## [0.7.2] - 2024-12-01

### Changed
- Improved release configuration and package metadata

## [0.7.1] - 2024-11-01

### Changed
- `webpki-roots` bumped to 1.0
- Dev-dependencies version constraints relaxed

## [0.7.0] - 2024-10-01

### Added
- Client builder pattern for configurable auth, timeout, pool, TLS, and proxy

### Fixed
- ETag and CTag reading normalized

## [0.6.0] - 2024-09-01

### Added
- Client builder with fluent API

## [0.5.0] - 2024-08-01

### Fixed
- ETag extraction and formatting improved before sending requests
- Streaming multistatus parsing hardened

## [0.4.4] - 2024-07-01

### Fixed
- Streaming multistatus parsing hardened

## [0.4.0] - 2024-06-01

### Security
- Calendar query inputs escaped and validated to prevent XML injection
- Credentials hardened against leakage

### Fixed
- Sync-support detection and deletion status parsing corrected
- Lock poisoning handling improved
- Probe negotiation corrected
