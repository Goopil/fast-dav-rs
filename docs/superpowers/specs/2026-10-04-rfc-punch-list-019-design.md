# RFC coverage punch list (#225) — cycle 0.19 design

## Goal

Implement the full RFC coverage punch list from issue #225 (audit 2026-09-10):
17 items across P1 (feature gaps), P2 (API ergonomics), P3 (test/documentation
debt), delivered as 7 parallel work streams, each with its own sub-issue,
worktree, subagent and PR, integrated into `main` on a rolling basis and
released as **0.19.0**.

## Decisions (validated with maintainer, 2026-10-04)

- **Scope:** the complete punch list (P1 + P2 + P3) in a single cycle.
- **DNS SRV (P1#6):** out of scope. The existing "not implemented" note is
  promoted to a documented exclusion covering both SRV (RFC 6764 §3) and TXT
  (§6) — which resolves P3#15. A dedicated follow-up issue explores an opt-in
  `srv` cargo feature (hickory-resolver behind a `Resolver` trait) later.
- **API policy:** **100% additive** — no signature changes on existing public
  methods. Only new methods, new fields on `#[non_exhaustive]` types, new
  `Error`/`Operation` variants, new public builder functions. This keeps the
  cycle compatible with #224, which reserves surface reduction (breaking) for
  1.0.
- **Parallelism:** approach 1 — 7 sub-issues, 7 isolated git worktrees with
  parallel subagents, one PR per stream, merge-as-ready into `main`, conflicts
  resolved at integration (append-only conflicts on shared files).
- **Pipeline:** unchanged (issue → worktree + subagent → review → gates → PR →
  CI 14/14 → squash merge), per tracker #181 convention.

## Stream decomposition (by file-overlap, to minimize merge conflicts)

Shared files touched by several streams (append-only conflicts, resolved at
integration): `src/error.rs` (S1, S3, S6), `src/webdav/client.rs` (S2, S6),
`src/webdav/types.rs` (S2, S7), `src/caldav/client.rs` (S3, S6),
`CHANGELOG.md` / `README.md` / `src/lib.rs` re-exports (all).

### S1 — calendar-proxy + ACL (P1#1) — largest

New modules, minimal overlap with other streams:

- `src/webdav/acl.rs`: `WebDavClient::acl(path, body)` (RFC 3744 §8.1) plus a
  typed ACE builder (grant/deny of a `Privilege` to a principal, `<D:ace>`
  serialization). New `Operation::Acl`.
- `src/caldav/proxy.rs`: parse `calendar-proxy-read-for` /
  `calendar-proxy-write-for` principal properties (hrefs), resolve
  `calendar-proxy-read` / `calendar-proxy-write` group membership, typed
  helpers (`list_calendar_proxies`, grant/revoke via the ACL primitive).
- New `DavItem` fields: `calendar_proxy_read_for: Vec<String>`,
  `calendar_proxy_write_for: Vec<String>` (pattern: `current_user_principal`).
- e2e: attempt to enable calendar-proxy support in the SabreDAV fixture; if the
  plugin does not round-trip for fixture principals, record observed behavior
  and keep proxy e2e wire-only (0.13/S1 fallback convention, Sonar exemption
  documented in the PR).

### S2 — schedule-tag retrieval + scheduling tests (P1#3, P3#14)

- `pub fn schedule_tag_from_headers(headers: &HeaderMap) -> Option<String>` in
  `src/webdav/client.rs` — mirror of `etag_from_headers` (line 170), RFC 6638
  §10.1.1 semantics (quoted/ unquoted forms accepted).
- New `DavItem.schedule_tag: Option<String>` + parsing of the CalDAV
  `schedule-tag` property in `src/webdav/streaming.rs` (pattern:
  `calendar_timezone`).
- Unit test for the private `split_params_value` (`src/caldav/client.rs`,
  around line 1150) in the gated `unit_tests` target.
- e2e SabreDAV: outbox `POST` exercising the discovered outbox URL (assert the
  documented wire behavior, e.g. expected status for a well-formed / malformed
  iTIP body).

### S3 — managed-attachment lifecycle (P1#2)

- `CalDavClient::put_managed_attachment(href, body, content_type,
  managed_id)` — attachment update, `Cal-Managed-ID` request header (RFC 8607
  §5.2).
- `CalDavClient::delete_managed_attachment(href, managed_id)` — attachment
  removal, `Cal-Managed-ID` request header (§5.3).
- New `Operation::PutManagedAttachment` / `Operation::DeleteManagedAttachment`.
- e2e Radicale (supports RFC 8607, verified in 0.13): attach → GET by href →
  update → delete round-trip.

### S4 — body builders (P1#4, P2#8, P2#10) — `webdav/xml.rs` exclusive

- `data_element_xml` extended: `<C:limit-recurrence-set start=… end=…/>` and
  `<C:limit-freebusy-set start=… end=…/>` (RFC 4791 §9.6.4), composable with
  the existing `expand`.
- New additive builder `build_calendar_query_body_with_limits(...)` (existing
  `build_calendar_query_body` untouched) + one structured client entry point
  `calendar_query_options(path, component, CalendarQueryOptions)` — a single
  extensible method instead of new positional variants (anticipates the #224
  Phase C consolidation).
- PROPFIND helpers (RFC 4918 §9.1): `build_propfind_allprop()`,
  `build_propfind_propname()`, `build_propfind_props([...])` (typed prop list).
- Typed MKCALENDAR / MKCOL property builders: `MkCalendarProps`
  (displayname, description, supported-component-set) → XML, usable with the
  existing `mkcalendar` / `mkcol` methods (no new client methods required).

### S5 — CardDAV address-data (P1#5) — `carddav/*` exclusive

- `build_addressbook_query_body`: limited `address-data` form
  (`<C:address-data><C:prop name="…"/></C:address-data>`, RFC 6352 §10.4) and
  `<D:limit><D:nresults>` on `addressbook-query` (§10.6).
- Struct `AddressQueryOptions` + additive method `addressbook_query_options`.
- `i;octet` collation made explicit on `TextMatch` (today only a comment in
  `src/webdav/xml.rs`).
- Shared logic lives in `webdav/` — duplication gate ≤ 3% (no caldav↔carddav
  copy-paste).

### S6 — conditional writes + Depth + privileges (P2#7, #9, #11, #12, P3#16)

- Typed conditional-write errors: `Error::PreconditionFailed { operation,
  detail }` and `Error::PreconditionRequired { operation }` — 412/428 mapped on
  the existing conditional write methods (`Error` is `#[non_exhaustive]`).
- Public helper building the RFC 4918 §10.4 `If` header for lock-token-guarded
  PUT/DELETE.
- `copy_with_depth` / `move_with_depth` (RFC 4918 §9.8.3 / §9.9.3): the private
  `copy_move` gains an internal `Option<Depth>` parameter; existing public
  `copy` / `move` signatures unchanged.
- `Privilege::All` variant (aggregate `<D:all/>`) so write-gating matches
  servers granting `all`; doc caveat on the `Privilege::Other("all")` mapping
  (P3#16).
- `list_calendars` requests `current-user-privilege-set`; new
  `CalendarInfo.privileges: Vec<Privilege>` field (`#[non_exhaustive]`).
  Behavior change (extra requested property) documented in CHANGELOG/README.

### S7 — tests + docs (P3#13, #15, #17)

- Wire tests for `mkcol` (RFC 4918 / RFC 4791) and `mkcol`-with-body (RFC 5689)
  in `tests/unit/webdav/` (may use S4 builders once merged; otherwise
  hand-built bodies).
- `discovery.rs`: promote the SRV note to a documented exclusion covering SRV +
  TXT (RFC 6764 §3/§6), README section, follow-up issue text for the opt-in
  `srv` feature.
- `DavItem.principal_url: Option<String>` — parse `<D:principal-URL>`
  (RFC 5397), with docs stating the legacy/current-user-principal relationship.

## Testing strategy (per stream, mandatory gates)

- **Unit wire-mock** (`tests/unit/`): every new client method gets
  success / HTTP-error / pre-I/O validation tests (existing mock patterns in
  `tests/unit/webdav/client_tests.rs` and siblings). This is what satisfies the
  Sonar ≥ 80% coverage-on-new-code gate.
- **E2E** (`tests/e2e/<fixture>/`): only where the fixture genuinely supports
  the feature (S3 → Radicale 8607, S2 → SabreDAV outbox, S1 → SabreDAV
  calendar-proxy attempt). Otherwise observed behavior documented (0.13
  convention; Sonar exemption documented in the PR).
- **Doctests**: every new public API carries a `no_run` example. `docs/*.md`
  are crate doctests via `include_str!` and must stay green.
- Per-stream gates before PR: `cargo fmt`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo nextest run --all-features --locked
  --test unit_tests`, `cargo test --doc --all-features`.

## Parallelism & integration

1. Create 7 GitHub sub-issues referenced by #225 (one per stream, with the
   checklist from this design) — pattern: tracker #181 → #182-187.
2. 7 git worktrees (`../fast-dav-rs-s1-acl`, …), one `general` subagent per
   stream, all dispatched in parallel. Each subagent receives: the sub-issue,
   the matching spec chunk, the AGENTS.md conventions, the "100% additive"
   rule, and the list of files it must not touch (other streams' exclusive
   files).
3. Merge-as-ready into `main`; preferred merge order when several PRs are
   simultaneously ready: S5 → S4 → S3 → S7 → S2 → S6 → S1 (S1 is the largest
   and finishes last; spacing S6/S1 minimizes `error.rs` overlap).
4. Each merge triggers full CI (fixtures via prebuilt GHCR images).

## Release

- Once all 7 PRs are merged: release PR 0.19.0 (version bump, consolidated
  CHANGELOG, README/AGENTS sync), tag `v0.19.0`, GitHub Release. crates.io
  publish by maintainer (0.14+ convention).
- Close #225 and the 7 sub-issues; create the follow-up issue for the opt-in
  `srv` feature (Option A).

## Explicitly out of scope (this cycle)

- Any surface reduction (#224 Phase C stays parked at 1.0).
- DNS SRV/TXT lookup (follow-up issue).
- The documented exclusions of #225 remain out of scope (iCalendar body
  assembly, managed-attachment × scheduling interplay, quota RFC 4331, WebDAV
  SEARCH RFC 5323, versioning RFC 3253, BIND RFC 5842, ordered collections
  RFC 3648).

## Risks

| Risk | Mitigation |
|---|---|
| `error.rs` conflicts (S1, S3, S6) | Append-only variants; merge order spacing S6/S1 |
| SabreDAV calendar-proxy not exposed by fixture | Wire-tests fallback + observed behavior documented (0.13 convention) |
| CHANGELOG/README multi-stream merges | One `Unreleased/0.19` subsection per stream |
| caldav/carddav duplication (S4/S5 options structs) | Shared engine in `webdav/`, duplication gate ≤ 3% enforced by review |
| Outbox POST fixture behavior unknown | Assert documented wire behavior; record observations |
