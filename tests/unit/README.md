# Unit Tests

Unit tests for fast-dav-rs, organized by module. Entry point: `tests/unit/mod.rs`
(the `unit_tests` target in `Cargo.toml`).

## Layout

| Directory | Files | Covers |
|---|---|---|
| `caldav/` | `client_tests.rs`, `caldav_helpers.rs`, `parser_tests.rs`, `parser_edge_cases.rs`, `streaming_tests.rs`, `filter_tests.rs`, `validation_tests.rs`, `etag_tests.rs`, `scheduling_tests.rs`, `timezone_tests.rs`, `attachments_tests.rs`, `xml_helper_tests.rs` | CalDAV client, filters (RFC 4791 DTD exclusivity), iCalendar validation, scheduling (RFC 6638), timezones (RFC 4791 §5.2.2), managed attachments (RFC 8607), ETag helpers, XML building |
| `carddav/` | `client_tests.rs`, `carddav_helpers.rs`, `parser_tests.rs`, `parser_edge_cases.rs`, `streaming_tests.rs`, `filter_tests.rs`, `prop_extraction_tests.rs`, `etag_tests.rs`, `xml_helper_tests.rs` | CardDAV client, vCard filters (RFC 6352 DTD exclusivity), property extraction, ETag helpers |
| `webdav/` | `auth_tests.rs`, `builder_tests.rs`, `compliance_tests.rs`, `compression_probe_tests.rs`, `discovery_tests.rs`, `locking_tests.rs`, `prefer_tests.rs`, `privileges_tests.rs`, `protocol_tests.rs`, `redirect_tests.rs`, `retry_tests.rs`, `streaming_tests.rs`, `sync_tests.rs`, `sync_capability_tests.rs`, `sync_session_tests.rs`, `tracing_tests.rs`, `uri_tests.rs` | Shared WebDAV core: auth (Basic/Bearer/`TokenProvider`), builder, `DAV:` header parsing, compression negotiation, discovery, locking (RFC 4918), `Prefer` (RFC 7240), privileges (RFC 3744), redirects, retry/backoff, streaming, sync (RFC 6578) + `SyncSession`, `tracing` instrumentation, URI handling |
| `common/` | `compression_tests.rs`, `compression_integration_tests.rs`, `error_tests.rs`, `http_helpers.rs` | Shared compression helpers, the typed `Error` enum, HTTP test helpers |

## Running

```bash
# All unit tests (preferred)
cargo nextest run --all-features --locked --test unit_tests

# One module
cargo nextest run --test unit_tests webdav::locking_tests

# One test, verbose
cargo nextest run --test unit_tests webdav::locking_tests::test_name -- --nocapture
```

## Notes

- E2E tests (against live Docker fixtures) live in `tests/e2e/` — see the
  per-fixture READMEs there and `README.md` ("End-to-End Testing").
- Every new public API or error variant needs unit tests here, including the
  error case; SonarCloud enforces ≥80% coverage on new code.
