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

# Run all tests with all features
cargo test --all-features

# Run documentation tests
cargo test --doc

# Run only unit tests
cargo test --test unit_tests

# Run only end-to-end tests
cargo test --test e2e_tests

# Run a single specific test
cargo test --test unit_tests test_name

# Run a single test with more verbose output
cargo test --test unit_tests test_name -- --nocapture

# Run tests in specific module
cargo test --test unit_tests webdav::client::tests
```

### Shell Scripts
- `./run-unit-tests.sh` - Quick unit test execution
- `./run-e2e-tests.sh` - End-to-end tests against SabreDAV server (requires Docker)
- `./sabredav-test/setup.sh` - Sets up E2E test environment
- `./sabredav-test/reset-db.sh` - Resets E2E test database

### CI Configuration
The project uses GitHub Actions with these key steps:
1. `cargo fmt --all --check` - Verify formatting
2. `cargo clippy --all-targets --all-features -- -D warnings` - Lint with strict warnings
3. `cargo test --all-features --locked --test unit_tests` - Run unit tests

## Code Style Guidelines

### Project Structure
```
src/
├── lib.rs              # Main library entry point with comprehensive examples
├── webdav/             # Core WebDAV functionality
│   ├── client.rs       # WebDavClient and HTTP operations
│   ├── types.rs        # Common types and enums
│   ├── streaming.rs    # Streaming XML parsing
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
```

### Imports and Dependencies
- Use `anyhow::{Result, anyhow}` for error handling throughout the codebase
- Prefer `use anyhow::Result;` over custom error types unless needed
- Standard HTTP and body utilities from `hyper`, `http-body-util`, `bytes`
- Async runtime: `tokio` with `macros`, `rt-multi-thread`, `time` features
- Use `futures` and `futures-util` for stream operations
- XML processing: `quick-xml` with `async-tokio` feature

### Type System and Traits
- Use `#[derive(Debug, Clone)]` for public structs that represent data
- Use `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` for enums and simple types
- Client structs should implement `Clone` and be cheap to clone (they share connection pools)
- Use `Arc<RwLock<T>>` for shared mutable state with occasional writes
- Use `tokio::sync::{Mutex, Semaphore}` for async synchronization

### Error Handling
- Return `Result<T>` from public functions
- Use `anyhow::anyhow!()` for creating errors with context
- Use `map_err()` to add context to external library errors
- For user-facing errors, provide clear, actionable messages
- Use `?` operator extensively for error propagation

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
3. `cargo test --all-features` passes
4. `cargo test --doc` passes
5. All new public APIs have documentation
6. Examples in documentation compile and run
7. Error handling is consistent and comprehensive
8. No TODO or FIXME comments left in final code