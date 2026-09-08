# Sync E2E Tests (SabreDAV)

CalDAV synchronization tests against the SabreDAV fixture: traditional
PROPFIND-based sync and WebDAV Sync (RFC 6578).

## Files

- `sync_tests.rs` — core WebDAV Sync functionality (sync-token round-trips,
  initial sync, incremental deltas, truncation)
- `comparison_tests.rs` — parity between traditional and WebDAV sync methods
- `truncation_tests.rs` — `507 Insufficient Storage` truncation handling
