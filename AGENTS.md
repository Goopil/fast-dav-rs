# Repository Guidelines for fast-dav-rs

This document contains essential information for agentic coding agents working in this repository.

## Build, Test, and Lint Commands

### Development Commands
```bash
# Build the project
cargo build

# Format code (mandatory before pushing)
cargo fmt

# Run linter with strict warnings (mandatory before pushing)
cargo clippy --all-targets --all-features -- -D warnings

# Run unit tests with nextest (preferred)
cargo nextest run --all-features --locked --test unit_tests

# Run documentation tests
cargo test --doc --all-features

# Run only end-to-end tests
cargo test --test e2e_tests

# Run e2e tests against the Radicale fixture (bring it up first)
cargo test --test e2e_radicale

# Run e2e tests against the Nextcloud fixture (bring it up first)
cargo test --test e2e_nextcloud

# Run the opt-in Provider A smoke tier (skips when PROVIDER_A_DAV_URL is unset)
PROVIDER_A_DAV_URL=https://dav.example.test cargo test --test e2e_provider_a_smoke -- --ignored

# Run a single specific test
cargo nextest run --test unit_tests test_name

# Run a single test with more verbose output
cargo nextest run --test unit_tests test_name -- --nocapture

# Run tests in specific module
cargo nextest run --test unit_tests webdav::client::tests

# Run coverage report
cargo llvm-cov nextest --test unit_tests --all-features --no-fail-fast --lcov --output-path target/llvm-cov/lcov.info
```

### Shell Scripts
- `./run-unit-tests.sh` - Quick unit test execution
- `./run-e2e-tests.sh` - End-to-end tests against SabreDAV server (requires Docker)
- `./sabredav-test/setup.sh` - Sets up E2E test environment
- `./sabredav-test/reset-db.sh` - Resets E2E test database
- `./radicale-test/setup.sh` - Starts + seeds the Radicale fixture (http://localhost:8081)
- `./radicale-test/reset.sh` - Resets the Radicale fixture (tmpfs wipe + re-seed)
- `./nextcloud-test/setup.sh` - Starts + provisions the Nextcloud fixture (http://localhost:8083; first boot is slow)
- `./nextcloud-test/reset.sh` - Full Nextcloud fixture wipe + reinstall

### CI Configuration
The project uses GitHub Actions with these key steps:
1. `cargo fmt --all --check` - Verify formatting
2. `cargo clippy --all-targets --all-features -- -D warnings` - Lint with strict warnings
3. `cargo nextest run --all-features --locked --test unit_tests` - Run unit tests
4. `cargo build --examples --all-features --locked` - Build examples
5. `cargo test --doc --all-features --locked` - Run doc tests

### SonarCloud Quality Gates
All PRs are analyzed by SonarCloud. The following gates **must always pass** on every PR:
1. **Coverage on New Code** ≥ 80% — New lines must be covered by unit tests. Code only reachable via e2e tests (HTTP methods against a live DAV server) is exempt; document exemptions in the PR if a gate fails for this reason.
2. **Duplications on New Code** ≤ 3% — Avoid copy-paste between `caldav/` and `carddav/`. Share logic via `webdav/` or `common/` instead of duplicating client method bodies.

These gates are mandatory and must not be bypassed. If a gate fails, fix the code before merging.

## Code Style Guidelines

### Project Structure
```
src/
├── lib.rs              # Main library entry point with comprehensive examples
├── webdav/             # Core WebDAV functionality
│   ├── client.rs       # WebDavClient and HTTP operations
│   ├── types.rs        # Common types and enums
│   ├── streaming.rs    # Streaming XML parsing
│   ├── sync.rs         # RFC 6578 sync sessions (SyncSession engine)
│   ├── xml.rs          # XML building utilities
│   └── mod.rs
├── caldav/             # CalDAV-specific functionality
│   ├── client.rs       # CalDavClient
│   ├── types.rs        # Calendar-specific types
│   ├── streaming.rs    # Calendar-specific streaming
│   └── mod.rs
├── carddav/            # CardDAV-specific functionality
│   ├── client.rs       # CardDavClient
│   ├── types.rs        # Address book types
│   ├── streaming.rs    # vCard streaming
│   └── mod.rs
└── common/             # Shared utilities
    ├── compression.rs  # HTTP compression handling
    ├── http.rs         # HTTP client configuration
    └── mod.rs

examples/               # Runnable example binaries (one per workflow; fixture prerequisites documented per file, see README "Runnable Examples")
```

### Imports and Dependencies
- Use `thiserror` for typed error handling via the `Error` enum in `src/error.rs`
- Standard HTTP and body utilities from `hyper`, `http-body-util`, `bytes`
- Async runtime: `tokio` with `macros`, `rt-multi-thread`, `time` features
- Use `futures` and `futures-util` for stream operations
- XML processing: `quick-xml` with `async-tokio` feature
- TLS: `rustls` with `rustls-pemfile`, `rustls-native-certs`, `webpki-roots`

### Type System and Traits
- Use `#[derive(Debug, Clone)]` for public structs that represent data
- Use `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` for enums and simple types
- Client structs should implement `Clone` and be cheap to clone (they share connection pools)
- Use `Arc<RwLock<T>>` for shared mutable state with occasional writes
- Use `tokio::sync::{Mutex, Semaphore}` for async synchronization

### Error Handling
- Return `Result<T>` (aliased to `Result<T, Error>`) from public functions
- Use the `Error` enum from `src/error.rs` for all error creation
- Use `Error::InvalidEtag` (with `EtagReason`) for ETag validation errors
- Use `Error::InvalidComponentName` for component name validation errors
- Use `Error::InvalidDateTime` for date-time validation errors
- Use `Error::InvalidConfig` for builder configuration errors
- Use `Error::InvalidInput(String)` only as an external escape-hatch for validation errors not covered by a specific variant
- Use `Error::UnexpectedStatus { operation, status }` with the `Operation` enum for HTTP status mismatches
- Use `Error::Timeout { limit }` for timeout errors
- Use `Error::Tls` for manually-wrapped TLS/certificate/PEM errors; `Error::TlsRustls` is auto-converted via `#[from]`
- Use `Error::other()` or `Error::with_source()` for catch-all errors (escape-hatch only)
- Use `?` operator extensively for error propagation — `#[from]` conversions handle most library errors automatically
- The `Error` enum and all struct variants are `#[non_exhaustive]` — always include a wildcard arm when matching

### Naming Conventions
- **Structs**: `PascalCase` (e.g., `CalDavClient`, `WebDavClient`)
- **Functions**: `snake_case` (e.g., `discover_current_user_principal`, `propfind_stream`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `AUTO_DEFAULT_ENCODING`)
- **Modules**: `snake_case` matching directory structure
- **Enums**: `PascalCase` with `snake_case` variants
- **Private functions**: `snake_case` with descriptive names

### Async/Concurrency Patterns
- All public client methods should be `async fn`
- Use `tokio::time::timeout` for operations that may hang
- Implement bounded concurrency with `tokio::sync::Semaphore`
- Use `FuturesOrdered` for ordered concurrent operations
- Clone clients freely; they're designed to be cheap and share connections

### HTTP and WebDAV Specifics
- Use `hyper` 1.x as the HTTP client
- Support both HTTP/1.1 and HTTP/2
- Handle compression automatically (gzip, brotli, zstd)
- Use proper conditional headers: `If-Match`, `If-None-Match`
- Implement ETag-based operations for safety
- Support WebDAV depth headers: `Depth::Zero`, `Depth::One`, `Depth::Infinity`

### Testing Guidelines
- Unit tests go in `tests/unit/` directory
- E2E tests go in `tests/e2e/` directory
- Use `#[tokio::test]` for async test functions
- Include both happy path and error case tests
- Test error scenarios with proper result checking
- Use descriptive test names that indicate the scenario

### Documentation Requirements
- All public APIs must have doc comments
- Include examples in doc comments using ````no_run` blocks
- Use proper markdown formatting in documentation
- Document error conditions and edge cases
- Include performance considerations where relevant
- **Keep documentation files in sync with code changes** — when adding, removing, or modifying public APIs, error variants, features, or configuration options, always update `README.md`, `AGENTS.md`, and any relevant examples in `examples/`. Stale documentation is a bug.

### Module Re-exports
- Each module's `mod.rs` should contain `pub use` re-exports for clean public API
- Main `lib.rs` provides both modular and legacy re-exports for backward compatibility
- Group related re-exports logically in each module

### Streaming and Memory Efficiency
- Prefer streaming APIs for large responses
- Use `parse_multistatus_stream` for large XML responses
- Implement backpressure awareness in streaming operations
- Use `Bytes` for efficient byte buffer handling

### Compression and Performance
- Auto-negotiate compression where possible
- Cache compression preferences after probing
- Support multiple compression algorithms: gzip, brotli, zstd
- Implement efficient connection pooling and reuse

### Code Review Checklist
Before submitting PRs, ensure:
1. `cargo fmt` passes without changes
2. `cargo clippy --all-targets --all-features -- -D warnings` passes
3. `cargo nextest run --all-features --locked --test unit_tests` passes
4. `cargo test --doc --all-features` passes
5. `cargo build --examples --all-features` passes
6. All new public APIs have documentation
7. Examples in documentation compile and run
8. Error handling is consistent and comprehensive
9. No TODO or FIXME comments left in final code
10. No copy-paste duplication between `caldav/` and `carddav/` (share via `webdav/` or `common/`)