# External Roast Triage — 2026-09 (against 0.14.0 / main @ 1fa18f7)

Third external review pass over the project: an unsolicited adversarial review
("roast") received 2026-09, scoring the project 7/10 as a technical library and
4/10 on adoption. Every claim was cross-checked against the repository state at
0.14.0 before disposition. Complements `ROAST-2026-09.md` (internal roast,
R01–R37) and `FINDINGS.md` (AUDIT-001..032).

## Disposition summary

6 accepted (tracked as issues), 5 already tracked, 1 rejected, plus
non-actionable commentary (scores, adoption notes).

## Accepted → issues

| Claim | Verdict | Issue | Scope |
|---|---|---|---|
| Zero benchmarks | TRUE — and already specified in PERFORMANCE.md §5, never implemented | #195 | 3 specified scenarios, criterion, in-process fixture, baselines published |
| No semver-checks in CI | TRUE | #196 | PR gate vs last release tag + CONTRIBUTING note + 1.0 intent in roadmap |
| No compatibility matrix | TRUE (roadmap had only a vague line) | #197 | feature × fixture grid derived from existing e2e suites; ✅ only where a test asserts it |
| No fuzzing | TRUE — new surface analysis | #198 | cargo-fuzz, 3 targets (multistatus/REPORT XML, sync response, etag/URI helpers), seeded corpus, scheduled CI |
| README monolith (1446 lines) | TRUE | #199 | deep-dives → docs/, short README, TOC/anchors preserved, Codecov badge |
| No HTTPS enforcement option | PARTIAL — AUDIT-011 + R19 (#139) cover adjacent behavior; an opt-in builder flag is new | #200 | `require_https(true)`, rejects http:// base URL + cross-scheme redirects at the shared validation point; coordinated with #139; non-breaking |

## Already tracked (no new work)

| Claim | Where |
|---|---|
| Coverage measured on unit tests only | Deliberate, documented carve-out (REMEDIATION_PLAN "Do not fix"); e2e behavior gaps are fixed with tests, not gate changes |
| LGPL-3.0 adoption friction | AUDIT-031 — maintainer decision, open |
| Non-idempotent retry risk | RELIABILITY.md failure-mode matrix; R08 Retry-After hang (#137) |
| http:// + credentials risk | AUDIT-011 (loudness) — Phase 1 |
| https→http downgrade / http discovery URLs | R19 (#139) |

## Rejected

| Claim | Rationale |
|---|---|
| Mutation testing (cargo-mutants) | Coverage ≥80% + Sonar + real-server e2e already in place; runtime cost and survivor triage outweigh the benefit at this stage. Revisit near 1.0. |

## Not adopted as metrics

The review's 1–10 scores are the reviewer's opinion, not project metrics; only
the actionable subset above is tracked. Factual corrections:

- "No benchmarks" — true, but the three scenarios were already specified
  (`PERFORMANCE.md` §5, Phase 2 of the remediation plan); the roast missed this.
- "Coverage proof unclear" — the coverage workflow intentionally targets unit
  tests (Codecov + SonarCloud wired in `coverage.yml`); the carve-out is
  documented, and the badge/proof ask is folded into the README-split issue.
- Provider-quirk documentation already exists (README "Provider quirks"); the
  compatibility matrix formalizes it per fixture.
