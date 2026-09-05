# External Roast Triage (2026-09) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate credible feedback from the external roast into the roadmap and create 6 GitHub issues. Docs only.

**Architecture:** 4 markdown files + 6 `gh` issues. Issues are created **before** the triage doc so the doc references real issue numbers (zero placeholders).

**Tech Stack:** markdown, `gh` CLI, conventional commits.

## Global Constraints

- All repository and GitHub documents in **English** (repo rule from user CLAUDE.md).
- Conventional commit style matching `git log` (`docs: ...`).
- No `src/` or `.github/` modifications in this delivery.
- No push, no PR (not requested).
- Verify `gh auth status` before creating issues.
- Exact contents for every file and issue body are in the tasks below — use verbatim, substituting only real issue numbers where marked.

---

### Task 1: Spec + plan trail

**Files:**
- Create: `docs/superpowers/specs/2026-09-05-roast-triage-design.md`
- Create: `docs/superpowers/plans/2026-09-05-roast-triage.md` (this file)

- [x] **Step 1: Write the spec** (content: decisions validated with maintainer, deliverables, 6 accepted items, rejections, already-tracked list)
- [x] **Step 2: Write this plan file**
- [x] **Step 3: Commit** `docs: roast triage (2026-09) — spec + plan`

### Task 2: Create the 6 GitHub issues (before the triage doc, to capture real numbers)

**Files:** none in repo; 6 issues on GitHub.

**Interfaces:**
- Produces: 6 issue numbers (e.g. #NN1..#NN6) — Task 3 substitutes them into the triage doc and roadmap section.

- [ ] **Step 1: Preflight** — `gh auth status` must pass; `gh issue list --state all --limit 3` to confirm numbering space (last known: #136–#142 internal roast, #194 merge).
- [ ] **Step 2: Create issues** with `gh issue create --title ... --body-file ...` (bodies below, English, verbatim):

**Issue A — `bench: add criterion benchmarks (PERFORMANCE.md §5)`**

```markdown
External review triage (2026-09): the repo has **zero benchmarks**. `docs/audit/PERFORMANCE.md §5` already specifies three scenarios; none is implemented (no `[[bench]]` target, no criterion dependency).

## Scope

- `benches/` with criterion; hyper **in-process** fixture (no network, no Docker)
- **B1** — `sync_collection` over 1k/10k synthetic items, `include_data` on/off
- **B2** — first-request latency in `Auto` compression mode, 32 concurrent callers
- **B3** — aggregated parse vs `parse_multistatus_stream_visit` throughput on a ~50 MB multistatus
- Baseline numbers recorded in `docs/audit/PERFORMANCE.md`
- No CI regression gate initially (criterion on per-PR CI is noisy); revisit after AUDIT-003/016 land

## Refs

- `docs/audit/PERFORMANCE.md` §5 (scenario definitions)
- `docs/audit/REMEDIATION_PLAN.md` Phase 2 (benchmarks were already slotted there)
- External triage: `docs/audit/ROAST-2026-09-EXTERNAL.md`
```

**Issue B — `ci: cargo-semver-checks gate on PRs`**

```markdown
External review triage (2026-09): the crate is at 0.14.0 with a 0.x API-churn history (e.g. the legacy module-path migration). `cargo semver-checks` was used ad-hoc once in a plan doc but is not part of CI.

## Scope

- Workflow job on PRs: `cargo semver-checks` against the last release tag (baseline); fail on breaking changes
- `CONTRIBUTING.md` note: what the gate covers, how to land intentional breaking changes in a 0.x minor window
- Roadmap statement of 1.0 intent (stability policy)

## Refs

- `.github/workflows/ci.yml` (job placement)
- External triage: `docs/audit/ROAST-2026-09-EXTERNAL.md`
```

**Issue C — `docs: provider compatibility matrix`**

```markdown
External review triage (2026-09): users ask "does it work with my server?", not "is RFC 6578 implemented?". Four fixtures exist (SabreDAV, Radicale, Nextcloud, Provider A smoke) but compatibility is only described in prose.

## Scope

- Feature × fixture grid: discovery (RFC 6764), WebDAV sync (RFC 6578), LOCK, scheduling (RFC 6638), calendar timezone, compression, OAuth — for SabreDAV / Radicale / Nextcloud / Provider A
- ✅ only where an e2e test asserts the behavior; explicit "not tested" otherwise — no aspirational claims
- Published in the README Testing section + `docs/`; new providers are added only with a fixture

## Refs

- `tests/e2e/` (source of truth per fixture)
- README "Provider quirks" section (existing prose to fold in)
- External triage: `docs/audit/ROAST-2026-09-EXTERNAL.md`
```

**Issue D — `fuzz: cargo-fuzz targets for parser surface`**

```markdown
External review triage (2026-09): the library parses untrusted XML/HTTP (multistatus, REPORT responses, etags, sync tokens, headers, redirects). No fuzz targets exist anywhere in the repo.

## Scope

- `fuzz/` (cargo-fuzz, nightly-only) with three targets:
  1. multistatus/propfind/REPORT XML parsing (incl. `parse_multistatus_stream`)
  2. sync-collection response parsing (sync tokens, 410/507 paths)
  3. etag/`If` header grammar + URI helpers (`build_uri`, `resolve_location`, `encode_path_segments`)
- Corpus seeded from fixture responses (`tests/e2e/`, fixture setup scripts)
- Scheduled (nightly/weekly) CI run — not per-PR
- Any crash → regression unit test + entry in `docs/audit/FINDINGS.md`

## Refs

- `docs/audit/ROAST-2026-09-EXTERNAL.md` (triage decision)
- Internal roast R17–R24 (URI handling findings — good fuzz oracle material)
```

**Issue E — `docs: split README into docs/`**

```markdown
External review triage (2026-09): the README is 1446 lines with 40+ sections; a reader who wants `CalDavClient::new(...)` faces a wall.

## Scope

- Move deep-dives to `docs/*.md`: Error Handling & Migration, Advanced Configuration, Streaming & Sync details, E2E fixture details
- README keeps: overview, quick start, feature summary, links into `docs/`; TOC + anchors preserved (no dead links)
- Add Codecov badge (external review asked for coverage proof; Codecov/SonarCloud are already wired in `coverage.yml`)
- Respect the AGENTS.md rule: keep README/AGENTS/examples in sync

## Refs

- `README.md` (current structure)
- `AGENTS.md` documentation-sync rule
- External triage: `docs/audit/ROAST-2026-09-EXTERNAL.md`
```

**Issue F — `feat(webdav): opt-in require_https guard`**

```markdown
External review triage (2026-09), maintainer decision: **opt-in, non-breaking**.

## Scope

- Builder flag `require_https(true)` (default: current behavior):
  - reject `http://` base URL at client construction
  - reject cross-scheme redirects at the shared validation point
- Coordinate with #139 (R19: silent https→http downgrade following, discovery returning http:// URLs) — one guard at the shared point, no duplicate logic
- Unit tests: base-URL reject, redirect reject, default behavior unchanged
- README Security section documents the flag

## Refs

- `docs/audit/FINDINGS.md` AUDIT-011 (adjacent: loudness for http+credentials)
- Internal roast R19 (#139) — downgrade handling lives there
- External triage: `docs/audit/ROAST-2026-09-EXTERNAL.md`
```

- [ ] **Step 3: Report the 6 URLs/numbers** (needed by Task 3).

### Task 3: Triage doc + roadmap section

**Files:**
- Create: `docs/audit/ROAST-2026-09-EXTERNAL.md`
- Modify: `docs/audit/REMEDIATION_PLAN.md` (append new section after the phase sections, before "Quick wins")

**Interfaces:**
- Consumes: 6 real issue numbers from Task 2 — substitute every `#<NA>`..`#<NF>` below.

- [ ] **Step 1: Write `docs/audit/ROAST-2026-09-EXTERNAL.md`** (content verbatim, substitute issue numbers):

```markdown
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
| Zero benchmarks | TRUE — and already specified in PERFORMANCE.md §5, never implemented | #<NA> | 3 specified scenarios, criterion, in-process fixture, baselines published |
| No semver-checks in CI | TRUE | #<NB> | PR gate vs last release tag + CONTRIBUTING note + 1.0 intent in roadmap |
| No compatibility matrix | TRUE (roadmap had only a vague line) | #<NC> | feature × fixture grid derived from existing e2e suites; ✅ only where a test asserts it |
| No fuzzing | TRUE — new surface analysis | #<ND> | cargo-fuzz, 3 targets (multistatus/REPORT XML, sync response, etag/URI helpers), seeded corpus, scheduled CI |
| README monolith (1446 lines) | TRUE | #<NE> | deep-dives → docs/, short README, TOC/anchors preserved, Codecov badge |
| No HTTPS enforcement option | PARTIAL — AUDIT-011 + R19 (#139) cover adjacent behavior; an opt-in builder flag is new | #<NF> | `require_https(true)`, rejects http:// base URL + cross-scheme redirects at the shared validation point; coordinated with #139; non-breaking |

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
```

- [ ] **Step 2: Append to `docs/audit/REMEDIATION_PLAN.md`** (before "## Quick wins"):

```markdown
## External review 2026-09 (roast triage)

From the unsolicited external review (claim-by-claim triage in `ROAST-2026-09-EXTERNAL.md`). Accepted items, ordered by ROI:

- [ ] #<NA> — criterion benchmarks (`PERFORMANCE.md` §5) — unblocks Phase 2 evidence
- [ ] #<NB> — `cargo-semver-checks` PR gate
- [ ] #<NC> — provider compatibility matrix (docs)
- [ ] #<ND> — cargo-fuzz targets (parser surface)
- [ ] #<NE> — README split into `docs/`
- [ ] #<NF> — opt-in `require_https` guard (coordinate with #139)

Rejected: mutation testing (revisit near 1.0). Already tracked, no new work: coverage carve-out (deliberate), AUDIT-031 (LGPL), AUDIT-011, R08/#137, R19/#139.
```

- [ ] **Step 3: Commit** `docs: external roast triage (2026-09) — 6 accepted items, roadmap update`

### Task 4: Verification

- [ ] **Step 1:** `git status` clean; `git log --oneline -3` shows the two commits.
- [ ] **Step 2:** `gh issue list --limit 8` shows the 6 new issues; issue numbers match those referenced in `ROAST-2026-09-EXTERNAL.md` and `REMEDIATION_PLAN.md`.
- [ ] **Step 3:** No `src/` or `.github/` changes: `git diff --name-only 1fa18f7..HEAD` lists only the 4 doc files.
- [ ] **Step 4:** Report final links (6 issue URLs + file paths).
