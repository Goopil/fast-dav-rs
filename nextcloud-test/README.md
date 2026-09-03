# Nextcloud Test Environment

Docker fixture running `nextcloud:stable-apache` with SQLite (the lightest
possible setup for DAV interop testing) and a CLI-created test user, for the
`e2e_nextcloud` integration tests.

## Setup

```bash
./setup.sh        # idempotent; the first boot installs the instance (a few minutes)
```

- Server: http://localhost:8083
- DAV root: http://localhost:8083/remote.php/dav/
- Credentials: `test` / `fixture-dav-password`
  (Nextcloud's password policy rejects trivial passwords even via `occ`, so
  the fixture password is not the `test`/`test` pair used by the other fixtures)
- Admin account: `admin` / `admin-password` (fixture only)
- The setup script also mints an app password via the OCS API and saves it to
  `.app-password` (git-ignored).

## Auth: Basic + app password (Bearer/OIDC out of scope)

Nextcloud's documented DAV auth paths are Basic auth and app passwords.
OpenID Connect / Bearer tokens require an OIDC app and are **explicitly out
of scope for this fixture**:

- The tests use Basic auth with the account password (the instance has no
  2FA, so Basic is accepted).
- For hardened instances with 2FA enabled, Basic auth only works with an
  **app password** (personal settings → Security, or the OCS
  `core/getapppassword` endpoint used by `setup.sh`): pass the app password
  as the Basic password, username unchanged.

## Reset

```bash
./reset.sh
```

Full wipe (`docker compose down -v`) followed by a fresh install — expect a
few minutes on the next boot. Data persists across plain `docker compose
down`/`up` via the `nc_data` volume, which keeps routine restarts fast.

## Run the tests

```bash
cargo test --test e2e_nextcloud
# Override the endpoint if needed:
NEXTCLOUD_URL=http://localhost:8083 cargo test --test e2e_nextcloud
```

## Observed quirks (Nextcloud `stable-apache`)

- **DAV path layout**: everything lives under `/remote.php/dav/`. The site
  root is not DAV-capable (`PROPFIND /` → `405`); the DAV root answers
  `current-user-principal` directly.
- **Principal paths contain a `users/` segment**:
  `principals/users/{uid}/` — and the **addressbook home is asymmetric** with
  the calendar home: `addressbooks/users/{uid}/` vs `calendars/{uid}/`.
- **`occ user:add` alone does not provision the DAV tree**: the principal
  collection answers `404` until the user's first login / first DAV access.
  The setup script warms it with a PROPFIND; the default calendar
  (`personal`), birthdays calendar, and scheduling inbox/outbox appear after
  the first calendar operation.
- **Password policy applies to `occ user:add`**: "compromised" passwords are
  rejected from the CLI too (the setup script uses a policy-safe password).
- Well-known URIs (`/.well-known/caldav|carddav`) answer `301` to
  `/remote.php/dav/`.

## Files

- `docker-compose.yml` — Nextcloud service (port 8083, SQLite, healthcheck)
- `setup.sh` / `reset.sh` — entry points (occ user creation, DAV warm-up,
  app-password minting)
- `.app-password` — minted at runtime, git-ignored, never committed
