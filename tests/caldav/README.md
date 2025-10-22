# CalDAV Client Test Suite

This directory contains a comprehensive test suite for the fast-dav-rs CalDAV client, organized by functionality domains.

## Test Organization

### 🧪 Core Functionality
- **Connectivity Tests** - Basic HTTP operations (GET, OPTIONS, HEAD)
- Located in: `core/connectivity_tests.rs`

### 🔍 Discovery Operations
- **Principal Discovery** - Finding user principals and calendar homes
- **Resource Enumeration** - Listing calendars and events
- Located in: `discovery/discovery_tests.rs`

### 📅 Calendar Operations
- **Calendar Creation** - MKCALENDAR operations
- **Property Updates** - PROPPATCH operations
- Located in: `operations/calendar_tests.rs`

### 🗓️ Event Operations
- **Event CRUD** - Create, Read, Update, Delete operations
- **Conditional Updates** - ETag-based operations
- Located in: `operations/event_tests.rs`

### 📦 Resource Operations
- **COPY Operations** - Duplicating resources
- **MOVE Operations** - Relocating resources
- Located in: `operations/resource_tests.rs`

### 🗜️ Compression Support
- **Request Compression** - gzip, brotli, zstd support
- **Response Handling** - Decompression capabilities
- Located in: `compression/compression_tests.rs`

## Running Tests

### Prerequisites
1. SabreDAV test environment running (`cd sabredav-test && ./setup.sh`)
2. Docker installed and running

### Execute All Tests
```bash
cargo test --test e2e_tests
```

### Execute Specific Test Domain
```bash
# Core connectivity tests
cargo test --test e2e_tests core

# Discovery tests
cargo test --test e2e_tests discovery

# Calendar operations
cargo test --test e2e_tests operations::calendar_tests

# Event operations
cargo test --test e2e_tests operations::event_tests

# Resource operations
cargo test --test e2e_tests operations::resource_tests

# Compression tests
cargo test --test e2e_tests compression
```

## Test Coverage

The test suite validates:
- ✅ HTTP/1.1 and HTTP/2 support
- ✅ Basic and Digest authentication
- ✅ All CalDAV operations (MKCALENDAR, PROPFIND, PUT, GET, DELETE)
- ✅ Advanced operations (COPY, MOVE, PROPPATCH)
- ✅ Conditional operations (If-Match, If-None-Match)
- ✅ Compression support (gzip, brotli, zstd)
- ✅ Calendar and event discovery
- ✅ Proper error handling
- ✅ ETag support
- ✅ WebDAV Sync capabilities

## Continuous Integration

These tests are designed to run in CI/CD pipelines and provide comprehensive validation of the CalDAV client's functionality against real SabreDAV servers.