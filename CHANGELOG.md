# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- MSRV verification job (Rust 1.85) in CI
- `cargo audit` security scanning job in CI

### Changed
- Default `User-Agent` header is now `fast-dav-rs/{version}` instead of none
- Default `pool_idle_timeout` is now 90 seconds instead of unbounded

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
