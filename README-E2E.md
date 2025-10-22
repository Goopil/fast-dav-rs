# Fast CalDAV RS - E2E Testing

This directory contains end-to-end tests for the fast-dav-rs CalDAV client library.

## Test Environment

The E2E tests require a running SabreDAV server with the following configuration:
- Nginx with compression modules (gzip, Brotli, zstd)
- PHP-FPM backend with SabreDAV
- MySQL database with preconfigured tables
- Test user: `test` / `test`

## Running Tests

### Automated Setup

1. Make sure Docker is running
2. Run the test setup script:
   ```bash
   cd sabredav-test
   ./setup.sh
   ```

3. In another terminal, run the tests:
   ```bash
   cargo test --test e2e_tests
   ```

### Manual Setup

If you prefer to manage the test environment manually:

1. Start the SabreDAV test environment:
   ```bash
   cd sabredav-test
   docker-compose up -d
   ```

2. Run the tests:
   ```bash
   cargo test --test e2e_tests
   ```

## Test Organization

The E2E tests are organized into functional domains:

### Core Functionality (`core/`)
- Basic connectivity and HTTP methods

### Discovery Operations (`discovery/`)
- Principal and calendar discovery
- Resource enumeration

### Calendar Operations (`operations/`)
- Calendar creation and management
- Property updates

### Event Operations (`operations/`)
- Event CRUD operations
- Conditional updates with ETags

### Resource Operations (`operations/`)
- COPY and MOVE operations

### Compression Support (`compression/`)
- Request/response compression
- All supported algorithms

### Streaming Operations (`streaming/`)
- Streamed PROPFIND and REPORT operations
- Memory-efficient response handling

### Parallel Operations (`parallel/`)
- Batch PROPFIND and REPORT operations
- Concurrent request handling

See `tests/caldav/README.md` for detailed test organization.

## Test Coverage

The E2E tests verify the following CalDAV operations:

### Basic Operations
- Connectivity testing with GET, OPTIONS, HEAD requests
- PROPFIND operations on various WebDAV resources
- Compression support (gzip, Brotli, zstd)
- Response handling for compressed content

### Calendar Management
- Creating calendars with MKCALENDAR
- Listing and discovering calendars
- Updating calendar properties with PROPPATCH
- Deleting calendars

### Event Management
- Creating calendar events with PUT
- Retrieving events with GET
- Updating events with conditional PUT (If-Match)
- Deleting events with DELETE

### Resource Operations
- Copying resources with COPY
- Moving resources with MOVE
- Principal and calendar discovery

### Advanced Features
- Streaming response handling for large collections
- Parallel/batch operations for efficient processing
- Memory-efficient XML parsing
- Concurrent request execution

## Resetting the Test Environment

To reset the database and start fresh:
```bash
cd sabredav-test
./reset-db.sh
```

## Troubleshooting

If tests fail to connect:
1. Ensure Docker is running
2. Check that all containers are up: `docker-compose ps`
3. Verify the server is accessible: Check the logs with `docker-compose logs nginx`