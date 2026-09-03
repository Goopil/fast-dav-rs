#!/usr/bin/env bash
# Bring up the Nextcloud fixture (idempotent): start the container, wait for
# the install to finish, create the test user, warm the DAV stack, and mint
# an app password. Safe to re-run.
#
# Env overrides: NEXTCLOUD_URL, NEXTCLOUD_USER, NEXTCLOUD_PASS.
set -euo pipefail
cd "$(dirname "$0")"

NEXTCLOUD_URL="${NEXTCLOUD_URL:-http://localhost:8083}"
NEXTCLOUD_USER="${NEXTCLOUD_USER:-test}"
NEXTCLOUD_PASS="${NEXTCLOUD_PASS:-fixture-dav-password}"

compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  else
    docker-compose "$@"
  fi
}

compose up -d

echo "Waiting for Nextcloud at ${NEXTCLOUD_URL} (first boot installs the instance; this can take a few minutes)..."
ready=0
for _ in $(seq 1 90); do
  if curl -fsS -o /dev/null "${NEXTCLOUD_URL}/status.php" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 5
done
if [ "$ready" != "1" ]; then
  echo "ERROR: Nextcloud did not become ready in time" >&2
  compose logs nextcloud >&2 || true
  exit 1
fi
echo "Nextcloud is up."

# Create the test user (tolerates already-existing on re-runs).
if ! compose exec -T -u www-data nextcloud php occ user:info "${NEXTCLOUD_USER}" >/dev/null 2>&1; then
  echo "Creating user ${NEXTCLOUD_USER} via occ..."
  compose exec -T -u www-data nextcloud sh -c "OC_PASS='${NEXTCLOUD_PASS}' php occ user:add --password-from-env ${NEXTCLOUD_USER}"
fi

# Force the DAV stack to provision the user's principal/calendar/addressbook
# tree (Nextcloud provisions lazily on first DAV access).
code="$(curl -s -o /dev/null -w '%{http_code}' -u "${NEXTCLOUD_USER}:${NEXTCLOUD_PASS}" \
  -X PROPFIND -H 'Depth: 0' --data '<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>' \
  "${NEXTCLOUD_URL}/remote.php/dav/principals/users/${NEXTCLOUD_USER}/")"
if [ "$code" != "207" ] && [ "$code" != "200" ]; then
  echo "ERROR: authenticated DAV PROPFIND on the principal returned HTTP ${code}" >&2
  echo "(If it is 401: the fixture relies on Basic auth with the account password; see README.md)" >&2
  exit 1
fi

# Mint an app password via the OCS API (auth with the account password) and
# store it next to the fixture. App passwords are the documented Nextcloud
# DAV auth path for hardened instances; the tests use Basic auth with the
# account password by default (see README.md).
APP_PASSWORD="$(curl -s -u "${NEXTCLOUD_USER}:${NEXTCLOUD_PASS}" \
  -H 'OCS-APIRequest: true' -X GET \
  "${NEXTCLOUD_URL}/ocs/v2.php/core/getapppassword" \
  | grep -o '<apppassword>[^<]*' | sed 's/<apppassword>//' || true)"
if [ -n "${APP_PASSWORD}" ]; then
  printf '%s' "${APP_PASSWORD}" > .app-password
  echo "App password minted and saved to nextcloud-test/.app-password (git-ignored)."
else
  echo "WARNING: could not mint an app password via OCS (non-fatal; tests use the account password)." >&2
fi

echo
echo "Nextcloud fixture ready at ${NEXTCLOUD_URL} (user ${NEXTCLOUD_USER})"
echo "Run the e2e tests: cargo test --test e2e_nextcloud"
echo "Reset the fixture: ./reset.sh (full wipe; next boot re-installs)"
