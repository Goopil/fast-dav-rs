# Radicale Test Environment

Docker fixture running Radicale 3.7.6 (community image `tomsquest/docker-radicale`,
pinned — no official Radicale image exists on Docker Hub) with Basic auth and
ephemeral (tmpfs) storage, for the `e2e_radicale` integration tests.

## Setup

```bash
./setup.sh        # idempotent: starts the container, waits, seeds test data
```

- Server: http://localhost:8081
- Credentials: `test` / `test` (htpasswd, plain encryption — fixture only)
- Seeded by `seed.sh`:
  - calendar `/test/fixture-calendar/` with two VEVENTs
  - address book `/test/fixture-addressbook/` with one VCARD

## Reset

```bash
./reset.sh
```

Data lives on a tmpfs mount, so restarting the container wipes it; the script
re-seeds from scratch. **Note:** a reset also invalidates every sync token
issued before it (see quirks below).

## Run the tests

```bash
cargo test --test e2e_radicale
# Override the endpoint if needed:
RADICALE_URL=http://localhost:8081 cargo test --test e2e_radicale
```

## Observed quirks (Radicale 3.7.6)

These surfaces are exercised by the e2e tests — the recording tests print the
observed status/body and assert only the loose shape:

- **Sync-token invalidation**: a `sync-collection` REPORT with an unknown or
  stale token (e.g. issued before a data wipe) is answered with
  `403 Forbidden` + `<D:error><valid-sync-token/></D:error>` (RFC 6578 §3.2
  stale signal). `WebDavClient::sync_collection_resilient` transparently
  falls back to an initial sync on this shape.
- **No LOCK**: `OPTIONS /` advertises `DAV: 1, 2, 3`, but a `LOCK` request is
  answered `405 Method Not Allowed`. Clients must not rely on locking even
  when the compliance class is advertised.
- **Auto-create on first principal access**: on empty storage, the first
  authenticated `PROPFIND` of `/{user}/` succeeds (207) and creates the
  principal collection tree — no `MKCOL` needed. Arbitrary nonexistent paths
  are NOT auto-created (plain `404`).
- **Well-known**: `/.well-known/caldav` and `/.well-known/carddav` answer
  `301` to `/` (not to the principal path).
- **Discovery**: `current-user-principal`, `calendar-home-set` and
  `addressbook-home-set` all resolve to `/{user}/` (principal == home set),
  both on the root PROPFIND and on the principal PROPFIND.

## Files

- `docker-compose.yml` — Radicale service (port 8081 → 5232, tmpfs data)
- `config/config` — Radicale config (Basic auth, `owner_only` rights)
- `config/users` — htpasswd file (`test:test`)
- `seed.sh` — shared wait + seed logic (sourced by the other scripts)
- `setup.sh` / `reset.sh` — entry points
