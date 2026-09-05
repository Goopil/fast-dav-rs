# Provider compatibility matrix

This matrix tracks which server features the end-to-end suites actually
verify per provider fixture. It is **evidence-based**:

- ✅ — an e2e test in `tests/e2e/` exercises and asserts the behavior against
  that fixture (the citing test is named in the evidence table below).
- ◐ — partially exercised: the cited test asserts a narrower slice of the
  behavior (see the note).
- ❌ — the fixture is known not to support the feature; the note carries the
  observed evidence.
- — — not tested: no e2e assertion exists. This is a statement about test
  coverage, **never** about whether the server supports the feature.

**Adding a provider:** new providers are added to this matrix only together
with a runnable fixture under `tests/e2e/` (and its `*-test/` setup scripts);
a row without a fixture cannot carry evidence.

Fixtures and test targets:

| Fixture | Setup | Test target | Notes |
| --- | --- | --- | --- |
| SabreDAV | `sabredav-test/setup.sh` (:8080) | `--test e2e_tests` | primary fixture |
| Radicale | `radicale-test/setup.sh` (:8081) | `--test e2e_radicale` | Radicale 3.7.6 |
| Nextcloud | `nextcloud-test/setup.sh` (:8083) | `--test e2e_nextcloud` | |
| Provider A | no fixture (opt-in, credential-free) | `--test e2e_provider_a_smoke -- --ignored` | skips itself without `PROVIDER_A_DAV_URL` |

## Matrix

| Feature | SabreDAV | Radicale | Nextcloud | Provider A |
| --- | --- | --- | --- | --- |
| Discovery (RFC 6764) | ✅ | ✅ | ✅ | ◐ (A1) |
| WebDAV-Sync (RFC 6578) | ✅ | ✅ | ✅ | — |
| LOCK (RFC 4918 class 2) | ✅ | ❌ (R1) | ✅ | — |
| Scheduling (RFC 6638) | ✅ (S1) | — | — | — |
| `calendar-timezone` (RFC 4791 §5.2.2) | — (S2) | ✅ | ✅ | — |
| Compression (gzip / brotli / zstd) | ✅ | — | — | — |
| OAuth / Bearer auth | — (S3) | — | — (N1) | — |

Notes:

- **(A1) Discovery on Provider A** is exercised only unauthenticated by the
  smoke tier: the well-known shape (3xx redirect carrying `Location`, or a
  direct 401 with a `WWW-Authenticate: Basic` challenge) is asserted, and an
  unauthenticated principal `PROPFIND` must answer 401 without leaking the
  principal. No authenticated discovery flow is run.
- **(R1) Radicale has no LOCK.** `OPTIONS /` advertises `DAV: 1, 2, 3`, yet a
  `LOCK` request is answered `405 Method Not Allowed` (observed on Radicale
  3.7.6; also listed in `radicale-test/README.md`). Clients must not rely on
  locking even when the compliance class is advertised.
- **(S1) SabreDAV scheduling** is verified for endpoint discovery
  (schedule-inbox-URL, schedule-outbox-URL, calendar-user-address-set) and
  schedule-inbox listing. Two gaps: the fixture (SabreDAV 4.7.1) does not
  implement the RFC 6638 §8 schedule-tag mechanism — the e2e records the
  observed behavior — and the outbox `POST` flow has no e2e coverage.
- **(S2) SabreDAV `calendar-timezone`** has no read-back assertion:
  `test_parsing_edge_case_timezones` sets the property at `MKCALENDAR` time
  but only logs the outcome, and the `PROPPATCH` write path is untested on
  this fixture. Radicale and Nextcloud round-trip the property via
  `PROPPATCH` (set → read back, remove → absent).
- **(S3) Bearer auth is not exercised against the SabreDAV fixture.**
  `test_bearer_auth_reaches_the_wire` proves the client emits
  `Authorization: Bearer` on the wire — but against a local echo server, not
  the fixture. The fixture itself authenticates with Basic.
- **(N1) The Nextcloud fixture** authenticates with Basic (app passwords are
  the documented path for hardened instances); Bearer/OIDC is explicitly out
  of scope for the fixture (`nextcloud-test/README.md`). The
  `nextcloud_client` example demonstrates the bearer builder API, but
  examples are not tests.

## Evidence

All paths relative to `tests/e2e/`. A ✅/❌/◐ cell in the matrix maps to the
tests below; `—` cells have no test and are therefore absent.

| Feature | Fixture | Verdict | Evidence (test file → tests) |
| --- | --- | --- | --- |
| Discovery | SabreDAV | ✅ | `sabredav/webdav/discovery_tests.rs` → `test_discover_caldav_follows_well_known_redirect`, `test_discover_carddav_follows_well_known_redirect` (`.well-known` 301 followed, final URI asserted); `sabredav/caldav/discovery/discovery_tests.rs` → `test_discovery_operations` (principal + calendar-home-set + calendar list); `sabredav/carddav/discovery/discovery_tests.rs` → `test_discovery_operations` |
| Discovery | Radicale | ✅ | `radicale/core.rs` → `test_discover_principal_and_home_sets` (principal, calendar- and addressbook-home-set asserted) |
| Discovery | Nextcloud | ✅ | `nextcloud/discovery.rs` → `test_discover_principal_and_home_sets`, `test_dav_root_scoping` (`/remote.php/dav/` scoping asserted) |
| Discovery | Provider A | ◐ (A1) | `provider_a/mod.rs` → `test_smoke_well_known_caldav_shape`, `test_smoke_well_known_carddav_shape`, `test_smoke_unauthenticated_propfind_current_user_principal` |
| WebDAV-Sync | SabreDAV | ✅ | `sabredav/caldav/sync/sync_tests.rs` → `test_initial_sync_collection`, `test_incremental_sync`, `test_sync_deletion_tracking`, `test_sync_limit_and_pagination`, `test_sync_resilient_reinitializes_on_unknown_token`, `test_sync_session_tracks_additions_and_deletions`; `sabredav/caldav/sync/truncation_tests.rs` → `test_sync_collection_not_truncated` |
| WebDAV-Sync | Radicale | ✅ | `radicale/sync.rs` → `test_sync_session_invalid_token_transparent_resync` (403 + `valid-sync-token` → transparent resync, `resynced == true` asserted), `test_sync_collection_unknown_token_records_observed_behavior` |
| WebDAV-Sync | Nextcloud | ✅ | `nextcloud/sync.rs` → `test_sync_session_initial_and_empty_incremental` |
| LOCK | SabreDAV | ✅ | `sabredav/webdav/locking_tests.rs` → `test_lock_refresh_unlock_relock_lifecycle`, `test_put_succeeds_after_unlock` |
| LOCK | Radicale | ❌ (R1) | `radicale/locking.rs` → `test_lock_unsupported_records_observed_behavior` (LOCK → error status, 405 observed) |
| LOCK | Nextcloud | ✅ | `nextcloud/locking.rs` → `test_lock_unlock_round_trip_on_nextcloud` |
| Scheduling | SabreDAV | ✅ (S1) | `sabredav/caldav/scheduling_tests.rs` → `test_discover_schedule_endpoints_on_sabredav` (inbox/outbox hrefs + `mailto:` user address asserted), `test_list_inbox_empty_on_sabredav`; schedule-tag gap recorded by `test_schedule_tag_unsupported_records_observed_behavior_on_sabredav` |
| `calendar-timezone` | Radicale | ✅ | `radicale/core.rs` → `test_calendar_timezone_write_round_trip` (PROPPATCH set → stored verbatim, remove → reads back absent) |
| `calendar-timezone` | Nextcloud | ✅ | `nextcloud/crud.rs` → `test_calendar_timezone_write_round_trip` (PROPPATCH set → read back, remove → absent) |
| Compression | SabreDAV | ✅ | `sabredav/caldav/compression/compression_tests.rs` → `test_compression_support` (forced gzip/br/zstd requests succeed), `test_compressed_response_handling`; `sabredav/carddav/compression/compression_tests.rs` → `test_compressed_response_handling`; `sabredav/webdav/auto_probe_tests.rs` (Auto probe never poisons a write) |

## Per-fixture notes

### SabreDAV (primary fixture)

- Full CalDAV + CardDAV stack behind nginx (gzip/brotli/zstd) + PHP-FPM + MySQL; user `test`/`test`.
- Cleartext `http://` behind nginx: HTTP/1.1 only (h2 requires TLS/ALPN) — pinned by `test_sabredav_negotiates_http_1_1_only`.
- Schedule plugin enabled (RFC 6638 endpoint discovery + inbox listing); schedule-tag is **not** implemented by SabreDAV 4.7.1 (recorded, not a failure).
- `calendar-timezone` is only loosely exercised at `MKCALENDAR` time; the `PROPPATCH` write path is untested.
- Auth fixture-side is Basic; Bearer tokens are verified only against a local echo server.

### Radicale

- Radicale 3.7.6 on tmpfs, Basic auth (`test`/`test`); see `radicale-test/README.md` for the full quirk list.
- **No LOCK**: `DAV: 1, 2, 3` is advertised but `LOCK` answers `405` (note R1).
- WebDAV-Sync supported; stale tokens answer `403` + `<valid-sync-token/>` (RFC 6578 §3.2) — the resilient sync and `SyncSession` transparent resync are asserted by e2e.
- `calendar-timezone` (RFC 4791 §5.2.2) `PROPPATCH` round-trip verified (set → read back; remove → absent).
- Auto-create on first principal access; `.well-known` URIs answer `301` to `/`.
- No scheduling surface (RFC 6638), no compression tier, Basic htpasswd auth only — all untested by design of the fixture.

### Nextcloud

- DAV strictly under `/remote.php/dav/`; principals at `principals/users/{uid}`; the addressbook home is asymmetric with the calendar home; the site root is not DAV-capable. Asserted by `test_dav_root_scoping`.
- LOCK round-trip, `SyncSession` initial + incremental sync, and the `calendar-timezone` `PROPPATCH` round-trip are asserted by e2e.
- Auth: Basic + app passwords (Bearer/OIDC out of fixture scope — note N1).
- The server provisions schedule-inbox/outbox collections after the first DAV access, but no e2e test asserts RFC 6638 behavior.

### Provider A

Opt-in, credential-free smoke tier (`--test e2e_provider_a_smoke -- --ignored`): never runs in CI, uses zero credentials, and skips itself when `PROVIDER_A_DAV_URL` is unset. Four unauthenticated probes: `OPTIONS /`, `/.well-known/caldav`, `/.well-known/carddav`, and an unauthenticated current-user-principal `PROPFIND` — asserting the `401` + `WWW-Authenticate: Basic` challenge and that the principal never leaks, while recording the well-known shape (redirect vs direct 401).

One real-world deployment (**Provider A**) has a CardDAV write-path quirk:
vCard `PUT`s whose body contains multi-byte UTF-8 sequences (e.g. non-ASCII
names in `FN`/`N`) can come back **double-encoded** on read — the stored
bytes are a second, redundant percent/UTF-8 encoding of the original, so a
later `GET` returns corrupted text.

Until that is fixed server-side, treat CardDAV writes on that service as
**unverified until read back**: after a successful `PUT`, issue a `GET` (or
`addressbook-multiget` fetch) and compare the returned bytes against what you
sent — or normalize both sides to Unicode NFC before comparing — before
marking the contact as settled in your local store. The same read-back
pattern is a cheap safety net for any provider you do not fully control.
