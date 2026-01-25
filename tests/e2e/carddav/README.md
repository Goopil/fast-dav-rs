# CardDAV Client Test Suite

This directory contains E2E tests for the fast-dav-rs CardDAV client.

## Test Organization

### 🧪 Core Functionality
- **Connectivity Tests** - Basic HTTP operations (GET, OPTIONS, HEAD)
- Located in: `core/connectivity_tests.rs`

### 🔍 Discovery Operations
- **Principal Discovery** - Finding user principals and addressbook homes
- **Resource Enumeration** - Listing addressbooks
- Located in: `discovery/discovery_tests.rs`

### 📇 Addressbook Operations
- **Addressbook Creation** - MKADDRESSBOOK operations
- **Property Updates** - PROPPATCH operations
- Located in: `operations/addressbook_tests.rs`

### 👤 Contact Operations
- **Contact CRUD** - Create, Read, Update, Delete operations
- **Query Helpers** - UID-based queries
- Located in: `operations/contact_tests.rs`

### 🗜️ Compression Support
- **Request Compression** - gzip, brotli, zstd support
- **Response Handling** - Decompression capabilities
- Located in: `compression/compression_tests.rs`

### 🌊 Streaming Operations
- **Streamed PROPFIND** - Memory-efficient large responses
- **Streamed REPORT** - Streaming WebDAV reports
- Located in: `streaming/streaming_tests.rs`

### 🔁 Sync Operations
- **WebDAV Sync Support** - Server capability detection
- **Initial/Delta Sync** - Sync token based updates
- Located in: `sync/sync_tests.rs`

## Running Tests

### Prerequisites
1. SabreDAV test environment running (`cd sabredav-test && ./setup.sh`)
2. Docker installed and running

### Execute All Tests
```bash
cargo test --test e2e_tests carddav
```

### Execute Specific Test Domain
```bash
# Core connectivity tests
cargo test --test e2e_tests carddav::core

# Discovery tests
cargo test --test e2e_tests carddav::discovery

# Addressbook operations
cargo test --test e2e_tests carddav::operations::addressbook_tests

# Contact operations
cargo test --test e2e_tests carddav::operations::contact_tests
```
