# Unit Tests

This directory contains unit tests for the fast-dav-rs library, organized by module.

## Test Organization

### 📦 CalDAV Module Tests
- **Client Tests** - `client_tests.rs`
  - Client creation and URI building
  - Depth enum values
  - XML escaping functions
  - XML body builders

- **Helpers Tests** - `caldav_helpers.rs`
  - Calendar query builders
  - Calendar multiget builders
  - Sync collection builders
  - Response mapping functions

- **Parser Tests** - `parser_tests.rs`
  - Multistatus XML parsing
  - Calendar property extraction

- **Streaming Tests** - `streaming_tests.rs`
  - Streaming XML parsing (if applicable)

- **Integration Tests** - `integration_tests.rs`
  - Combined functionality tests

- **ETag Tests** - `etag_tests.rs`
  - ETag header parsing and handling
  - Conditional request helpers

- **Builder Tests** - `builder_tests.rs`
  - XML body builder edge cases
  - Complex query construction

- **XML Helper Tests** - `xml_helper_tests.rs`
  - XML escaping utilities
  - Unicode handling

- **Parser Edge Cases** - `parser_edge_cases.rs`
  - Malformed XML handling
  - Performance testing
  - Unexpected element handling

### 📦 CardDAV Module Tests
- **Client Tests** - `client_tests.rs`
  - Client creation and URI building
  - Depth enum values
  - XML escaping functions
  - XML body builders

- **Helpers Tests** - `carddav_helpers.rs`
  - Addressbook query builders
  - Addressbook multiget builders
  - Sync collection builders
  - Response mapping functions

- **Parser Tests** - `parser_tests.rs`
  - Multistatus XML parsing
  - Addressbook property extraction

- **Streaming Tests** - `streaming_tests.rs`
  - Streaming XML parsing (if applicable)

- **Integration Tests** - `integration_tests.rs`
  - Combined functionality tests

- **ETag Tests** - `etag_tests.rs`
  - ETag header parsing and handling
  - Conditional request helpers

- **XML Helper Tests** - `xml_helper_tests.rs`
  - XML escaping utilities
  - Unicode handling

- **Parser Edge Cases** - `parser_edge_cases.rs`
  - Malformed XML handling
  - Performance testing
  - Unexpected element handling

### 🗜️ Common Module Tests
- **Compression Tests** - `compression_tests.rs`
  - Content encoding detection
  - Header manipulation
  - Basic compression functions

- **Compression Integration Tests** - `compression_integration_tests.rs`
  - Full compress/decompress cycles
  - Performance with large data
  - Multiple compression formats

## Running Tests

### Execute All Unit Tests
```bash
cargo test --test unit_tests
```

### Execute Specific Test Module
```bash
# Client tests
cargo test --test unit_tests client_tests

# Parser tests
cargo test --test unit_tests parser_tests

# Compression tests
cargo test --test unit_tests compression_tests

# ETag tests
cargo test --test unit_tests etag_tests
```

## Test Coverage

The unit tests validate:
- ✅ Client creation and configuration
- ✅ URI building logic
- ✅ XML escaping and construction
- ✅ HTTP header handling
- ✅ Response parsing and mapping
- ✅ Compression/decompression
- ✅ ETag handling
- ✅ Error conditions
- ✅ Edge cases
- ✅ Performance characteristics
- ✅ Unicode support
