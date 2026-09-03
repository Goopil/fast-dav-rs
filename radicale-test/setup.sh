#!/usr/bin/env bash
# Bring up the Radicale fixture (idempotent): start the container, wait for it,
# and seed the test collections. Safe to re-run.
set -euo pipefail
cd "$(dirname "$0")"

source ./seed.sh
compose up -d
radicale_wait
radicale_seed

echo
echo "Radicale fixture ready at ${RADICALE_URL} (user ${RADICALE_USER})"
echo "Run the e2e tests: cargo test --test e2e_radicale"
echo "Reset the fixture: ./reset.sh"
