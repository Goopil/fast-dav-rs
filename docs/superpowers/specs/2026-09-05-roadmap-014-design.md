# Roadmap 0.14 — design (hardening + consolidation)

**Date**: 2026-09-05 · **Status**: approved (user) · **Supersedes**: n/a (consumes backlog #154 and the 0.13 deferred items)
**Context**: 0.13.0 released (crates.io, 2026-09-05); tracker #170 closed; backlog #154 open.

## Goals

Purge the accumulated LOW-severity backlog (#154) and the 0.13 deferreds in one
consolidation release, plus two small API additions that were blocked on fixtures or
engine availability (calendar-timezone write, batched CardDAV multiget). No new protocol
surface beyond those two. The crate stays pure-HTTP, provider-agnostic, caller-side
parsing.

## Decisions (user-approved)

| Decision | Choice |
|---|---|
| Scope | Full consolidation: all 11 #154 items + citation/marker nits + SabreDAV scheduling fixture **+** timezone write **+** CardDAV batched multiget with reconciliation ("le tout") |
| Multiget reconciliation | **Additive signal**: `BatchItem.missing_hrefs: Vec<String>` — requested hrefs absent from the server response. No per-chunk error (an omission must not destroy the 95% of data that returned). `BatchItem` is already `#[non_exhaustive]` |
| Timezone write API | `set_calendar_timezone(path, vtimezone: Option<&str>) -> Result<()>`: `Some` = set (body verbatim), `None` = `<remove>`. **No VTIMEZONE structural validation** — the crate never parses iCalendar (consistent with the verbatim read) |
| Breaking change | `SyncResponse` gains `#[non_exhaustive]` on CalDAV + CardDAV — breaks external struct literals; accepted at 0.x, flagged in CHANGELOG |
| Cycle shape | 5 packages (H1–H5) in 2 waves; release 0.14.0 at the end (user go required) |
| SabreDAV scheduling | Enable `Sabre\CalDAV\Schedule\Plugin` (plugin ships in the pinned `sabre/dav ^4.4`; `schedulingobjects` table already in `init.sql`) and pin `composer.lock` so the fixture stops floating |

## Work packages

### H1 — Multiget engine + CardDAV batching + reconciliation (wave 1, medium)

- Extract the chunking engine from `calendar_multiget_many` (`caldav/client.rs:555-623`)
  into shared `webdav/` machinery (duplication gate ≤3% — CardDAV must not copy-paste it).
- `addressbook_multiget_many` on `CardDavClient` mirroring the CalDAV semantics
  (chunking, `FuturesOrdered` ordering, partial-failure `BatchItem`s).
- `BatchItem.missing_hrefs: Vec<String>`: after each chunk's multistatus parse, compare
  requested hrefs against returned responses (exact href string match, documented) and
  surface the absent ones. Update the documented "Result shape and ordering" contract.
- Empty-href fix: empty hrefs are filtered out **before** chunking (today they are
  dropped from the request XML but still recorded in `BatchItem.hrefs`, violating the
  contract; an all-empty input produces no batches — document both).
- `build_calendar_multiget_body` (public, re-exported, undocumented) gets its doc
  comment.
- Tests: mock server omitting hrefs → `missing_hrefs` populated; empty-href chunks;
  CardDAV batching happy path + partial failure; ordering preserved.
- Acceptance: both clients share one engine; reconciliation documented + tested; no
  behavior change for compliant servers.

### H2 — Client correctness fixes (wave 2, small)

- `status_error` honors `WebDavError::parse_failed`: a 423 with a malformed `<D:error>`
  body becomes distinguishable from one with no body (today the
  `precondition_code.is_some()` gate at `webdav/client.rs:1539-1548` discards the flag
  set by `streaming.rs:1255-1264`). Callers affected: `lock_request`, `unlock`.
- Filter exclusivity validated pre-I/O in `calendar_query`, mirroring the existing
  prop-filter checks (`Error::InvalidInput`): comp-filter `is-not-defined` combined with
  `time_range`/`prop_filters` is rejected (today silently precedence-resolved at
  `caldav/types.rs:312-327`); param-filter `is-not-defined` combined with `text_match`
  is rejected (RFC 4791 §9.7.1/§9.7.3 DTD exclusivity) on CalDAV and CardDAV paths.
- Tests: malformed-423 through `status_error` (lock + unlock); comp-level and
  param-level exclusivity rejections; existing prop-filter rejection tests untouched.

### H3 — SabreDAV scheduling fixture (wave 1, small)

- Add `Sabre\CalDAV\Schedule\Plugin` in `sabredav-test/public/index.php` (one
  `addPlugin` line); commit `composer.lock` to pin the floating `composer install`.
- E2e against the real fixture: `discover_schedule_endpoints` (principal PROPFIND),
  `list_inbox`, `put_if_schedule_tag`/`delete_if_schedule_tag` round-trip.
- Fallback: if the plugin does not surface inbox/outbox for the fixture principals,
  record observed behavior and keep the scheduling e2e wire-only (Sonar coverage
  exemption documented in the PR) — same convention as S1 in 0.13.
- Acceptance: scheduling e2e green against SabreDAV, or documented fallback.

### H4 — Calendar-timezone write path (wave 2, small)

- `CalDavClient::set_calendar_timezone(path, vtimezone: Option<&str>) -> Result<()>`:
  typed PROPPATCH wrapper (`Depth: 0`) composing the `<C:calendar-timezone>` body on the
  existing raw `proppatch` (`webdav/client.rs:1477-1496`); `None` sends a `<remove>`.
  Errors: `Error::UnexpectedStatus` with `Operation::ProppatchCalendarTimezone` (new
  `Operation` variant).
- Unit tests wire-mock (success 207, propstat failure, empty `Some("")` rejected as
  `Error::InvalidInput` pre-I/O — a remove is `None`, not an empty set).
- Live verification against the Nextcloud fixture during implementation (Sabre-based;
  expected to accept the PROPPATCH); result documented in the README support table.
  Radicale e2e records observed behavior (expected unsupported) — records-observed
  convention from S2.
- README section + `no_run` doc example; read-path doc stops saying "write path is not
  exposed".
- Acceptance: write path round-trips on Nextcloud or is wire-only + documented.

### H5 — Docs/type polish batch (wave 1, small)

- `#[non_exhaustive]` on `SyncResponse` (`caldav/types.rs:86`, `carddav/types.rs:152`)
  + CHANGELOG breaking-change note.
- `TextMatch::to_caldav_xml()` / `ParamFilter::to_caldav_xml()` public (RFC 4791
  serialization without `match-type`); existing `to_xml()` stays, documented as the
  CardDAV flavor; CalDAV tests currently codifying the wrong serialization
  (`tests/unit/caldav/filter_tests.rs:48-113`) move to the CalDAV flavor.
- RFC 6352 §10.5.1 DTD misquote fixed (`carddav/types.rs:80`: `text-match*` not
  `text-match?`).
- RFC 4791 citation corrections (8 sites): `max-resource-size` §5.2.5,
  `supported-calendar-data` §5.2.4, `max-attendees-per-instance` §5.2.9.
- Stale `ponytail:` markers: remove the three "(0.10 window)" comments in
  `webdav/client.rs`; the version-free ones stay.
- README: document the base-URL userinfo rejection (`Error::InvalidConfig`,
  security-relevant builder behavior).
- Discovery redaction: strip userinfo from the discovered service URL before returning
  it (`webdav/discovery.rs:87` and the base-URL fallbacks) — speculative #154 item,
  resolved by sanitization + doc note.
- Acceptance: `rg` for the old citations/returns finds nothing; docs tests pass.

## Waves

- **Wave 1 (parallel)**: H1 ∥ H3 ∥ H5 — disjoint domains (multiget machinery /
  sabredav-test / types+docs).
- **Wave 2 (parallel)**: H2 ∥ H4 — both touch `caldav/client.rs` (different hunks) but
  start after wave 1 merges, keeping one conflict surface.
- Then final whole-branch review (diff `v0.13.0..main`), triage, release 0.14.0
  (user go required).

## Constraints & conventions

- Provider A is never named; no production URL in the repo (env-gated smoke tier).
- Pipeline per package: issue → worktree + subagent → review + spot-check → local gates
  (fmt, clippy `-D warnings`, nextest unit `--all-features --locked`, doctests,
  `cargo test --all-features --no-run` for compile-all, `CARGO_TARGET_DIR=target`) →
  PR → **CI 14/14 verified explicitly** → squash merge → CHANGELOG/README union on
  conflicts.
- SonarCloud gates mandatory: coverage ≥80% new code, duplication ≤3% (share via
  `webdav/`/`common/`, never copy-paste caldav↔carddav).
- Docs kept in sync (README, AGENTS.md, doc comments, CHANGELOG) — stale docs are bugs.
- CodeQL: no uids/tokens interpolated into log lines or assertion messages.
- Release only on explicit user go (publish.yml triggers on GitHub Release published).
- On completion: close #154 (all 11 items addressed or explicitly resolved).

## Risks

- SabreDAV scheduling may need principal-backend support beyond the plugin line
  (schedule-inbox-URL comes from the plugin's principal properties) — fallback is
  wire-only + documented; `composer.lock` pin removes version drift as a variable.
- H1 moves hot multiget machinery — regression risk mitigated by the existing
  chunking/ordering/partial-failure test battery; engine move is mechanical.
- Nextcloud timezone-write support is assumed (Sabre-based) but unverified until
  implementation — H4 has a documented wire-only fallback.
- Three packages historically touch `caldav/client.rs`; wave separation keeps at most
  one conflict surface per wave.
