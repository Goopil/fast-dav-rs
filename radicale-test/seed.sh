#!/usr/bin/env bash
# Shared wait + seed logic for the Radicale fixture. Sourced by setup.sh / reset.sh.
set -euo pipefail

RADICALE_URL="${RADICALE_URL:-http://localhost:8081}"
RADICALE_USER="${RADICALE_USER:-test}"
RADICALE_PASS="${RADICALE_PASS:-test}"

compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  else
    docker-compose "$@"
  fi
}

radicale_wait() {
  echo "Waiting for Radicale at ${RADICALE_URL} ..."
  for _ in $(seq 1 60); do
    # Any HTTP response (401 included) proves the server is up.
    local code
    code="$(curl -s -o /dev/null -w '%{http_code}' "${RADICALE_URL}/" || true)"
    if [ -n "$code" ] && [ "$code" != "000" ]; then
      echo "Radicale is up (HTTP ${code})."
      return 0
    fi
    sleep 1
  done
  echo "ERROR: Radicale did not become ready in time" >&2
  compose logs radicale >&2 || true
  exit 1
}

# HTTP status of a request; prints the status code.
radicale_status() {
  curl -s -o /dev/null -w '%{http_code}' "$@"
}

radicale_seed() {
  local auth=(-u "${RADICALE_USER}:${RADICALE_PASS}")
  local principal_url="${RADICALE_URL}/${RADICALE_USER}/"
  local cal_url="${principal_url}fixture-calendar/"
  local abook_url="${principal_url}fixture-addressbook/"

  echo "Probing principal (also exercises Radicale auto-create on first access)..."
  local code
  code="$(radicale_status "${auth[@]}" -X PROPFIND -H 'Depth: 0' -H 'Content-Type: application/xml' \
    --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>' \
    "${principal_url}")"
  if [ "$code" != "207" ] && [ "$code" != "200" ]; then
    echo "ERROR: authenticated PROPFIND on principal returned HTTP ${code}" >&2
    exit 1
  fi

  echo "Seeding calendar ${cal_url} ..."
  code="$(radicale_status "${auth[@]}" -X MKCALENDAR -H 'Content-Type: application/xml' \
    --data '<?xml version="1.0" encoding="utf-8"?><C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:set><D:prop><D:displayname>Fixture Calendar</D:displayname></D:prop></D:set></C:mkcalendar>' \
    "${cal_url}")"
  # 201 created, 405 already exists (idempotent re-run). Radicale may also
  # answer 200/204 depending on version.
  if ! [[ "$code" =~ ^(2|405) ]]; then
    echo "ERROR: MKCALENDAR returned HTTP ${code}" >&2
    exit 1
  fi

  local i uid
  for i in 1 2; do
    uid="fixture-event-${i}@example.com"
    echo "Seeding event ${uid} ..."
    code="$(radicale_status "${auth[@]}" -X PUT -H 'Content-Type: text/calendar; charset=utf-8' \
      --data-binary "BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//Fixture//EN
BEGIN:VEVENT
UID:${uid}
DTSTAMP:20260101T00000${i}Z
DTSTART:2026010${i}T100000Z
DTEND:2026010${i}T110000Z
SUMMARY:Fixture event ${i}
END:VEVENT
END:VCALENDAR" \
      "${cal_url}${uid}.ics")"
    if ! [[ "$code" =~ ^2 ]]; then
      echo "ERROR: event PUT returned HTTP ${code}" >&2
      exit 1
    fi
  done

  echo "Seeding address book ${abook_url} ..."
  code="$(radicale_status "${auth[@]}" -X MKCOL -H 'Content-Type: application/xml' \
    --data '<?xml version="1.0" encoding="utf-8"?><D:mkcol xmlns:D="DAV:" xmlns:CR="urn:ietf:params:xml:ns:carddav"><D:set><D:prop><D:resourcetype><D:collection/><CR:addressbook/></D:resourcetype><D:displayname>Fixture Address Book</D:displayname></D:prop></D:set></D:mkcol>' \
    "${abook_url}")"
  if ! [[ "$code" =~ ^(2|405) ]]; then
    echo "ERROR: MKCOL (address book) returned HTTP ${code}" >&2
    exit 1
  fi

  code="$(radicale_status "${auth[@]}" -X PUT -H 'Content-Type: text/vcard; charset=utf-8' \
    --data-binary 'BEGIN:VCARD
VERSION:4.0
UID:fixture-contact@example.com
FN:Fixture Contact
N:Contact;Fixture;;;
END:VCARD' \
    "${abook_url}fixture-contact.vcf")"
  if ! [[ "$code" =~ ^2 ]]; then
    echo "ERROR: vCard PUT returned HTTP ${code}" >&2
    exit 1
  fi

  echo "Radicale seed complete."
}
