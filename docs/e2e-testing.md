# End-to-End Testing

The project ships three Docker e2e fixtures (SabreDAV, Radicale, Nextcloud)
plus an opt-in, credential-free smoke tier against a real-world deployment
(referred to as **Provider A** — never named in the repository). The suites
live in one tree per fixture under `tests/e2e/` (`sabredav/`, `radicale/`,
`nextcloud/`, `provider_a/`), with shared fixture helpers in
`tests/e2e/util.rs`.

| Tier | Fixture dir | Test target | URL (env override) | Credentials |
|------|-------------|-------------|--------------------|-------------|
| SabreDAV | `sabredav-test/` | `--test e2e_tests` | http://localhost:8080 (`SABREDAV_URL`) | `test` / `test` |
| Radicale | `radicale-test/` | `--test e2e_radicale` | http://localhost:8081 (`RADICALE_URL`) | `test` / `test` |
| Nextcloud | `nextcloud-test/` | `--test e2e_nextcloud` | http://localhost:8083 (`NEXTCLOUD_URL`) | `test` / `fixture-dav-password` |
| Provider A smoke | — (no fixture) | `--test e2e_provider_a_smoke -- --ignored` | `PROVIDER_A_DAV_URL` (required) | none |

CI runs the SabreDAV, Radicale, and Nextcloud tiers (jobs `e2e-tests`,
`e2e-radicale`, `e2e-nextcloud`); the Provider A smoke tier is `#[ignore]`-gated,
never runs in CI, uses zero credentials, and skips itself when
`PROVIDER_A_DAV_URL` is unset:

```bash
PROVIDER_A_DAV_URL=https://dav.example.test \
  cargo test --test e2e_provider_a_smoke -- --ignored --nocapture
```

It probes only the unauthenticated surface (`OPTIONS /`, the two well-known
URIs, an unauthenticated principal PROPFIND — 4 requests) and asserts the
401 + `WWW-Authenticate: Basic` challenge while recording the well-known
shape (redirect vs direct 401).

### SabreDAV (primary fixture)

This project includes a complete e2e testing environment with a SabreDAV server that supports CalDAV and CardDAV
features including compression, WebDAV locking (class 2), and WebDAV sync.

### Prerequisites

1. Docker and Docker Compose
2. The SabreDAV test environment (located in `sabredav-test/`)

### Setting up the test environment

```bash
cd sabredav-test
./setup.sh
```

This will start a complete SabreDAV environment with:

- Nginx with gzip, Brotli, and zstd compression modules
- PHP-FPM for better performance
- MySQL database with preconfigured SabreDAV tables
- Test user (test/test) and sample calendar events

### Running e2e tests

```bash
./run-e2e-tests.sh
```

Or manually:

```bash
cargo test --test e2e_tests -- --nocapture
```

### Resetting the test environment

To reset the database to a clean state:

```bash
cd sabredav-test
./reset-db.sh
```

### Radicale fixture

```bash
./radicale-test/setup.sh   # http://localhost:8081, user test/test
./radicale-test/reset.sh   # restart wipes the tmpfs data, re-seeds
cargo test --test e2e_radicale
```

Radicale is the second engine in the matrix and exercises different failure
modes: sync-token invalidation (`403` + `valid-sync-token`), no LOCK support
(405 despite an advertised class 2), and the auto-create-on-first-principal-
access quirk. See `radicale-test/README.md` for the full quirk list.

### Nextcloud fixture

```bash
./nextcloud-test/setup.sh   # http://localhost:8083, first boot installs the instance
./nextcloud-test/reset.sh   # full wipe + reinstall (slow)
cargo test --test e2e_nextcloud
```

Nextcloud is the real-world reference: DAV strictly under `/remote.php/dav/`,
`principals/users/{uid}` paths, VTODO coverage, and Basic auth (with app
passwords as the documented path for hardened instances; Bearer/OIDC is out
of scope for the fixture). See `nextcloud-test/README.md`.

### Provider quirks: UTF-8 double-encoding on CardDAV writes

Provider A's CardDAV write path can double-encode multi-byte UTF-8 in vCard
writes (corrupted text on later reads). The full quirk note and the
read-back workaround live in [`compatibility.md`](compatibility.md#provider-a).
