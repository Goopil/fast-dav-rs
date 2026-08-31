# fast-dav-rs Technical Audit — Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Full adversarial technical audit of fast-dav-rs 0.9.0, persisted as `docs/audit/*.md` with stable `AUDIT-xxx` IDs for future fix/reaudit cycles.

**Architecture:** Read-only audit of source/CI/tests; deliverables are 10 Markdown documents under `docs/audit/`. No source code changes.

**Tech Stack:** Rust (hyper + rustls + quick-xml), GitHub Actions, SonarCloud.

## Global Constraints

- Documents written in English (repo rule).
- No source code modifications — only `docs/audit/*` and this plan.
- Every finding: exact `file:line` location + honest confidence level (Confirmed / Likely / Needs Verification).
- Evidence First: no finding without evidence, impact, trigger, verification, remediation.

## Tasks

- [ ] T0. Save this plan (done by writing this file).
- [ ] T1. Dynamic checks: `cargo clippy --all-targets --all-features -- -D warnings`, `cargo nextest run --test unit_tests --all-features --locked`, `cargo audit` (if installed). Results feed TESTING.md/OPERATIONS.md.
- [ ] T2. Verify remaining High findings in source (compression bomb, auth attach, NoVerify, mkcalendar divergence, silent-swallow inventory, weak etag, dead env vars, root re-exports).
- [ ] T3. Second-pass red team: refute own conclusions; new angles (LGPL, Cargo.lock, README claims, docs/superpowers in public repo, clone fan-out).
- [ ] T4. Write docs/audit/: README, EXECUTIVE_SUMMARY, FINDINGS, SECURITY, PERFORMANCE, RELIABILITY, ARCHITECTURE, TESTING, OPERATIONS, REMEDIATION_PLAN. IDs assigned by decreasing severity. Include 13-domain scoring, risk matrix, prioritization (quick wins / structural / long-term / do-not-fix), final verdict with mandatory sections (Top 10 risks, Top 10 actions, Three things I would fix tomorrow, What I would NOT change).
- [ ] T5. Cross-check IDs referenced across all documents.

## Already-confirmed findings (spot-checked directly)

- `sync_collection` sends `Depth: 1`; RFC 6578 requires `Depth: 0` (src/caldav/client.rs:584, src/carddav/client.rs:558).
- Per-request timeout covers headers only; body read/decompress has no timeout (src/webdav/client.rs:592-609).
- No decompressed-size cap anywhere; all high-level APIs fully buffer (src/webdav/client.rs:609, src/common/compression.rs:194-238).
- Authorization header attached to any absolute URL without origin check (src/webdav/client.rs:263-267, 559-561).
- publish.yml:18 — `workflow_dispatch` short-circuits tag check; any branch can publish.
- Zero tracing/log hooks in src/.
