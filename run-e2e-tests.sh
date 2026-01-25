#!/usr/bin/env bash

# Script to run E2E tests against the SabreDAV server

echo "Starting SabreDAV test environment..."
cd sabredav-test || exit
./setup.sh
cd ..

echo "Waiting for services to be ready..."
sleep 5

echo "Running E2E tests..."
cargo test --manifest-path Cargo.toml --test e2e_tests

echo "Test run completed!"
