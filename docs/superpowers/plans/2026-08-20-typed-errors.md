# Typed Errors with thiserror — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge main into the `feat/typed-thiserrors` PR branch, extend the typed-errors migration to cover the client builder, add a `Tls` error variant, and remove `anyhow` entirely.

**Architecture:** PR #69 introduced a `#[non_exhaustive]` `Error` enum via `thiserror`, replacing `anyhow` across 12 source files. Main has since added a client builder (`src/webdav/builder.rs`, ~860 lines) with 13+ `anyhow!` calls that need migration, plus an etag normalization fix that conflicts with the PR's changes in `webdav/client.rs`. We merge main into the branch, resolve conflicts, then extend the builder and add the `Tls` variant.

**Tech Stack:** Rust, thiserror 2, hyper, rustls, quick-xml, tokio, hyper-util

## Global Constraints

- `thiserror = "2"` is the only error-handling dependency (no `anyhow` anywhere)
- `Error` enum is `#[non_exhaustive]` — always include wildcard arm in matches
- All public APIs return `crate::Result<T>` (alias for `Result<T, Error>`)
- `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must pass after every task
- Branch: `feat/typed-thiserrors` (PR #69), merging `main` into it
- Current working branch: `Goopil/typed-errors` (tracks main, at v0.7.2)

---

### Task 1: Merge main into the PR branch

(Done — merge completed with conflicts resolved)

### Task 2: Add the `Tls` variant to `src/error.rs`

Add `Error::Tls { context, source }` variant and `Error::tls()` helper. Add tests.

### Task 3: Migrate `src/webdav/builder.rs` to typed errors

Replace all `anyhow!()` in builder with typed `Error` variants. Update macro.

### Task 4: Migrate `src/caldav/builder.rs` and `src/carddav/builder.rs` doc examples

Replace `anyhow::Error` with `fast_dav_rs::Error` in doc examples.

### Task 5: Remove anyhow from Cargo.toml and verify complete removal

Grep for any remaining `anyhow` references. Remove from Cargo.toml.

### Task 6: Update documentation (README.md, AGENTS.md)

Add Tls variant to README error table. Update AGENTS.md error handling.

### Task 7: Final verification and cleanup

Full build, test, clippy, fmt, grep verification.
