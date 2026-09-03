#!/usr/bin/env bash
# Reset the Nextcloud fixture to a pristine state: full wipe of the data
# volume, then re-run setup (the next boot re-installs Nextcloud — expect a
# few minutes).
set -euo pipefail
cd "$(dirname "$0")"

compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  else
    docker-compose "$@"
  fi
}

compose down -v
exec ./setup.sh
