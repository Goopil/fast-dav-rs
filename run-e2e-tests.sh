#!/bin/bash

# Script to run E2E tests against the SabreDAV server

echo "Starting SabreDAV test environment..."
cd sabredav-test
./setup.sh
cd ..

echo "Waiting for services to be ready..."
sleep 5

echo "Running E2E tests..."
cargo test --manifest-path Cargo.toml --test '*' || true

echo "Test run completed!"