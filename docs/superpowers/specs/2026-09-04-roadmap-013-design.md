# Roadmap 0.13 — design

**Date**: 2026-09-04 · **Status**: approved (user) · **Supersedes**: the 0.13 sketch in
`2026-09-03-roadmap-012-013-design.md`
**Context**: 0.12.0 released (crates.io, 2026-09-04); tracker #157 closed; backlog #154 open.

## Goals

Extend the protocol surface beyond 0.12: scheduling (RFC 6638), managed attachments
(RFC 8607), timezones (RFC 7809), and typed privilege exposure — while keeping the crate
pure-HTTP, provider-agnostic, and caller-side-parsing as before.

## Decisions (user-approved)

| Decision | Choice |
|---|---|
| Scheduling gate | Built **unconditionally** (spec-conformant, SabreDAV-tested); the live Provider A verification (questionnaire) only decides whether an extra read-only smoke extension is added |
| Scheduling scope | **Full RFC 6638 surface**: probe + inbox/outbox discovery, POST outbox iTIP (REQUEST/REPLY/CANCEL + free-busy), inbox listing, `If-Schedule-Tag-Match` conditional writes |
| Cycle shape | 4 packages (S1–S4) in 2 waves; wave 2 packages are file-disjoint and parallelizable; release 0.13.0 at the end (user go required) |
| ACL | Still not implemented (unchanged from 0.12 decision); S4 is read-only typed privilege exposure |

## Work packages

### S1 — Scheduling RFC 6638 (wave 1)

All in `caldav/`, reusing the shared HTTP pipeline (`webdav/`); no iTIP parsing in the
crate.

- `discover_schedule_endpoints()` on `WebDavClient` + thin `CalDavClient` wrapper → one
  PROPFIND on the principal fetching `schedule-inbox-URL`, `schedule-outbox-URL`,
  `calendar-user-address-set` → `#[non_exhaustive]` `ScheduleEndpoints { inbox,
  outbox, user_addresses }`.
- `post_schedule(outbox, ical_body) -> SchedulingResponse { status, body }` — the raw
  response body comes back as-is; callers parse iTIP (`REQUEST-STATUS` included) with
  `icalendar`. Free-busy is the same POST with a `VFREEBUSY` body (200 + body).
- `list_inbox()` → PROPFIND Depth: 1 on the inbox URL + content via the existing
  multiget machinery → `Vec<InboxItem { href, etag, data }>` (thin).
- `put_if_schedule_tag(path, body, schedule_tag)` and `delete_if_schedule_tag(...)` →
  `If-Schedule-Tag-Match` header (RFC 6638 §10.1); `Schedule-Tag` is read from GET
  response headers (already surfaced). Stateless, no cached tag.
- `Operation` gains `PostSchedule`, `ScheduleInbox`, `ScheduleOutbox`,
  `ScheduleConditionalWrite`; errors stay `UnexpectedStatus`.

Tests: wire tests (mock server: outbox 200 + REQUEST-STATUS, plugin-off 403, inbox
PROPFIND, conditional PUT); SabreDAV e2e **if the fixture's scheduling plugin answers**
(verify at implementation; otherwise wire-only + documented, per the Sonar exemption
convention); smoke tier: extend the existing read-only Provider A smoke with
`discover_schedule_endpoints` once the live questionnaire confirms inbox/outbox function.

### S2 — Managed attachments RFC 8607 (wave 2, small)

- `post_managed_attachment(path, ics_uid, recurrence_id: Option<String>, body,
  content_type) -> ManagedAttachment { href, managed_id }` — POST to the
  `calendar-attachment-post` URI (probed once, cached like the compression probe).
- GET/DELETE of attachments reuse existing methods on the href; `MANAGEDIDS` prop added
  as an **optional** PROPFIND request prop surfaced as `managed_ids` on entries (no
  breaking change).
- `Operation::PostManagedAttachment`. Wire tests + Radicale e2e (supports 8607);
  SabreDAV depending on fixture version.

### S3 — Timezones RFC 7809 (wave 2, verify + docs + small API)

- `calendar-timezone` property (iCal body) readable via calendar PROPFIND
  (`calendar_timezone()`). Read-only until a fixture proves the write path
  (`ponytail:` no server in the fixtures round-trips a write; add
  mkcalendar/PROPPATCH support when one does).
- Minimum deliverable regardless of server support: **live support verification
  documented** (support table in README: Radicale no, SabreDAV partial, Nextcloud —)
  + pairing note with `icalendar` (parsing stays caller-side).

### S4 — Typed current-user-privilege-set (wave 2, read-only)

- `current_user_privileges(path) -> Vec<Privilege>` — PROPFIND
  `current-user-privilege-set` → `#[non_exhaustive]` enum `Privilege { Read, Write,
  WriteProperties, WriteContent, Bind, Unbind, Unlock, ReadFreeBusy,
  Other(String) }`. No ACL writes (RFC 3744 still out).
- Wire tests + SabreDAV e2e (native support).

## Constraints & conventions

- Provider A is never named; no production URL in the repo (env-gated smoke tier).
- Pipeline per package: issue → worktree + subagent → review + spot-check → local gates
  (fmt, clippy `-D warnings`, nextest unit `--all-features --locked`, doctests,
  `cargo test --all-features --no-run` for compile-all, `CARGO_TARGET_DIR=target`) →
  PR → **CI 14/14 verified explicitly** → squash merge → CHANGELOG union on conflicts.
- SonarCloud gates mandatory: coverage ≥80% new code, duplication ≤3% (share via
  `webdav/`/`common/`, never copy-paste caldav↔carddav).
- Docs kept in sync (README, AGENTS.md, doc comments, CHANGELOG) — stale docs are bugs.
- CodeQL lesson from 0.12: no uids/tokens interpolated into log lines or assertion
  messages (tests and examples included).
- Release only on explicit user go (publish.yml triggers on GitHub Release published).

## Risks

- SabreDAV fixture may not have the CalDAV scheduling plugin enabled — fallback is
  wire-only tests + docs note (coverage gate exemption documented in the PR).
- Radicale attachment support level (8607) to verify at implementation; if weaker than
  expected, S2 keeps wire tests + docs note.
- S1 touches `client.rs` conditional-write paths — keep schedule-tag methods additive;
  no changes to existing `If-Match` behavior.
- Provider A behaviors remain community-sourced; the smoke extension is read-only and
  env-gated so silent provider changes cannot break CI.
