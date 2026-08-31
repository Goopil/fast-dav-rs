# Operations Review — fast-dav-rs 0.9.0

**Audit date:** 2026-08-31 · Companion to `FINDINGS.md`.

## 1. The 3 a.m. question

*"Si ce logiciel gérait demain 10× plus d'utilisateurs et qu'une panne survenait à 3 h du matin, qu'est-ce qui me ferait le plus peur ?"*

1. **The library cannot speak.** Zero `log`/`tracing` hooks (AUDIT-010). A wedged `send()` (AUDIT-002) or a silently disabled compression path (AUDIT-012) produces no log line, no metric, nothing. Embedders see "my worker is stuck" and must introspect a third-party crate to learn why.
2. **Timeout contract is a lie at the body phase** (AUDIT-002): `Error::Timeout` documentation promises coverage the code does not deliver — operators trust a documented timeout that does not exist for 50% of the request lifetime.
3. **The release gate has a hole** (AUDIT-005): any branch can be published to crates.io by dispatch. At 3 a.m. the question "what exactly did we ship?" becomes unanswerable if someone dispatched from a non-tag ref.

## 2. Observability inventory

| Mechanism | Status |
|---|---|
| Structured errors | Good — typed `Error`/`Operation`, `#[non_exhaustive]`, source chains preserved |
| Logging/tracing hooks | **None** (AUDIT-010) |
| Metrics | None (acceptable for a lib; hooks are the prerequisite) |
| Health checks | N/A (library) — but capability probes silently misreport (AUDIT-013) |
| Correlation IDs | None; embedders must thread their own (fine, but document) |

## 3. CI/CD posture

**Solid:** fmt + clippy `-D warnings` + nextest `--all-features --locked` + examples + doc-tests (`ci.yml:40-58`); MSRV job on exact 1.85.0; `rustsec/audit-check`; concurrency cancel-in-progress; least-privilege permissions; e2e with log dump on failure and GHA-cached image builds.

**Gaps (AUDIT-005, AUDIT-019):**
- `publish.yml:18` — `workflow_dispatch` bypasses the tag check → publish from any branch; no version↔tag match; no protected environment.
- No `timeout-minutes` on any job (6 h default burn).
- Fork PRs fail coverage (Sonar token unavailable, `fail_ci_if_error: true`).
- No action SHA-pinning; no cargo-deny.
- `e2e-tests.yml`'s unit job is weaker than the main CI job.
- Versioning drift at HEAD: `Cargo.toml` = 0.9.0 with 4 post-tag commits and an empty `[Unreleased]` changelog (AUDIT-029 sibling) — a dispatch-publish today would collide with the released 0.9.0.

## 4. Release/recovery runbook gaps

- No downgrade/rollback story needed for a *library* consumers pin; but the publish gate + version drift (above) make "what shipped when" harder than necessary.
- `reset-db.sh` exits 0 on failure and uses compose-v1 spelling (AUDIT-018) — a broken e2e environment looks healthy.
- e2e readiness accepts 401 as "ready" (`e2e-tests.yml:89-94`) — the e2e target can be wrong and green.

## 5. Recommendations

1. Add the `tracing` feature (AUDIT-010): request start/finish with method+path+status+duration, probe outcomes, retries, decompressed sizes. This single change converts findings AUDIT-002/012/013 from "silent" to "observable".
2. Fix the publish gate (AUDIT-005): tag-only + version match + `environment:` protection.
3. Add `timeout-minutes` to all jobs; make fork-PR coverage tokenless/`continue-on-error`.
4. Bump version + changelog at HEAD before the next release cycle (the `[Unreleased]` section is empty despite two functional commits).
5. Adopt the rule from AUDIT-009: committed fix plans are either executed or closed with a decision note — otherwise the repo accumulates ghosts of past audits.
