#!/usr/bin/env bash

# Script to run unit tests

echo "Running unit tests..."
cargo test --manifest-path Cargo.toml --test unit_tests

echo "Unit test run completed!"
