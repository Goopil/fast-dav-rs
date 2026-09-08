# SabreDAV Test Environment with Nginx

Docker Compose fixture running SabreDAV with CalDAV + CardDAV, used by the
`e2e_tests` target. It is the primary fixture: a real class-2 server with
locking (PDO locks backend) and WebDAV Sync.

## Setup

```bash
./setup.sh
```

- Server: http://localhost:8080
- Credentials: `test` / `test`

## Seeded data

- Principal `principals/test` (digest auth user `test`)
- Calendar `calendars/test/default/` ("Default Calendar", `VEVENT,VTODO`) with
  3 seeded `VEVENT`s (`event1.ics` … `event3.ics`)
- Addressbook `addressbooks/test/default/` ("Default Addressbook") with
  2 seeded vCards

Seeded by `sql/init.sql` (schema + user + collections) and `sql/seed.sql`
(calendar objects + contacts).

## Running e2e tests

```bash
cargo test --test e2e_tests
# Override the endpoint if needed:
SABREDAV_URL=http://localhost:8080 cargo test --test e2e_tests
```

## Database Management

- Reset and reseed the database:
  ```bash
  ./reset-db.sh
  ```

## Observed quirks (SabreDAV fixture)

- **`PUT` responses do not always carry an `ETag` header** — read the ETag
  back with a `GET` (see `examples/locking_concurrent_edits.rs` for the
  pattern).
- **`calendar-timezone`**: supported on calendar creation (set at
  `MKCALENDAR` time); the `PROPPATCH` write path is untested on this fixture
  (see the compatibility table in the main `README.md`).

## Structure

- `config/` - Configuration files
- `data/` - SabreDAV application files
- `sql/` - Database initialization and seeding scripts
- `docker-compose.yml` - Docker Compose configuration
- `Dockerfile` - Custom SabreDAV Docker image with PHP-FPM
- `nginx/` - Nginx configuration and custom build with compression modules

## Requirements

- Docker Engine 20.10+
- Docker Compose V2 (included with Docker Desktop, or install separately)

Note: This setup uses the modern `docker compose` command (V2) rather than the legacy `docker-compose` command.

## Features

- Nginx with gzip, Brotli, and zstd compression modules
- PHP-FPM for better performance
- MySQL database with preconfigured SabreDAV tables
- Test user, calendar (with events), and addressbook (with contacts) pre-created
- Reset script for clean testing environment
