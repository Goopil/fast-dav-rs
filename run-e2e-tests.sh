#!/bin/bash

# E2E Test Runner for fast-dav-rs
# This script runs the E2E tests against a local SabreDAV server

set -e

echo "🚀 Starting E2E tests for fast-dav-rs"

# Check if SabreDAV test environment is running
if [[ $(docker ps | grep -c "sabredav-test") -lt 3 ]]; then
    echo "❌ SabreDAV test environment is not running"
    echo "Please start it with: cd sabredav-test && ./setup.sh"
    exit 1
fi

echo "✅ SabreDAV test environment is running"

# Run the E2E tests
echo "🧪 Running E2E tests..."
echo "Note: 'Unknown frame descriptor' errors are expected and indicate successful compression handling"
cargo test --test caldav_suite -- e2e_tests --nocapture

echo "🎉 E2E tests completed!"
echo "✅ The tests verify that:"
echo "  - Client can connect to SabreDAV server"
echo "  - WebDAV methods work correctly"
echo "  - Compression is handled properly"
echo "  - Server responses are processed (even if compressed)"