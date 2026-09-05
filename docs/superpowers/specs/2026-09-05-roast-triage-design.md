# External roast triage (2026-09) — design

## Goal

Process the external adversarial review of fast-dav-rs received 2026-09 ("roast"):
keep the credible feedback, integrate accepted items into the remediation roadmap,
and track each accepted item as a GitHub issue.

## Decisions (validated with maintainer, 2026-09-05)

- **Accept (6):**
  1. Criterion benchmarks — the 3 scenarios specified in `docs/audit/PERFORMANCE.md §5`
  2. `cargo-semver-checks` gate on PRs
  3. Provider compatibility matrix (feature × fixture, derived from e2e suites)
  4. Fuzzing — cargo-fuzz, 3 targets (multistatus/REPORT XML, sync response, etag/URIs)
  5. README split — deep-dives to `docs/`, short README, Codecov badge
  6. Opt-in `require_https(true)` builder flag (non-breaking; shared validation point with #139/R19)
- **Reject:** mutation testing (cargo-mutants) — cost/triage outweighs benefit at this stage; revisit near 1.0.
- **Already tracked (no new work):** coverage carve-out (deliberate, documented), AUDIT-031 (LGPL), AUDIT-011 (http+credentials loudness), R08/#137 (Retry-After hang), R19/#139 (https→http downgrade).

## Deliverables

- `docs/audit/ROAST-2026-09-EXTERNAL.md` — claim-by-claim triage (English)
- `docs/audit/REMEDIATION_PLAN.md` — new "External review 2026-09 (roast triage)" section
- 6 GitHub issues (created before the triage doc so it references real issue numbers)

No code, CI, or `src/` changes in this delivery.

## Plan

`docs/superpowers/plans/2026-09-05-roast-triage.md`
