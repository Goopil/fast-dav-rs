# RFC Coverage Audit — fast-dav-rs

- **Date:** 2026-09-10
- **Revision audited:** `5dd9e0a` (v0.17.0, branch main)
- **Method:** six parallel domain audits (one per RFC family), each requirement verified
  against the codebase with `file:line` evidence for both the implementation and its tests;
  every line number was re-read, not guessed. Statuses: **COVERED** (implementation + test
  proof), **PARTIAL** (implemented with a defined shortfall or untested surface), **GAP**
  (absent), **OUT-OF-SCOPE** (justified, server-only, or documented as out of scope).
- **Scope:** RFC 4918 (WebDAV 1+2), 6578 (Sync), 4791 (CalDAV), 6638 (scheduling),
  8607 (managed attachments, CalendarServer wire form), 6352 (CardDAV), 6764 (discovery),
  3744/5397 (ACL + principal), 8144 (Prefer), 5545/6350 (iCalendar/vCard policy).
- **Bilan:** 46 COVERED · 16 PARTIAL · 5 GAP · 4 OUT-OF-SCOPE. Punch list at the end;
  gaps and priorities are tracked in issue #225.

## Executive summary

| RFC | Domain | COVERED | PARTIAL | GAP | OUT-OF-SCOPE |
|-----|--------|---------|---------|-----|--------------|
| 4918 — WebDAV class 1/2 | 10 | 4 | 0 | 0 |
| 6578 — WebDAV Sync | 8 | 0 | 0 | 0 |
| 8144 — Prefer | 2 | 0 | 0 | 1 |
| 4791 — CalDAV | 9 | 3 | 1 | 0 |
| 6638 — Scheduling | 2 | 2 | 1 | 1 |
| 8607 — Managed attachments | 2 | 1 | 1 | 0 |
| 6352 — CardDAV | 6 | 2 | 0 | 0 |
| 6764 — Service discovery | 2 | 0 | 2 | 0 |
| 3744 + 5397 — ACL/principal | 5 | 4 | 0 | 2* |
| **Total** | **46** | **16** | **5** | **5** |

\* The final row also folds in RFC 5545/6350 validation (3 COVERED) and the quick checks
RFC 5689 (PARTIAL), 4331 and 5323 (both absent-by-design → OUT-OF-SCOPE).

The crate is **feature-complete for the everyday client surface**: PROPFIND/REPORT/207
parsing, locking, conditional writes, sync (including truncation, stale-token recovery and
the stateful session engine), scheduling discovery + outbox/inbox, multiget with href
reconciliation, filters with pre-I/O validation, and well-known discovery all have
implementation **and** test evidence. The remaining work is concentrated in five feature
gaps and a handful of ergonomic/API gaps listed in the punch list.

---

## RFC 4918 — WebDAV class 1/2

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | PROPFIND Depth 0/1/infinity | COVERED | `Depth` enum `src/webdav/types.rs:361-374`; header emission `src/webdav/client.rs:1528`, `:1944`, `:2335-2391` | `tests/unit/caldav/client_tests.rs:41-44`; `tests/unit/webdav/discovery_tests.rs:44-46`; `tests/unit/webdav/streaming_tests.rs:529` |
| 2 | PROPFIND body variants (specific/allprop/propname, §9.1) | PARTIAL | Body is caller-supplied XML `src/webdav/client.rs:1521-1526`; no `allprop`/`propname` helpers in `src/webdav/xml.rs` | indirect: `tests/unit/caldav/client_tests.rs:1016-1037` |
| 3 | 207 Multi-Status parsing (propstat + per-item statuses) | COVERED | `CommonParser` `src/webdav/streaming.rs:51-235`; `PropStat` `src/webdav/types.rs:432-445`; `DavItem` `src/webdav/types.rs:707-756` | `tests/unit/webdav/compliance_tests.rs:192,241,272,296`; `tests/unit/webdav/streaming_tests.rs:34,447,529` |
| 4 | PROPPATCH + 207 per-propstat evaluation (§14.23 404-remove-is-success) | COVERED | `src/webdav/client.rs:1547-1562`; `set_calendar_timezone` `src/caldav/client.rs:319-394` | `tests/unit/caldav/timezone_tests.rs:133,172,198,221,238`; e2e sabredav calendar_tests.rs:101 |
| 5 | MKCOL | PARTIAL | `src/webdav/client.rs:1588-1599` (optional XML body) | no direct test — only `tests/unit/webdav/retry_tests.rs:78-86` (non-idempotent classification) |
| 6 | GET/HEAD with ETag/Content-Type/Last-Modified | COVERED | `src/webdav/client.rs:1380-1389`; `etag_from_headers` `:170-176`; getcontenttype/getlastmodified `src/webdav/streaming.rs:211-224` | e2e sabredav event_tests.rs:94,132-145; `tests/unit/caldav/parser_tests.rs:165-188` |
| 7 | PUT conditionals If-Match/If-None-Match; `If` header for lock-token-guarded writes (§10.4) | PARTIAL | `if_match_header_value` `src/webdav/client.rs:71-109`; `put_if_match_with` `:1420-1435`; `put_if_none_match` `src/caldav/client.rs:159-170`; `If` built only in `refresh_lock` `:1826-1830` | `tests/unit/caldav/etag_tests.rs:104,130,157,201`; `tests/unit/webdav/redirect_tests.rs:447` |
| 8 | DELETE (+ conditional) | COVERED | `src/webdav/client.rs:1392-1395`, `delete_if_match` `:1411-1415` | `tests/unit/caldav/etag_tests.rs:104-198`; e2e contact_tests.rs:87 |
| 9 | COPY/MOVE with Destination + Overwrite (§9.8) | PARTIAL | `copy_move` `src/webdav/client.rs:1473-1518` (Destination `:1508-1511`, Overwrite `:1512-1515`); **no Depth parameter** (§9.8.3 shallow copy not expressible) | `tests/unit/webdav/uri_tests.rs:117-189`; e2e resource_tests.rs:88-123 |
| 10 | LOCK class 2 (lockdiscovery, Timeout §10.7, 423, coded-URL §10.5) | COVERED | `lock` `src/webdav/client.rs:1752-1785`; `lock_request` `:1668-1701`; clamp `:1650-1655`; `parse_lock_discovery_bytes` `src/webdav/streaming.rs:1487-1589` | `tests/unit/webdav/locking_tests.rs:36-715` (17 tests incl. clamp at `:238`); e2e sabredav locking_tests.rs:33 |
| 11 | UNLOCK with Lock-Token | COVERED | `src/webdav/client.rs:1851-1866` | `tests/unit/webdav/locking_tests.rs:426-557`; e2e sabredav + nextcloud |
| 12 | DAV header / compliance classes (§10.1) | COVERED | `capabilities()` `src/webdav/client.rs:1366-1377`; `parse_dav_header` `src/webdav/types.rs:495-510`; `DavCompliance` `:532-558` | `tests/unit/webdav/compliance_tests.rs:25-71,341-418`; e2e sabredav discovery_tests.rs:51-82 |
| 13 | `<D:error>` precondition parsing (§16/§14.12) | COVERED | `parse_error_body` `src/webdav/streaming.rs:1614-1702`; `status_error` → `Error::UnexpectedStatusWithDav` `src/webdav/client.rs:1612-1624` | `tests/unit/webdav/compliance_tests.rs:89-189` (8 tests) |
| 14 | Percent-encoding + href normalization | COVERED | `encode_path_segments` `src/webdav/client.rs:221-275`; `build_uri` `:697-742`; `normalize_href` `src/webdav/multiget.rs:136-175` | `tests/unit/webdav/uri_tests.rs:8-196`; `tests/unit/caldav/client_tests.rs:1621` |

## RFC 6578 — WebDAV Sync

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `sync-collection` REPORT body (Depth 0) | COVERED | `build_sync_collection_body` `src/webdav/xml.rs:128-163`; `src/webdav/client.rs:2189-2202` | `tests/unit/webdav/sync_tests.rs:41-72`; `tests/unit/caldav/client_tests.rs:884-889` |
| 2 | sync-token: initial-sync request; top-level + `Sync-Token` header (response chain) | COVERED | `src/webdav/xml.rs:141-147`; priority chain `src/webdav/types.rs:879-894` | `tests/unit/caldav/caldav_helpers.rs:303-384,673`; `tests/unit/webdav/sync_tests.rs:177-184` |
| 3 | `sync-level` 1/infinite | COVERED | `SyncLevel` `src/webdav/types.rs:36-52`; emission `src/webdav/xml.rs:148-150`; `sync_collection_with_level` `src/webdav/client.rs:2089-2109` | `tests/unit/webdav/sync_tests.rs:34-136` (wire capture) |
| 4 | `limit`/`nresults` + multi-page loop | COVERED | `src/webdav/xml.rs:156-160`; `SyncSession::sync_pages` `src/webdav/sync.rs:434-472`, dedup `:667-680` | `tests/unit/webdav/sync_session_tests.rs:803-853`; e2e sync_tests.rs:472 |
| 5 | Truncation: 507 → `truncated` (§3.6) | COVERED | `src/webdav/types.rs:897-900`; `SyncResponse.truncated` `src/caldav/types.rs:88-93`; non-continuable → `Error::SyncIncomplete` `src/webdav/sync.rs:461-470` | `tests/unit/caldav/client_tests.rs:892-948`; `tests/unit/webdav/sync_session_tests.rs:855-953`; e2e truncation_tests.rs:29-104 |
| 6 | Deletions: 404/410 → `is_deleted` (§3.5); 5xx ≠ deletion | COVERED | `src/webdav/types.rs:911`; unknown-status policy `src/webdav/sync.rs:690-695` | `tests/unit/caldav/caldav_helpers.rs:387-431,634-653`; `tests/unit/webdav/sync_session_tests.rs:236-380`; e2e sync_tests.rs:625 |
| 7 | Stale token: 410 / 403+`valid-sync-token` → resync; §3.4 no pre-existing deletions on initial sync | COVERED | `src/webdav/client.rs:2210-2240`; `diff_rows(replace=true)` `src/webdav/sync.rs:411-415,713-733` | `tests/unit/webdav/sync_tests.rs:138-301`; `tests/unit/webdav/sync_session_tests.rs:733-800`; e2e sabredav sync_tests.rs:791-881 |
| 8 | `SyncSession` stateful engine (persistence, incremental, 405 fallback, single-flight) | COVERED | `src/webdav/sync.rs:230-554` | `tests/unit/webdav/sync_session_tests.rs` (~1000 lines); `tests/unit/webdav/sync_capability_tests.rs:38-101`; e2e three fixtures |

## RFC 8144 — Prefer

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `Prefer: return=minimal/representation` emission (builder-wide + per-request) | COVERED | `Prefer` enum `src/webdav/types.rs:54-87`; injection `src/webdav/client.rs:1002-1009`; `put_if_match_prefer` `:2732-2751` | `tests/unit/webdav/prefer_tests.rs:9-256` (9 tests) |
| 2 | `Preference-Applied` parsing | COVERED | `preference_applied_from_headers` `src/webdav/client.rs:201-215` | `tests/unit/webdav/prefer_tests.rs:15-81` |
| 3 | `wait` / `respond-async` / 102 Processing | OUT-OF-SCOPE | documented: "v1.0 supports the `return` preference only" `src/webdav/types.rs:56-58`; manual-header escape hatch | negative test `prefer_tests.rs:60-61` |

## RFC 4791 — CalDAV

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | MKCALENDAR with calendar properties (§5.3.1) | PARTIAL | `src/caldav/client.rs:176-191` (raw XML body, Depth 0); no typed `<D:set>` builder | `tests/unit/caldav/client_tests.rs:984`; e2e nextcloud crud.rs:8-29 |
| 2 | `calendar-query` filters + pre-I/O DTD validation (§7.8, §9.7.x) | COVERED | `src/caldav/client.rs:555-631`; filter model `src/caldav/types.rs:140-350`; `build_calendar_query_body` `:1186-1223` | `tests/unit/caldav/filter_tests.rs:46-505`; `tests/unit/caldav/client_tests.rs:689-780,1773-1935` |
| 3 | `calendar-multiget` + href reconciliation (§7.9) | COVERED | `src/caldav/client.rs:651-677,755-785`; engine `src/webdav/multiget.rs:44` | `tests/unit/caldav/client_tests.rs:556-573,1375-1747` (9 tests) |
| 4 | `free-busy-query` + FBTYPE (§7.10) | COVERED | `src/caldav/client.rs:868-910,1109-1184` | `tests/unit/caldav/client_tests.rs:9-174`; in-module `client.rs:1372-1427`; e2e wave3_tests.rs:90-179 |
| 5 | `calendar-data` inline retrieval (§9.6) | COVERED | `src/webdav/streaming.rs:929-936` → `DavItem.calendar_data` `src/webdav/types.rs:725` | `tests/unit/caldav/parser_tests.rs:218`; `tests/unit/caldav/validation_tests.rs:328` |
| 6 | `<C:expand>` recurrence expansion (§9.6.5) | COVERED | `data_element_xml` `src/webdav/xml.rs:87-100`; `validate_expand` `src/caldav/client.rs:1081-1097` | `tests/unit/caldav/client_tests.rs:188-354`; e2e wave3_tests.rs:192-359 |
| 7 | `limit-recurrence-set` / `limit-freebusy-set` (§9.6.4) | **GAP** | zero occurrences in `src/` (grep) | none |
| 8 | `supported-calendar-component-set` (§5.2.3) | COVERED | `src/webdav/streaming.rs:758-785` → `CalendarInfo.supported_components` | `tests/unit/caldav/parser_tests.rs:94,147-153`; `tests/unit/caldav/parser_edge_cases.rs:144-215` |
| 9 | `calendar-timezone` read + PROPPATCH write (§5.2.2) | COVERED | `src/caldav/client.rs:254-394`; parser `src/webdav/streaming.rs:947-958` | `tests/unit/caldav/timezone_tests.rs:39-261` (10 tests) |
| 10 | `max-resource-size` / `supported-calendar-data` / `max-attendees-per-instance` (§5.2.x) | COVERED | `src/webdav/streaming.rs:396-403,812-830,996-1008` → `CalendarInfo` `src/caldav/types.rs:43-51` | `tests/unit/caldav/parser_tests.rs:26-91`; `tests/unit/caldav/client_tests.rs:1042,1103` |
| 11 | `calendar-home-set` discovery (§6.2.1) | COVERED | `discover_calendar_home_set` `src/caldav/client.rs:193-217` | `tests/unit/caldav/parser_tests.rs:142-145`; e2e sabredav discovery_tests.rs:131-150 |
| 12 | MKCOL path reachable from CalDAV client | PARTIAL | delegated via `impl_dav_client_delegates!` `src/webdav/client.rs:2806-2813` | no test exercises `mkcol` (only `mkcalendar`); see RFC 4918 #5 |
| 13 | iCalendar body assembly for PUT | PARTIAL (by design) | crate scopes out iCalendar parsing/assembly (`src/caldav/validation.rs:6-7`, `src/caldav/client.rs:282-284`); PUT bodies are caller-supplied, validated per `ValidationLevel` `src/caldav/validation.rs:31-44,100-213` | `tests/unit/caldav/validation_tests.rs:238-479` (13 tests) |

## RFC 6638 — Scheduling

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | schedule-inbox/outbox/calendar-user-address-set discovery (§2.1,§2.4) | COVERED | `discover_schedule_endpoints` `src/caldav/scheduling.rs:191-229`; parser `src/webdav/streaming.rs:1027-1052` | `tests/unit/caldav/scheduling_tests.rs:44`; e2e sabredav scheduling_tests.rs:19 |
| 2 | POST iTIP to outbox (§5) + scheduling-response parsing | PARTIAL | `post_schedule` `src/caldav/scheduling.rs:304-346` (Originator/Recipient headers, raw body passthrough); **no `CALDAV:schedule-response` XML parsing** (documented `:59-61`); no outbox e2e | `tests/unit/caldav/scheduling_tests.rs:85,129,159` |
| 3 | schedule-inbox listing (§2.2) | COVERED | `list_inbox` `src/caldav/scheduling.rs:383-416` | `tests/unit/caldav/scheduling_tests.rs:208,226`; e2e sabredav scheduling_tests.rs:60 |
| 4 | schedule-tag: `If-Schedule-Tag-Match` writes (§6.1,§10.1.1) | PARTIAL | emission `src/caldav/scheduling.rs:470-529` + header builder `:127-141`; cross-origin strip `src/webdav/client.rs:1210`; **no typed retrieval**: `Schedule-Tag` header not exposed via a helper, §10.1.1 `schedule-tag` property not parsed (no `ElementName`/`DavItem` field) | `tests/unit/caldav/scheduling_tests.rs:268-376`; `tests/unit/webdav/redirect_tests.rs:556-602` |
| 5 | calendar-proxy client features | **GAP** | only the compliance token is parsed (`DavCompliance::CalendarProxy` `src/webdav/types.rs:549-551,612`); no `ACL` method, no `calendar-proxy-read/write-for` properties, no proxy-group principal helpers | `tests/unit/webdav/compliance_tests.rs:341-367` (token only) |
| 6 | managed-attachments × scheduling interplay | OUT-OF-SCOPE | nothing implemented or claimed | — |

**calendar-proxy sub-feature inventory (for the future implementation):**
1. `calendar-proxy-read-for` / `calendar-proxy-write-for` principal properties — no
   `ElementName`, no `DavItem` field, no accessor today.
2. Proxy-group principal paths (`principals/<user>/calendar-proxy-read/`…) — no resolver.
3. Proxy grant/revoke — requires an `ACL` method (RFC 3744 §8.1), absent entirely.
4. Scheduling-as-proxy — combining 1-3 with `discover_schedule_endpoints`/`list_inbox` on
   the proxied principal; e2e coverage limited to the DAV-header token (live SabreDAV).

## RFC 8607 — Managed attachments (CalendarServer wire form)

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | POST `?action=attachment-add` + `Cal-Managed-ID` response | COVERED | `post_managed_attachment` `src/caldav/client.rs:987-1065` (Location-query fallback, typed `ManagedAttachment`) | `tests/unit/caldav/attachments_tests.rs:19-126` (5 tests) |
| 2 | GET attachment by href | PARTIAL | delegated generic `get` `src/webdav/client.rs:2642-2644`; `ManagedAttachment.href` documented as the target `src/caldav/types.rs:22-23` | **no test at any tier** |
| 3 | Replacement/removal (`attachment-update`, DELETE with `Cal-Managed-ID` request header) | **GAP** | absent — `attachment-update` nowhere in repo; `Cal-Managed-ID` only ever read from responses (`src/caldav/client.rs:1029`); escape hatch = public `send` (documented `src/caldav/types.rs:26-27`) | none |
| 4 | `managed-ids` property parsing (§5.1) | COVERED | `src/webdav/streaming.rs:1053-1058` → `DavItem.managed_ids` `src/webdav/types.rs:735-739` | `tests/unit/webdav/streaming_tests.rs:193-211` |

## RFC 6352 — CardDAV

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `addressbook-query` (filters, text-match+collations, pre-I/O DTD validation, §10.5) | PARTIAL | `src/carddav/client.rs:266-283,359-380`; `text_match_xml` `src/webdav/xml.rs:175-197`; `Collation`/`MatchType` `src/webdav/types.rs:152-201`. Missing: `limit`/`nresults` on addressbook-query (§10.6, sync-only today) and the `i;octet` collation | `tests/unit/carddav/filter_tests.rs` (30 tests); `tests/unit/carddav/client_tests.rs:1154,1181,1238`; e2e sabredav contact_tests.rs:60,142 |
| 2 | `addressbook-multiget` (§8.7) | COVERED | `src/carddav/client.rs:422-445`; builder `src/webdav/xml.rs:230-272` | `tests/unit/carddav/client_tests.rs:171-712`; e2e sabredav ×3 |
| 3 | `address-data` incl. limited-props form (§10.4) | PARTIAL | bare `<C:address-data/>` `src/carddav/client.rs:721-731,803-816`; **limited `<C:prop name="FN"/>` form not implementable** — `data_element_xml` `src/webdav/xml.rs:87-103` has no prop-list branch | bare: `tests/unit/carddav/client_tests.rs:227`; limited: none |
| 4 | `supported-address-data` (§6.2.2) | COVERED | `src/webdav/streaming.rs:459-490,786-811` → `AddressBookInfo.supported_address_data` | `tests/unit/webdav/streaming_tests.rs:167`; `tests/unit/carddav/parser_edge_cases.rs:144-222` |
| 5 | home-set / description / color discovery | COVERED | `discover_addressbook_home_set` `src/carddav/client.rs:212-235`; `list_addressbooks` `:239-251` | `tests/unit/carddav/client_tests.rs:530`; e2e nextcloud discovery.rs:14-40 |
| 6 | vCard verbatim (no validation) — documented | COVERED | `src/carddav/client.rs:122-125`; `README.md:79`; `docs/advanced-configuration.md:313-314` | `tests/unit/carddav/client_tests.rs:687,1068,1125` |
| 7 | chunked `addressbook_multiget_many` | COVERED | `src/carddav/client.rs:516-543` → shared engine | `tests/unit/carddav/client_tests.rs:773-1043` (7 tests) |
| 8 | Structured `CardDavFilter` → XML + validation | COVERED | `src/carddav/types.rs:13-107`; `src/webdav/types.rs:342-356` | `tests/unit/carddav/filter_tests.rs` (whole file) |

## RFC 6764 — Service discovery

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `.well-known/{caldav,carddav}` PROPFIND + redirects + https enforcement (§5,§6) | COVERED | `discover_well_known` `src/webdav/discovery.rs:72-129`; downgrade guard `src/webdav/client.rs:1187-1193`; `require_https` `:1181-1185` | `tests/unit/webdav/discovery_tests.rs:22-196`; `tests/unit/webdav/redirect_tests.rs:175-208`; e2e three fixtures |
| 2 | DNS SRV lookup (`_caldav._tcp`, §3/§6) | **GAP (documented)** | `src/webdav/discovery.rs:16-17` and `:160`; `docs/advanced-configuration.md:206` — "DNS SRV record lookup … is not implemented" | none (by design) |
| 3 | TXT path hint (§3) | **GAP (undocumented)** | no code, no doc mention — documentation asymmetry vs SRV | none |
| 4 | No credential leakage on cross-origin discovery redirects | COVERED | `src/webdav/client.rs:1025-1035,1195-1211`; `strip_userinfo` `src/webdav/discovery.rs:36-67` | `tests/unit/webdav/discovery_tests.rs:207`; `tests/unit/webdav/redirect_tests.rs:398,447,524,556` |

## RFC 3744 / 5397 — ACL basics + principal

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | `current-user-principal` (relative/absolute, anonymous variant) | COVERED | `discover_current_user_principal` `src/webdav/client.rs:2261-2284`; parser `src/webdav/streaming.rs:1377-1425` | `tests/unit/webdav/streaming_tests.rs:133-164`; `tests/unit/webdav/discovery_tests.rs:250-313`; e2e four fixtures |
| 2 | `current-user-privilege-set` → typed `Privilege` (§5.4) | PARTIAL | `current_user_privileges` `src/webdav/client.rs:2299-2320`; `Privilege` enum `src/webdav/types.rs:679-698`; missing dedicated variants for aggregate `all`, `read-acl`, `read-current-user-privilege-set` (surface as `Privilege::Other`) | `tests/unit/webdav/privileges_tests.rs:32-174` (7 tests) |
| 3 | Privileges on ordinary PROPFINDs (write-gating without extra round-trip) | PARTIAL | parser populates `DavItem.current_user_privileges` on any PROPFIND `src/webdav/streaming.rs:835-858`; **`list_calendars` body does not request the property** `src/caldav/client.rs:398-414` | `tests/unit/webdav/privileges_tests.rs:143-165`; e2e sabredav discovery_tests.rs:189-207 |
| 4 | `principal-URL` / `owner` (§4.2) | PARTIAL | `D:owner` parsed `src/webdav/streaming.rs:203-210` → `DavItem.owner`; **`principal-URL` property not parsed** (crate relies on RFC 5397 property) | `tests/unit/caldav/parser_tests.rs:209`; `tests/unit/carddav/parser_tests.rs:120` |
| 5 | RFC 5689 extended-MKCOL | PARTIAL | token parsed `src/webdav/types.rs:547,611`; `mkcol` accepts XML body `src/webdav/client.rs:1588-1599`; no WebDAV-level `<D:mkcol>` body builder, no direct test | compliance token test; CardDAV MKCOL fallback builder tests `src/carddav/client.rs:950-1011` |
| 6 | RFC 4331 quota | OUT-OF-SCOPE | absent by design | — |
| 7 | RFC 5323 WebDAV SEARCH | OUT-OF-SCOPE | absent by design | — |

## RFC 5545 / 6350 — validation policy

| # | Requirement | Status | Implementation evidence | Test evidence |
|---|-------------|--------|-------------------------|---------------|
| 1 | iCalendar validation levels (None/Structural/Strict), folding §3.1, BOM | COVERED | `src/caldav/validation.rs:31-44,100-213`; `src/common/ical.rs:13-30` | `tests/unit/caldav/validation_tests.rs` (479 lines, 13+ tests); `src/common/ical.rs:37-68` |
| 2 | Validation-level config + wire Content-Type `version` derivation (§3.7) | COVERED | `src/caldav/builder.rs:35,63-66`; `prepare_ical_put` `src/caldav/client.rs:126-134` | `tests/unit/caldav/validation_tests.rs:238-479` |
| 3 | vCard sent verbatim — documented (§6350) | COVERED | `src/carddav/client.rs:114-125`; README + guides quoted in the RFC 6352 table | `tests/unit/carddav/client_tests.rs:1109-1149` |
| 4 | FREEBUSY period parsing (folded lines, quoted params) | COVERED | `parse_free_busy_periods` `src/caldav/client.rs:1109-1145`; `split_params_value` `:1150-1160` (quote-aware, untested in isolation) | `src/caldav/client.rs:1373-1428`; folding `src/common/ical.rs:37-68` |

---

## Punch list (feeds issue #225)

### P1 — feature gaps

1. **calendar-proxy** (RFC 6638 companion) — the only user-requested feature: proxy
   `read-for`/`write-for` principal properties, proxy-group principal resolution, and the
   `ACL` method (RFC 3744 §8.1) for grant/revoke. Largest item; depends on an `ACL`
   primitive.
2. **Managed-attachment lifecycle** (RFC 8607): `attachment-update`/removal with
   `Cal-Managed-ID` as a request header + a test for attachment GET by href.
3. **schedule-tag retrieval** (RFC 6638): `Schedule-Tag` header helper (like
   `etag_from_headers`) + parse the §10.1.1 `schedule-tag` property into `DavItem`.
4. **`limit-recurrence-set` / `limit-freebusy-set`** (RFC 4791 §9.6.4): body-builder
   support in `data_element_xml`, paired with the existing `expand`.
5. **`address-data` limited-props form + `limit`/`nresults` on addressbook-query**
   (RFC 6352 §10.4/§10.6) and the `i;octet` collation.
6. **DNS SRV lookup** (RFC 6764 §3): decide in-scope (new dependency, e.g. `hickory-resolver`)
   or promote the existing "not implemented" note to the TXT section too.

### P2 — API ergonomics / small RFC bites

7. Typed `412 Precondition Failed` / `428 Precondition Required` handling on conditional
   writes; a public helper to build the RFC 4918 §10.4 `If` header for lock-token-guarded
   PUT/DELETE.
8. PROPFIND body helpers (`allprop` / `propname` / typed prop list) in `src/webdav/xml.rs`.
9. `Depth` parameter on COPY/MOVE (§9.8.3 shallow copy).
10. Typed MKCALENDAR / MKCOL property builders (displayname, description,
    supported-component-set) replacing hand-built XML.
11. `list_calendars` requesting `current-user-privilege-set` so `CalendarInfo` carries
    privileges (removes the second round-trip; behavior change → document).
12. Dedicated `Privilege::All` variant (aggregate `all`) so write-gating matches servers
    granting `all` instead of `write`.

### P3 — test/documentation debt

13. `mkcol` wire test (RFC 4918 #5 / 4791 #12) + `mkcol`-with-body test (RFC 5689).
14. Outbox POST e2e (no fixture exercises it) + `split_params_value` unit test.
15. Document the TXT-record silence next to the SRV disclaimer (RFC 6764 asymmetry).
16. Document the `Privilege::Other("all")` mapping caveat in the `Privilege` doc comment.
17. `principal-URL` legacy property (RFC 3744 §4.2): parse or document the RFC 5397
    reliance explicitly.

### Deliberately out of scope (documented)

- `wait`/`respond-async`/102 (RFC 8144) — documented manual-header escape hatch.
- iCalendar body assembly (RFC 5545) — the crate validates but never constructs/parses
  iCalendar; deliberate scope, documented at `src/caldav/validation.rs:6-7`.
- Managed-attachment × scheduling interplay; quota (RFC 4331); WebDAV SEARCH (RFC 5323);
  versioning (RFC 3253), BIND (RFC 5842), ordered collections (RFC 3648).
