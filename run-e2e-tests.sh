#!/usr/bin/env bash
set -euo pipefail

# Script to run E2E tests against the SabreDAV server

echo "Starting SabreDAV test environment..."
(cd sabredav-test && ./setup.sh)

echo "Waiting for services to be ready..."
for attempt in {1..30}; do
    if curl --fail --silent --show-error --output /dev/null http://localhost:8080; then
        break
    fi

    if [[ "$attempt" -eq 30 ]]; then
        echo "SabreDAV did not become reachable at http://localhost:8080" >&2
        exit 1
    fi

    sleep 1
done

echo "Running E2E tests..."
cargo test --manifest-path Cargo.toml --test e2e_tests

echo "Test run completed!"
