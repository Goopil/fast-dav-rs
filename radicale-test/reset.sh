#!/usr/bin/env bash
# Reset the Radicale fixture. Data lives on a tmpfs mount, so restarting the
# container wipes it; we then re-seed from scratch.
set -euo pipefail
cd "$(dirname "$0")"

source ./seed.sh
docker rm -f fast-dav-radicale >/dev/null 2>&1 || true
compose up -d
radicale_wait
radicale_seed

echo
echo "Radicale fixture reset complete."
