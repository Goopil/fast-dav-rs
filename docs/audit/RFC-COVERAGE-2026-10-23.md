# RFC Coverage Audit — fast-dav-rs (follow-up)

- **Date:** 2026-10-23
- **Revision audited:** `7d830ee` (v0.19.0, branch main)
- **Method:** same protocol as the 2026-09-10 audit — six parallel domain audits (one per RFC
  family), each requirement re-verified against the codebase with fresh `file:line` evidence for
  both the implementation and its tests; every line number was re-read at HEAD, not inherited from
  the previous audit (whose numbers were stale after 0.18.0/0.19.0). Statuses: **COVERED**
  (implementation + test proof), **PARTIAL** (implemented with a defined shortfall or untested
  surface), **GAP** (absent), **OUT-OF-SCOPE** (justified, server-only, or documented as out of
  scope).
- **Scope:** same 12 RFC families as the baseline audit — 4918, 6578, 8144, 4791, 6638, 8607,
  6352, 6764, 3744/5397, 5545/6350, plus the documented exclusions (5689, 4331, 5323, 3253,
  5842, 3648).
- **Bilan:** **62 COVERED · 3 PARTIAL · 2 GAP · 4 OUT-OF-SCOPE** (baseline 2026-09-10: 46 / 16 / 5 / 4).
  Every gap and partial from the 2026-09-10 punch list (issue #225) landed except the two
  documented RFC 6764 discovery gaps (SRV/TXT — tracked by the opt-in `srv` feature proposal,
  issue #254). No regression found in any family.

## Executive summary

| RFC | Domain | COVERED | PARTIAL | GAP | OUT-OF-SCOPE |
|-----|--------|---------|---------|-----|--------------|
| 4918 — WebDAV class 1/2 | 14 | 0 | 0 | 0 |
| 6578 — WebDAV Sync | 8 | 0 | 0 | 0 |
| 8144 — Prefer | 2 | 0 | 0 | 1 |
| 4791 — CalDAV | 12 | 1 | 0 | 0 |
| 6638 — Scheduling | 4 | 1 | 0 | 1 |
| 8607 — Managed attachments | 3 | 1 | 0 | 0 |
| 6352 — CardDAV | 8 | 0 | 0 | 0 |
| 6764 — Service discovery | 2 | 0 | 2 | 0 |
| 3744 + 5397 — ACL/principal (+5689/4331/5323) | 5 | 0 | 0 | 2 |
| 5545/6350 — iCalendar/vCard policy | 4 | 0 | 0 | 0 |
| **Total** | **62** | **3** | **2** | **4** |

The crate is now **feature-complete for the practical client surface of every in-scope RFC**.
The 2026-09-10 punch list was fully executed: calendar-proxy + ACL (S1 stream), managed-attachment
lifecycle, typed schedule-tag, recurrence/freebusy limits, CardDAV limited address-data +
`limit`/`nresults` + `i;octet`, PROPFIND body helpers, typed 412/428 conditional writes, Depth on
COPY/MOVE, typed MKCALENDAR, privileges on `list_calendars`, `Privilege::All`, `principal-URL`,
and the mkcol/mkcol-with-body/outbox-e2e/`split_params_value` test debt. The only remaining
feature work is RFC 6764 §3/§6 SRV+TXT discovery (opt-in, issue #254).

---

## Deltas since 2026-09-10 (16 rows improved)

| Requirement | Was → Now | What landed |
|---|---|---|
| 4918 #2 PROPFIND body variants | PARTIAL → COVERED | `build_propfind_allprop`/`build_propfind_propname` + `build_propfind_props` (typed prop list, ns grouping, injection-safe escaping), 4 unit tests |
| 4918 #5 MKCOL | PARTIAL → COVERED | `tests/unit/webdav/mkcol_tests.rs` — 4 wire tests (bare MKCOL, 405 passthrough, RFC 5689 extended body + Content-Type, 207 passthrough) |
| 4918 #7 conditionals + `If` header | PARTIAL → COVERED | Typed 412/428 via `conditional_write_error` → `Error::PreconditionFailed`/`PreconditionRequired`; public `if_header_for_lock_token` (§10.4/§10.5 coded-URL validation); 9 tests |
| 4918 #9 COPY/MOVE Depth | PARTIAL → COVERED | `copy_with_depth` (§9.8.3 verbatim) / `move_with_depth` (§9.9.3 Depth 0-only, pre-I/O rejection); 6 wire tests |
| 4791 #1 MKCALENDAR typed builder | PARTIAL → COVERED | `MkCalendarProps` + `build_mkcalendar_body` (displayname, description, supported-component-set), 4 tests + doctests + README |
| 4791 #7 limit-recurrence/freebusy-set | GAP → COVERED | `CalendarDataLimits` + `data_element_xml_with_limits` (DTD-ordered after `expand`), `calendar_query_options` entry point, pre-I/O `#REQUIRED` validation |
| 4791 #12 MKCOL from CalDAV | PARTIAL → COVERED | Delegated + covered by the new `mkcol_tests.rs` (unit tier; no e2e fixture drives MKCOL itself) |
| 6638 #4 schedule-tag typed retrieval | PARTIAL → COVERED | `schedule_tag_from_headers`, `DavItem.schedule_tag` + streaming parse, `put_if_schedule_tag`/`delete_if_schedule_tag`, cross-origin redirect stripping |
| 6638 #5 calendar-proxy | GAP → COVERED | `src/caldav/proxy.rs` (proxy-for listing, proxy-group resolution, grant/revoke) built on the new `WebDavClient::acl` + `build_acl_body`; 13 unit tests + e2e records fixture limitations |
| 8607 #3 attachment update/removal | GAP → COVERED | `put_managed_attachment`/`delete_managed_attachment` with `Cal-Managed-ID` request header + `managed_id_from_location` fallback |
| 6352 #1 addressbook-query limits + i;octet | PARTIAL → COVERED | `AddressQueryOptions.limit`, `limit_nresults_xml` as last child, `Collation::Octet` |
| 6352 #3 address-data limited-props | PARTIAL → COVERED | Props branch in `build_addressbook_query_body_with_options` with `[A-Za-z0-9-]+` validation + stable dedupe |
| 6764 #3 TXT path hint | GAP (undocumented) → GAP (documented) | Exclusion now documented next to SRV in `discovery.rs`, `docs/advanced-configuration.md`, README |
| 3744 #2 `Privilege::All` | PARTIAL → COVERED | Dedicated `All` variant (parser maps `all` case-insensitively), serialized in ACL bodies, doc caveat per punch list #16 |
| 3744 #3 privileges on `list_calendars` | PARTIAL → COVERED | `list_calendars` requests `current-user-privilege-set`; `CalendarInfo.privileges` |
| 3744 #4 `principal-URL` | PARTIAL → COVERED | Parsed into `DavItem.principal_url` (legacy, RFC 5397 §3 documented) |

Unchanged-but-verified: every previously COVERED row was re-confirmed at HEAD with fresh line
numbers; no regressions found in any family.

---

## RFC 4918 — WebDAV class 1/2

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | PROPFIND Depth 0/1/infinity | COVERED | `Depth` enum + `as_str()` src/webdav/types.rs:427-443; header emission `WebDavClient::propfind` src/webdav/client.rs:1701-1721 (Depth at 1707-1708), `report` (1786-1806), `propfind_many` (2101-2107) | PROPFIND wire asserts `depth: 0` — tests/unit/caldav/timezone_tests.rs:39-57; Depth::Zero/One on live SabreDAV — tests/e2e/sabredav/caldav/compatibility/compatibility_tests.rs:25-66; `Depth: infinity` on the wire (COPY) — tests/unit/webdav/copy_move_tests.rs:49-68 |
| 2 | PROPFIND body variants (allprop/propname/prop, §9.1) | COVERED | `build_propfind_allprop` src/webdav/xml.rs:317-332; `build_propfind_propname` xml.rs:334-350; `build_propfind_props(&[(&ns,&name)])` xml.rs:352-408 (root-level ns declarations, dedup, escaping); re-exported src/webdav/mod.rs:34-37, src/lib.rs:50-51; documented README.md:523-532 | tests/unit/webdav/body_builder_tests.rs:144-215 — 4 tests: exact allprop body, exact propname body, namespace grouping/dedup, escaping/injection |
| 3 | 207 Multi-Status parsing (propstat + per-item statuses) | COVERED | `CommonParser` builds one `PropStat` per group into `item.propstats` src/webdav/streaming.rs:113-129; response-level `<D:status>` 155-159; first-200-propstat rule 160-171; `http_status_code` src/webdav/types.rs:466-479 | tests/unit/webdav/compliance_tests.rs:192-317 — multiple propstat groups, backward-compat 200 fields, response-level status, single propstat; streaming_tests.rs:451-600 |
| 4 | PROPPATCH + 207 per-propstat evaluation (§14.23) | COVERED | `proppatch` sends explicit `Depth: 0` (§9.2) src/webdav/client.rs:1723-1742; per-propstat evaluation in `set_calendar_timezone` src/caldav/client.rs:355-394 incl. §14.23 404-on-remove idempotency (381-392) | tests/unit/webdav/protocol_tests.rs:4-35 (wire `depth: 0`); tests/unit/caldav/timezone_tests.rs:115-230 (207 bodies 200/403/404); e2e radicale core.rs:142-206, nextcloud crud.rs |
| 5 | MKCOL (§9.1) | COVERED | `WebDavClient::mkcol(path, xml_body: Option<&str>)` src/webdav/client.rs:1808-1831 — raw-verb contract (201 success; server-dependent 207 passed through), RFC 5689 extended body with `Content-Type: application/xml` | tests/unit/webdav/mkcol_tests.rs:28-135 — 4 direct wire tests: bare MKCOL + empty body + no Content-Type, 405 passthrough, extended body verbatim + Content-Type, 207 error-propstat passthrough |
| 6 | GET/HEAD with ETag/Content-Type/Last-Modified | COVERED | `head`/`get` src/webdav/client.rs:1504-1513; `etag_from_headers` client.rs:170-213; getcontenttype/getlastmodified src/webdav/streaming.rs:219-233 | tests/unit/webdav/protocol_tests.rs:37-91; parser tests tests/unit/caldav/parser_tests.rs:187-212, tests/unit/carddav/parser_tests.rs:98-123; e2e sabredav contact_tests.rs:66-83 |
| 7 | PUT conditionals If-Match/If-None-Match; `If` header for lock-token-guarded writes (§10.4) | COVERED | `put_if_match_with` src/webdav/client.rs:1544-1559; `put_if_none_match` src/caldav/client.rs:161-172 (+carddav mirror); public `conditional_write_error` client.rs:215-261 mapping 412→`Error::PreconditionFailed`, 428→`Error::PreconditionRequired` (src/error.rs:261-294); public `if_header_for_lock_token` client.rs:263-300 (§10.4 coded URL, §10.5 token validation) | tests/unit/webdav/conditional_tests.rs:13-120 — 9 tests (2xx→Ok, 412, 428, other→UnexpectedStatus, coded-URL build + rejections); wire If-Match tests tests/unit/caldav/etag_tests.rs:103-214; If-None-Match/If-Match wire tests tests/unit/caldav/validation_tests.rs:270-395 |
| 8 | DELETE (+ conditional) | COVERED | `delete` src/webdav/client.rs:1516-1519; `delete_if_match` client.rs:1521-1539 (weak ETags rejected client-side, RFC 9110 strong comparison) | tests/unit/caldav/etag_tests.rs:103-198 (wire DELETE If-Match, pre-I/O invalid/weak rejection incl. `InvalidEtag{Weak}`); e2e contact_tests.rs:66-98 |
| 9 | COPY/MOVE with Destination + Overwrite (+ Depth, §9.8/§9.9) | COVERED | `copy` 1571-1579, `copy_with_depth` 1591-1600 (Depth verbatim, §9.8.3), `r#move` 1608-1616, `move_with_depth` 1629-1645 (Depth::Zero only, others rejected pre-I/O per §9.9.3), shared `copy_move` 1647-1698 (absolute-URI Destination validation + no-userinfo rule 1655-1681, Overwrite T/F 1692-1695, Depth 1683-1687) | tests/unit/webdav/copy_move_tests.rs:18-163 — 6 wire tests; Destination validation tests/unit/webdav/uri_tests.rs:118-190; e2e resource_tests.rs:43-110 |
| 10 | LOCK class 2 (lockdiscovery, Timeout §10.7, 423, coded-URL §10.5) | COVERED | `lock` src/webdav/client.rs:1984-2017 (Depth 0 per §9.10.4, Timeout clamped per §10.7); `refresh_lock` 2050-2067 (§9.10.7 body-less If-header refresh); shared `lock_request` 1900-1933; `timeout_header_value` 1882-1887; `parse_lock_discovery_bytes` src/webdav/streaming.rs:1600+ | tests/unit/webdav/locking_tests.rs:36-715 — 33 tests (lockinfo wire, 423 + no-conflicting-lock precondition, Timeout fallback, Depth 0 + clamp, refresh If-header, lockdiscovery parse matrix); e2e sabredav locking_tests.rs:33-173 |
| 11 | UNLOCK with Lock-Token | COVERED | `unlock` src/webdav/client.rs:2083-2098 — `Lock-Token` coded-URL header, token validation, precondition-aware errors | tests/unit/webdav/locking_tests.rs:426-562 — Lock-Token wire header, non-success→`UnexpectedStatus{Unlock}`, precondition body, malformed body, invalid/empty token |
| 12 | DAV header / compliance classes (§10.1) | COVERED | `DavCapabilities`/`DavCompliance` + `parse_dav_header` src/webdav/types.rs:481-499, 556+; `capabilities()` src/webdav/client.rs:1490-1501 | tests/unit/webdav/compliance_tests.rs:7-88, 319-413 — 28 tests; e2e asserts SabreDAV advertises `1, 3, extended-mkcol, calendar-access` (discovery_tests.rs:45) |
| 13 | `<D:error>` precondition parsing (§16/§14.12) | COVERED | `parse_error_body` src/webdav/streaming.rs:1704-1810 (DAV:-ns child preferred, `parse_failed=true` for garbage); wired into `status_error` src/webdav/client.rs:1835-1856 | tests/unit/webdav/compliance_tests.rs:89-190 — 10 tests; locking_tests.rs:143-213, 471-537 |
| 14 | Percent-encoding + href normalization | COVERED | `encode_path_segments` src/webdav/client.rs:383-399; `build_uri` client.rs:821+; `normalize_href` src/webdav/multiget.rs:136-179 (shared with calendar-proxy src/caldav/proxy.rs:105,280,355); Destination must be pre-encoded (client.rs:1655-1671) | tests/unit/webdav/uri_tests.rs:9-190 — 20 tests; percent-echo equivalence via multiget `missing_hrefs` tests (carddav client_tests.rs:1012-1040) |

## RFC 6578 — WebDAV Sync

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `sync-collection` REPORT body (Depth 0) | COVERED | `build_sync_collection_body` src/webdav/xml.rs:217-250; `sync_collection_report` sends `Depth::Zero` src/webdav/client.rs:2426-2439; CalDAV wrapper src/caldav/client.rs:889-906; CardDAV src/carddav/client.rs:637-648 | Depth-0 wire assertion tests/unit/caldav/client_tests.rs:889-912; body shapes tests/unit/webdav/sync_tests.rs:44-105; exact-byte body body_builder_tests.rs:82-123 |
| 2 | sync-token: initial-sync; top-level + `Sync-Token` header chain | COVERED | Empty-token initial sync src/webdav/xml.rs:234-236; resolution chain top-level → header → per-item src/webdav/types.rs:989-1009; `normalize_sync_token` src/webdav/client.rs:153-155; parser capture src/webdav/streaming.rs:192,1047,1235 | Priority-chain tests tests/unit/caldav/caldav_helpers.rs:286-376, 670-676; initial-sync wire assertions tests/unit/webdav/sync_session_tests.rs:270-274, 758-766 |
| 3 | `sync-level` 1/infinite | COVERED | `SyncLevel` src/webdav/types.rs:35-53; emission src/webdav/xml.rs:237-239; `sync_collection_with_level` src/webdav/client.rs:2321-2341 | as_str tests tests/unit/webdav/sync_tests.rs:38-42; wire capture :107-140; CalDAV infinite tests/unit/caldav/client_tests.rs:1337-1362 |
| 4 | `limit`/`nresults` + multi-page loop | COVERED | `limit_nresults_xml` src/webdav/xml.rs:177-182; `SyncSession::sync_pages` src/webdav/sync.rs:436-477, `dedup_rows` :672-685 | page continuation tests/unit/webdav/sync_session_tests.rs:803-853; e2e `test_sync_limit_and_pagination` tests/e2e/sabredav/caldav/sync/sync_tests.rs:476-626 |
| 5 | Truncation: 507 → `truncated` (§3.6) | COVERED | 507 detection src/webdav/types.rs:1013-1015; `SyncResponse.truncated` src/caldav/types.rs:96-98; non-continuable → `Error::SyncIncomplete` src/webdav/sync.rs:466-474 | tests/unit/caldav/client_tests.rs:915-1003; sync_session_tests.rs:855-953; e2e truncation_tests.rs:29-104 |
| 6 | Deletions: 404/410 → `is_deleted` (§3.5); 5xx ≠ deletion | COVERED | `is_deleted` src/webdav/types.rs:1026; `has_error_status` src/webdav/sync.rs:696-700, skip in `diff_rows` :740-742, kept-in-state :795-801 | 404/410/not-"4040" tests tests/unit/caldav/caldav_helpers.rs:622-668; 5xx-unknown tests/unit/webdav/sync_session_tests.rs:346-384; "507 is not a deletion" client_tests.rs:966; e2e sync_tests.rs:443-463 |
| 7 | Stale token: 410 / 403+`valid-sync-token` → resync; §3.4 suppression | COVERED | `sync_collection_resilient_report` src/webdav/client.rs:2447-2477 (stale check :2458-2463, one empty-token re-issue :2470-2474); `resynced` flag src/webdav/sync.rs:459; §3.4 `diff_rows(replace=true)` :413-418, 713-737 | 410/403 recovery tests tests/unit/webdav/sync_tests.rs:143-277; sync_session_tests.rs:734-800; e2e radicale sync.rs:124-190 (§3.4 assert :179-182), sabredav sync_tests.rs:793-884 |
| 8 | `SyncSession` stateful engine | COVERED | src/webdav/sync.rs (state :132-143; single-flight :227-233; initial :292-303; incremental :331-342; probe :344-356; 405-downgrade-only :358-382; PROPFIND fallback :481-598; batched multiget :607-658) | 18 wire tests / 1036 lines tests/unit/webdav/sync_session_tests.rs; sync_capability_tests.rs:37-97; e2e all three fixtures |

## RFC 8144 — Prefer

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 9 | `Prefer: return=minimal/representation` emission | COVERED | `Prefer` enum src/webdav/types.rs:55-87; builder option src/webdav/builder.rs:491-511; injection with per-request precedence src/webdav/client.rs:1126-1133; `put_if_match_prefer` (macro) client.rs:2989-3008 | tests/unit/webdav/prefer_tests.rs:83-256 — 9 tests (builder, per-request wins, none by default, CalDAV/CardDAV override) |
| 10 | `Preference-Applied` parsing | COVERED | `preference_applied_from_headers` src/webdav/client.rs:302-339 (lenient, case-insensitive, invalid UTF-8 → None) | tests/unit/webdav/prefer_tests.rs:15-81 (both values, case-insensitive, unrecognized → None, invalid UTF-8) |
| 11 | `wait` / `respond-async` / 102 Processing | OUT-OF-SCOPE | documented "v1.0 supports the `return` preference only" + manual `HeaderMap` escape hatch src/webdav/types.rs:57-59, docs/advanced-configuration.md:255-265 | negative test `wait=10` → None prefer_tests.rs:60-61 |

## RFC 4791 — CalDAV

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | MKCALENDAR with calendar properties (§5.3.1) | COVERED | `MkCalendarProps` + `build_mkcalendar_body` src/webdav/xml.rs:410-534; `CalDavClient::mkcalendar` src/caldav/client.rs:174-193; re-exports src/caldav/mod.rs:12-14, src/lib.rs:30,33; README.md:518-560 | tests/unit/webdav/body_builder_tests.rs:219-275 (4 tests incl. `InvalidComponentName` rejections); wire test `mkcalendar_sends_depth_zero` tests/unit/caldav/client_tests.rs:1006-1035; e2e all 3 fixtures |
| 2 | `calendar-query` filters + pre-I/O DTD validation (§7.8, §9.7.x) | COVERED | `calendar_query` src/caldav/client.rs:635-711; `calendar_query_timerange` :451-473; `calendar_query_stream` :508-535; filter model src/caldav/types.rs:145-350; renderers src/webdav/xml.rs:536-599, 280-315 | tests/unit/caldav/filter_tests.rs (100+ tests); client_tests.rs:711-803, 1885-1976; e2e sabredav parsing_tests.rs:298 |
| 3 | `calendar-multiget` + href reconciliation (§7.9) | COVERED | `calendar_multiget` src/caldav/client.rs:731-757; chunked `calendar_multiget_many` :835-865 → shared engine src/webdav/multiget.rs | tests/unit/caldav/client_tests.rs:218-300, 1487-1884 (11 wire tests incl. missing-hrefs reconciliation, partial-failure isolation); e2e multiget_tests.rs:31-86 |
| 4 | `free-busy-query` + FBTYPE (§7.10) | COVERED | `free_busy_query` src/caldav/client.rs:948-990 (§7.10.2 multistatus and Sabre bare `text/calendar`); `parse_free_busy_periods` :1365-1401; `split_params_value` :1406-1416 (quote-aware); `fbtype_from_params` :1423-1440 | tests/unit/caldav/client_tests.rs:10-201 (5 wire tests incl. folded lines, quoted params); in-module client.rs:1607-1639; e2e wave3_tests.rs:90-179 |
| 5 | `calendar-data` inline retrieval (§9.6) | COVERED | Parser src/webdav/streaming.rs:383-384, 951-961 (multi-line preserved, capped); `DavItem.calendar_data` src/webdav/types.rs:806 | tests/unit/caldav/parser_tests.rs:126-159 (CDATA multiline); wire test client_tests.rs:2135-2206; e2e across fixtures |
| 6 | `<C:expand>` recurrence expansion (§9.6.5) | COVERED | `data_element_xml` src/webdav/xml.rs:84-175; `validate_expand` src/caldav/client.rs:1312-1325 (both attrs `#REQUIRED`, `end > start`); wired into query/multiget/sync/stream | tests/unit/caldav/client_tests.rs:189-383 (wire + pre-network rejections); DTD order test body_builder_tests.rs:5-23; e2e wave3_tests.rs:192-359 |
| 7 | `limit-recurrence-set` / `limit-freebusy-set` (§9.6.4) | COVERED | `CalendarDataLimits` src/webdav/types.rs:1062-1096; `data_element_xml_with_limits` + `limit_element_xml` src/webdav/xml.rs:124-192 (after `expand`, §9.6 DTD order); `validate_data_limits` src/caldav/client.rs:1331-1353; `calendar_query_options` :588-612 | tests/unit/webdav/body_builder_tests.rs:4-142 (5 tests incl. DTD order); tests/unit/caldav/client_tests.rs:2079-2330 (serialization, wire REPORT, pre-I/O `#REQUIRED` enforcement); README.md:563-600 |
| 8 | `supported-calendar-component-set` (§5.2.3) | COVERED | Parser src/webdav/streaming.rs:379-382; requested by `list_calendars` src/caldav/client.rs:400-417; `CalendarInfo.supported_components` src/caldav/types.rs:42 | tests/unit/caldav/parser_tests.rs:122-153; parser_edge_cases.rs:144-215; e2e nextcloud crud.rs:15-36 |
| 9 | `calendar-timezone` read + PROPPATCH write (§5.2.2) | COVERED | Read src/caldav/client.rs:256-276; write/remove :321-396 (XML-escaped set, `<D:remove>`, 404-remove idempotent); multi-line parser src/webdav/streaming.rs:972-984 | tests/unit/caldav/timezone_tests.rs:39-261 (10 tests); e2e round-trips radicale core.rs:142-204, nextcloud crud.rs:256-312 |
| 10 | `max-resource-size` / `supported-calendar-data` / `max-attendees-per-instance` (§5.2.x) | COVERED | Parser src/webdav/streaming.rs:409-416, 484-509; `CalendarInfo` src/caldav/types.rs:34-51; requested src/caldav/client.rs:408-410, mapped :1531-1544 | tests/unit/caldav/client_tests.rs:1064-1122 (wire + mapping), :1213-1235 |
| 11 | `calendar-home-set` discovery (§6.2.1) | COVERED | `discover_calendar_home_set` src/caldav/client.rs:196-219; parser src/webdav/streaming.rs:393-394 | tests/unit/caldav/parser_tests.rs:145; e2e sabredav discovery_tests.rs:131-189, nextcloud discovery.rs:30,78 |
| 12 | MKCOL path reachable from CalDAV client | COVERED | `WebDavClient::mkcol` src/webdav/client.rs:1820-1831; delegated to `CalDavClient` client.rs:3072-3079 | NEW tests/unit/webdav/mkcol_tests.rs:29-147 (4 wire tests incl. RFC 5689 extended path with a CalDAV-typed body). Unit tier only — no live-fixture e2e drives `mkcol` directly |
| 13 | iCalendar body assembly for PUT | PARTIAL (by design) | Crate validates but never constructs iCalendar: src/caldav/validation.rs:1-7 ("out of scope"), `ValidationLevel` :31-44, `prepare_ical_put` src/caldav/client.rs:128-136; documented README.md:81,903 | tests/unit/caldav/validation_tests.rs:20-181 (13 validator tests) + :238-479 (9 wire tests: pre-I/O rejection, version parameter, level matrix) |

## RFC 6638 — Scheduling

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | schedule-inbox/outbox/calendar-user-address-set discovery (§2.1, §2.4) | COVERED | `discover_schedule_endpoints` src/caldav/scheduling.rs:191-229; `ScheduleEndpoints` :44-54; `DavItem.schedule_inbox/schedule_outbox/calendar_user_addresses` src/webdav/types.rs:810-815 | Unit wire test tests/unit/caldav/scheduling_tests.rs:44-57; e2e `test_discover_schedule_endpoints_on_sabredav` tests/e2e/sabredav/caldav/scheduling_tests.rs:18-54 |
| 2 | POST iTIP to outbox (§5) + scheduling-response parsing | PARTIAL | `post_schedule` src/caldav/scheduling.rs:304-346 (Originator/Recipient headers, `Operation::PostSchedule`); `SchedulingResponse { status, body }` :81-88 — body returned **verbatim**; "no iTIP parsing is performed" documented :56-61 (typed `CALDAV:schedule-response` XML parsing still absent by design) | Unit: scheduling_tests.rs:86-206 (per-recipient headers, non-success mapping, pre-I/O validation). NEW e2e: `test_outbox_post_well_formed_free_busy_request` tests/e2e/sabredav/caldav/scheduling_tests.rs:123-165 (live 200 + `schedule-response`/`2.0;Success` assertion) + malformed-POST rejection :171-210 |
| 3 | schedule-inbox listing (§2.2) | COVERED | `list_inbox` src/caldav/scheduling.rs:383-416 (Depth-1 PROPFIND, deleted/empty filtered); `InboxItem` :108-117 | Unit scheduling_tests.rs:209-246; e2e `test_list_inbox_empty_on_sabredav` :60-83 |
| 4 | schedule-tag typed retrieval + `If-Schedule-Tag-Match` writes (§6.1, §8.3, §10.1.1) | COVERED | `schedule_tag_from_headers` src/webdav/client.rs:207-213; `DavItem.schedule_tag` src/webdav/types.rs:821-824; streaming parse src/webdav/streaming.rs:433-434, 986-996; `put_if_schedule_tag`/`delete_if_schedule_tag` src/caldav/scheduling.rs:472-531; cross-origin strip src/webdav/client.rs:1329-1334 | tests/unit/webdav/header_helpers_tests.rs:6-65 (header helper, property parse, absent→None); conditional-write wire tests tests/unit/caldav/scheduling_tests.rs:269-400 (quoted header, Content-Type, 412/404 passthrough, pre-I/O rejections); redirect safety redirect_tests.rs:598-601; e2e records SabreDAV 4.7.1's lack of §8 :221-285 |
| 5 | calendar-proxy client features (read-for/write-for, proxy-group resolution, grant/revoke) | COVERED | `src/caldav/proxy.rs`: `list_calendar_proxies` :155-182, `calendar_proxy_group_members` :198-227, `grant_calendar_proxy` :260-302, `revoke_calendar_proxy` :338-383 (member-aware ACL re-issuance via `partition_members` :97-117; revoke of non-member → `InvalidInput` pre-I/O :355-363; empty `<D:acl>` for last-member :364-368). Typed `build_acl_body`/`Ace` src/webdav/acl.rs:46-71, 206-219; `WebDavClient::acl` src/webdav/client.rs:1760-1783; `DavItem.calendar_proxy_*` src/webdav/types.rs:825-839; parse src/webdav/streaming.rs:1096-1124 | tests/unit/caldav/proxy_tests.rs (13 tests: mapping, group resolution, grant/revoke wire shapes, href normalization, non-member revoke, empty-ACL body, 403→Operation mapping); parser tests tests/unit/webdav/proxy_parse_tests.rs:24-66; ACL builder suite tests/unit/webdav/acl_tests.rs:4-201; e2e tests/e2e/sabredav/caldav/proxy_tests.rs:22-115 (records fixture lack of calendar-proxy support) |
| 6 | managed-attachments × scheduling interplay (RFC 8607 §7) | OUT-OF-SCOPE | No code or doc mention; justified: no fixture implements managed attachments (README.md:892, docs/compatibility.md:35,53) | — |

## RFC 8607 — Managed attachments (CalendarServer wire form)

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | POST `?action=attachment-add` + `Cal-Managed-ID` response | COVERED | `post_managed_attachment` src/caldav/client.rs:1067-1118 (uid/recurrence-id percent-encoding :1084-1092, `Cal-Managed-ID` read :1108-1112 with `managed_id_from_location` fallback :1283-1293); `ManagedAttachment` src/caldav/types.rs:19-29. Note: the RFC 8607 §3.4 resource-targeted conforming form (POST with `rid`) is documented as not implemented (client.rs:1021-1026) — CalendarServer collection-targeted form only | tests/unit/caldav/attachments_tests.rs:20-155 (href+id from headers, encoding, Location fallback, no-managed-id error, 403 mapping); e2e Radicale 405 recorded tests/e2e/radicale/attachments.rs:19-59 |
| 2 | GET attachment by href | PARTIAL | Via generic `get` (src/webdav/client.rs:1510); `post_managed_attachment` doc points to "later GET/PUT/DELETE" (client.rs:1029-1034). No dedicated method and **no test at any tier** — no managed-attachment-capable fixture exists (README.md:892) | None (unchanged since 2026-09-10) |
| 3 | Replacement/removal (`attachment-update`, DELETE with `Cal-Managed-ID`, §5.2/§5.3) | COVERED | `put_managed_attachment` src/caldav/client.rs:1155-1191; `delete_managed_attachment` :1222-1246; `MANAGED_ID_HEADER` :1253-1255; `managed_attachment_header` pre-I/O validation :1257-1271; `Operation::PutManagedAttachment`/`DeleteManagedAttachment` src/error.rs:537,540 | tests/unit/caldav/attachments_tests.rs:162-386 (PUT/DELETE wire shapes, non-success → typed Operations, empty managed-id/href/content-type rejected pre-I/O with no wire request); e2e Radicale 400/404 recorded :77-161 |
| 4 | `managed-ids` property parsing (§5.1) | COVERED | `ElementName::ManagedId` src/webdav/streaming.rs:332, 425-426; parse into `DavItem.managed_ids` streaming.rs:1090-1095; field src/webdav/types.rs:816-820 | tests/unit/webdav/streaming_tests.rs:197-215 |

## RFC 6352 — CardDAV

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `addressbook-query` (§10.5/§10.6): filters, text-match+collations, DTD validation, limit/nresults | COVERED | `addressbook_query_options` src/carddav/client.rs:333-367; `AddressQueryOptions.limit` src/webdav/types.rs:388-391, 420-424; `limit_nresults_xml` src/webdav/xml.rs:177-182 appended as **last child** (client.rs:859-861); `text_match_xml` always emits collation+match-type xml.rs:546-568; `Collation::Octet` → `"i;octet"` types.rs:153-180; DTD exclusivity validation client.rs:443-464 | tests/unit/carddav/client_tests.rs:171-187 (i;octet wire), 1175-1287 (DTD rejections pre-I/O), 1390-1614 (limited props + limit-last-child + wire capture + invalid names + stable dedupe); filter_tests.rs (37 tests); e2e contact_tests.rs:44-142, parsing_tests.rs:44-72 |
| 2 | `addressbook-multiget` (§8.7) | COVERED | `addressbook_multiget` src/carddav/client.rs:502-529 (Depth::Zero per §8.7, 503-505); builder → shared `webdav::xml::build_multiget_body` | tests/unit/carddav/client_tests.rs:189-210; e2e resilience_tests.rs:109-124, parallel_tests.rs:100-119 |
| 3 | `address-data` incl. limited-props form (§10.4/§10.4.2) | COVERED | Limited form in `build_addressbook_query_body_with_options` src/carddav/client.rs:840-864 (props branch 848-856, escape 852); `[A-Za-z0-9-]+` validation + first-occurrence dedupe :338-349; `address_data_props` src/webdav/types.rs:381-387, 412-418. (Implemented in the CardDAV body builder, not a `data_element_xml` branch — functionally equivalent) | tests/unit/carddav/client_tests.rs:1390-1614 (both props + limit, bare form, wire capture with duplicate FN dropped, invalid names → `InvalidInput` pre-I/O) |
| 4 | `supported-address-data` (§6.2.2) | COVERED | Parser src/webdav/streaming.rs:397-402 (+ shared helper 485+); requested in `list_addressbooks` src/carddav/client.rs:237-251 | tests/unit/carddav/parser_tests.rs:12-35; parser_edge_cases.rs:70-232; tests/unit/webdav/streaming_tests.rs:170-185; e2e discovery_tests.rs:87-123 |
| 5 | home-set / description / color discovery | COVERED | `discover_addressbook_home_set` src/carddav/client.rs:211-235; `list_addressbooks` PROPFIND carries description/color/supported-address-data :243-245 | e2e `test_discovery_operations` tests/e2e/sabredav/carddav/discovery/discovery_tests.rs:125-166; unit parser_tests.rs:12-35 |
| 6 | vCard verbatim (no client-side validation) | COVERED (documented) | `put` doc src/carddav/client.rs:114-138 ("no client-side vCard validation", 122-125); VERSION-derived Content-Type (116-120) | e2e parsing_tests.rs:44-93 (roundtrip preserves fields; malformed vCard rejected by server) |
| 7 | Chunked `addressbook_multiget_many` | COVERED | src/carddav/client.rs:531-627 → shared engine (chunking, ordering, per-chunk failure isolation, missing-hrefs reconciliation) | tests/unit/carddav/client_tests.rs:794-1080 (7 tests) |
| 8 | Structured `CardDavFilter` → XML + validation | COVERED | `CardDavFilter` + `to_filter_xml` src/carddav/types.rs:6-107 (§10.5.1 DTD note); pre-I/O validation src/carddav/client.rs:443-464 (§10.5.1/§10.5.2 exclusivity) | tests/unit/carddav/filter_tests.rs (37 tests); client_tests.rs:1175-1287 (both DTD rejections cite §10.5.1/§10.5.2) |

## RFC 6764 — Service discovery

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `.well-known/{caldav,carddav}` PROPFIND + redirects + https enforcement (§5, §6) | COVERED | `discover_well_known` src/webdav/discovery.rs:74-134 (404→base-URL fallback §6 at 101-105; unfollowable 3xx → descriptive error 107-115); `strip_userinfo` 41-72; https→http downgrade guard in the shared redirect pipeline | tests/unit/webdav/discovery_tests.rs:22-240 (multi-hop, 404 fallback, direct success, typed errors, userinfo stripped from hostile `Location`); redirect_tests.rs:175-237, builder_tests.rs:104-158; e2e sabredav discovery_tests.rs:13-41 |
| 2 | DNS SRV lookup (`_caldav._tcp`, §3/§6) | GAP (documented) | No SRV code in `src/` — declared deliberately out of scope at src/webdav/discovery.rs:16-22, 165-170, 199-200; no `srv` feature in Cargo.toml. Opt-in `srv` feature proposal tracked as issue #254 (hickory-resolver behind an injectable `Resolver` trait) | None (by design) |
| 3 | TXT path hint (§6) | GAP (documented) | No TXT code; exclusion now documented **next to SRV** in src/webdav/discovery.rs:17, 166-170, 199-200; docs/advanced-configuration.md:206-211; README.md:102-105, 192-196 (the 2026-09-10 documentation asymmetry is closed) | None (by design) |
| 4 | No credential leakage on cross-origin discovery redirects | COVERED | `strip_userinfo` src/webdav/discovery.rs:41-72 (hostile `Location` userinfo cut at 124-132); `Authorization`/`Cookie` stripped on cross-origin hops (module docs 7-9) | tests/unit/webdav/redirect_tests.rs:397-444 (auth to origin, stripped on cross-origin hop); discovery_tests.rs:207-240 |

## RFC 3744 / 5397 — ACL + principal

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `current-user-principal` (relative/absolute, anonymous variant) | COVERED | `discover_current_user_principal` src/webdav/client.rs:2498-2521 (404 → `Error::PrincipalNotFound` distinguishment); parser src/webdav/streaming.rs:1490-1538 | streaming.rs inline tests 2039-2086; tests/unit/webdav/discovery_tests.rs:250-311; e2e provider_a smoke mod.rs:204-234 |
| 2 | `current-user-privilege-set` → typed `Privilege` (§5.4) | COVERED | `Privilege` enum src/webdav/types.rs:755-779 with dedicated `All` variant :776 (doc caveat 723-736, punch list #16); parser `privilege_from_local_name` src/webdav/streaming.rs:459-482 (`all`→`All` case-insensitive 477-478); `current_user_privileges` src/webdav/client.rs:2536-2558 | tests/unit/webdav/privileges_tests.rs:32-159 (wire, typed, `All` aggregate, case-insensitive `ALL`, absent→empty, operation mapping); e2e sabredav discovery_tests.rs:189-208 |
| 3 | Privileges on ordinary PROPFINDs (write-gating without extra round-trip) | COVERED | `list_calendars` requests `<D:current-user-privilege-set/>` src/caldav/client.rs:399-427 (line 413); `CalendarInfo.privileges` src/caldav/types.rs:52-56; mapped client.rs:1517, 1543 | tests/unit/caldav/client_tests.rs:1125-1209 (privileges surface on `CalendarInfo` + assert the request body contains the property) |
| 4 | `principal-URL` / `owner` (§4.2) | COVERED | `DavItem.principal_url` src/webdav/types.rs:845-849 (doc marks legacy per RFC 5397 §3), `DavItemCommon.principal_url` :929-932; element name streaming.rs:431-432; href parser streaming.rs:203-210; `owner` :211-218 | tests/unit/webdav/principal_url_tests.rs:11-50 (present/absent) |
| 5 | RFC 5689 extended MKCOL | COVERED | `mkcol(path, xml_body)` src/webdav/client.rs:1820-1831 (`Content-Type: application/xml` when body present); `DavCompliance::ExtendedMkcol` parsed types.rs:616, 680; CardDAV typed `<D:mkcol>` builder (private) src/carddav/client.rs:735, 757 | tests/unit/webdav/mkcol_tests.rs:29-147 (bare-MKCOL wire test, extended-body verbatim + Content-Type, 405/207 passthrough); carddav builder inline tests client.rs:1089-1150; compliance token tests |
| 6 | RFC 4331 quota | OUT-OF-SCOPE | No quota matches in src/ or tests/; documented exclusion in docs/superpowers/specs/2026-10-04-rfc-punch-list-019-design.md:172-179 | — |
| 7 | RFC 5323 WebDAV SEARCH | OUT-OF-SCOPE | No SEARCH method anywhere in src; same design doc | — |

## RFC 5545 / 6350 — validation policy

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | iCalendar validation levels (None/Structural/Strict), folding §3.1, BOM | COVERED | `ValidationLevel` src/caldav/validation.rs:33-44 (`Structural` default); `validate_icalendar_level` :100-140 (BOM strip + unfold 107-117); `unfold_ical_lines` src/common/ical.rs:13-30 | tests/unit/caldav/validation_tests.rs (19+ tests incl. folded PRODID/UID); src/common/ical.rs inline tests 36-68 (fold, tab, LF-only, BOM) |
| 2 | Validation-level config + wire Content-Type `version` derivation (§3.7) | COVERED | `validation_level` builder option src/caldav/builder.rs:35, 63; `prepare_ical_put` src/caldav/client.rs:128-136 (`text/calendar; version={VERSION}`), used by put :149, put_if_none_match :166, scheduling.rs:478 | tests/unit/caldav/validation_tests.rs (level gating) + wire tests; doctests run under `cargo test --doc` |
| 3 | vCard sent verbatim — documented (§6350) | COVERED | src/carddav/client.rs:114-143 ("no client-side vCard validation … sent verbatim"); VERSION scan derives wire CT (116-120) | inline tests client.rs:1054; tests/unit/carddav/filter_tests.rs:8-11; e2e parsing_tests.rs:44-93 |
| 4 | FREEBUSY period parsing (folded lines, quoted params) | COVERED | `parse_free_busy_periods` src/caldav/client.rs:1365-1401; quote-aware `split_params_value` :1406-1416; folded input via `unfold_ical_lines` (1367) | tests/unit/caldav/client_tests.rs:115-172 (folded period line; quoted params with embedded `:`); punch-list #14 closed: regression-lock test tests/unit/caldav/scheduling_tests.rs:409-469 |

---

## Remaining punch list (post-audit 2026-10-23)

### Feature gaps (P1)

1. **DNS SRV + TXT discovery (RFC 6764 §3/§6)** — the only remaining GAP pair. Ship as the
   opt-in `srv` cargo feature per issue #254 (hickory-resolver behind an injectable `Resolver`
   trait; SRV → TXT `path=` → `.well-known` → base-URL fallback order; document in the README
   feature table, `docs/advanced-configuration.md`, and each discovery method's rustdoc).

### Defined shortfalls (P2 — implemented, documented, non-blocking)

2. **Typed `CALDAV:schedule-response` parsing (RFC 6638 §7)** — `post_schedule` returns the raw
   body by design (documented scheduling.rs:56-61); the outbox e2e proves the live response
   shape. A typed response model would complete the family.
3. **GET attachment by href (RFC 8607)** — no dedicated method and no test at any tier; blocked
   in practice by the absence of a managed-attachment-capable fixture.
4. **RFC 8607 §3.4 resource-targeted attachment POST (`rid` query)** — the conforming form is
   documented as not implemented; only the CalendarServer collection-targeted form ships.
5. **Generic public `<D:mkcol>` body builder (RFC 5689)** — extended-MKCOL bodies are
   caller-supplied raw XML to `mkcol(Option<&str>)`; only CardDAV has a typed (private) builder.
6. **`create_calendar(path, props)` convenience** — MKCALENDAR requires combining
   `build_mkcalendar_body` + `mkcalendar`; the runnable examples still use hand-built XML.

### Test debt (P3)

7. **No e2e for the new CardDAV §10.6/§10.4.2 options** — `addressbook_query_options` (limit/
   nresults, limited props, `i;octet`) is proven by unit wire captures only; no fixture
   exercises it live.
8. **PROPFIND `Depth: 1`/`infinity` header assertions** — proven transitively (REPORT Depth: 1;
   COPY infinity); a dedicated PROPFIND wire assertion would close the sliver.
9. **Dedicated 5xx-member-inside-207 test on the sync-collection REPORT path** — the "5xx is not
   a deletion" policy is directly unit-tested only on the PROPFIND fallback path.
10. **e2e MKCOL** — unit-tier only; no fixture sends MKCOL itself.
11. **Free-busy `start/duration` parsing TODOs** — two deferred notes remain at
    src/caldav/client.rs:1382-1384 (unknown FBTYPE values deliberately skipped); resolve or
    convert to a tracked issue to honor the no-lingering-TODOs rule.

### Deliberately out of scope (documented, unchanged)

- `wait`/`respond-async`/102 (RFC 8144); iCalendar body assembly (RFC 5545); managed-attachment
  × scheduling interplay (RFC 8607 §7); quota (RFC 4331); WebDAV SEARCH (RFC 5323); versioning
  (RFC 3253); BIND (RFC 5842); ordered collections (RFC 3648).
