# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Optional `tracing` instrumentation behind the new `tracing` feature
  (issue #91, audit AUDIT-010): with `features = ["tracing"]`, the shared
  request pipeline emits `tracing` events at `debug` level — request start and
  finish (method, URI, status, duration), redirect hops, transient retries
  (status + computed delay), exhausted retry budget, per-request timeout, and
  compression-probe outcome + negotiated encoding — and the decompressed
  response body size at `trace` level. Zero-cost when disabled: the feature is
  off by default and compiles out every `tracing` reference (no dependency, no
  binary-size or compile-time cost). No public API changes; the `tracing`
  crate is not re-exported
- Custom hyper client injection (issue #91): new `with_hyper_client(HyperClient)`
  builder method on `WebDavClientBuilder`, `CalDavClientBuilder`, and
  `CardDavClientBuilder`. When a client is injected, the builder skips its own
  transport construction entirely — `force_http1`, pool, TLS, and proxy options
  are not applied (the caller owns the transport); request-level options (auth,
  timeout, compression, redirects, `Prefer`, retries) still apply. The
  `webdav::HyperClient` type alias and the `common::http::MaybeProxied`
  connector are now public (`MaybeProxied` is `#[non_exhaustive]`, with a
  `MaybeProxied::direct(HttpConnector)` constructor) so users can build a
  client of the exact expected shape
- RFC 6764 §5 auto-discovery (issue #91): free functions `discover_caldav` and
  `discover_carddav` (taking `&WebDavClient`, re-exported from `fast_dav_rs::webdav` and the
  crate root) probe `{base}/.well-known/caldav` or `/.well-known/carddav` with a `Depth: 0`
  PROPFIND requesting `DAV:current-user-principal` (RFC 6764 §6). Redirects are followed by
  the client's redirect pipeline and the final request URL is returned as the discovered
  service URL; `404` (or a success answered directly on the `.well-known` URI) falls back to
  the base URL unchanged; any other non-success status fails with `Error::UnexpectedStatus`
  (new `Operation::DiscoverWellKnownCaldav` / `Operation::DiscoverWellKnownCarddav` variants).
  Client credentials are attached to the probe and stripped on cross-origin redirect hops.
  DNS SRV record lookup (RFC 6764 §3) is not implemented (would require a DNS resolver
  dependency; deferred)
- WebDAV locking (RFC 4918 class 2, issue #90): `lock` (LOCK with a `Timeout: Second-N`
  header and a `<D:lockinfo>` body), `refresh_lock` (LOCK re-issued without a body with the
  lock token in an `If` header, RFC 4918 §9.10.7), and `unlock` (UNLOCK with the token in a
  `Lock-Token` header) on `WebDavClient`, `CalDavClient`, and `CardDavClient`. New
  `#[non_exhaustive]` types `webdav::LockInfo` (parsed `<D:activelock>`: token, timeout,
  scope, owner) and `webdav::LockScope` (`Exclusive`/`Shared`), re-exported from
  `fast_dav_rs::webdav` and the crate root, plus the public `webdav::parse_lock_discovery_bytes`
  helper for `PROPFIND lockdiscovery` bodies. Non-success statuses — including `423 Locked` —
  surface as `Error::UnexpectedStatus` with the new `Operation::Lock`/`Operation::Unlock`
  variants. The client keeps no implicit lock state: callers pass the token where needed.
- `CalDavClient::calendar_multiget_many` (issue #105): batched concurrent
  `calendar-multiget` — chunks the href list into `batch_size` slices, issues one
  REPORT per chunk with `max_concurrency`-bounded parallelism (same
  `Semaphore` + ordered futures machinery as `WebDavClient::report_many`),
  and returns `Vec<BatchItem<CalendarObject>>` with deterministic ordering
  (chunk index, then server order within the chunk). A failed chunk (transport
  error, non-success status, unparsable body) yields exactly one error
  `BatchItem`; sibling chunks are unaffected. `batch_size == 0` fails with
  `Error::InvalidConfig` and empty `hrefs` returns an empty result — both
  before any network I/O

- Retry with exponential backoff for transient failures (issue #91, audit finding H8):
  new `max_retries(usize)` and `retry_all(bool)` builder options on `WebDavClientBuilder`,
  `CalDavClientBuilder`, and `CardDavClientBuilder` (defaults `0` / `false` — retrying
  disabled, the previous behavior). When enabled, `429`, `503`, and `504` responses are
  retried in the shared request pipeline: a `429` honors the server's `Retry-After` header
  (integer seconds or HTTP-date; absent → exponential backoff), while `503`/`504` always
  use an exponential backoff (base 2, initial ~250 ms, doubling per attempt, capped at
  ~8 s) with ±25 % jitter (no new dependency). By default only idempotent methods (`GET`,
  `HEAD`, `OPTIONS`, `PROPFIND`, `REPORT`) are retried; `retry_all(true)` extends retrying
  to every method. The retry budget counts every HTTP attempt across the whole redirect
  chain (total attempts = `1 + max_retries`), each attempt runs under the same per-request
  timeout, and exhausted retries return the last response as-is (no synthetic error).
  Compression-retry semantics are unchanged.

### Changed
- e2e gate for the v1.1 APIs (issue #124): new e2e tests against the live
  SabreDAV fixture cover WebDAV locking (lock/refresh/unlock lifecycle, 423
  enforcement, re-lock), RFC 6764 discovery over a real well-known redirect
  (the fixture's SabreDAV instance now answers `301 /principals/test/` on
  `/.well-known/caldav` and `/carddav` via a `beforeMethod:*` handler — the
  documented SabreDAV pattern, since sabre/dav ships no well-known support —
  and the tests assert the final post-redirect service URI),
  `calendar_multiget_many` (batched retrieval + ordering),
  the Auto request-compression probe on a real PUT (AUDIT-012),
  `with_hyper_client` injection on a live PROPFIND (AUDIT-017), and the
  `truncated == false` sync regression (AUDIT-015). The fixture now serves the
  SabreDAV `Locks` plugin with the PDO locks backend (class 2) — a `locks`
  table was added to `sabredav-test/sql/init.sql` (additive; fresh CI
  environments pick it up automatically). Soft asserts in the parallel e2e
  tests (AUDIT-017) were hardened to panics
- **Breaking:** `SyncItem`, `SyncResponse`, `build_sync_collection_body`, and
  `map_sync_response` are no longer re-exported from the crate root. CalDAV and
  CardDAV define distinct same-named types/helpers, and the root previously bound
  only the CalDAV versions — a CardDAV user importing `fast_dav_rs::SyncResponse`
  silently got the wrong type (AUDIT-014). Import them from their modules instead:
  `fast_dav_rs::caldav::{SyncItem, SyncResponse, build_sync_collection_body,
  map_sync_response}` or `fast_dav_rs::carddav::{SyncItem, SyncResponse, …}`. The
  shared WebDAV types (`DavItem`, `BatchItem`, `Depth`, `TextMatch`, `Collation`,
  `MatchType`, `ParamFilter`) are unchanged — a single definition re-exported by
  both modules.
- New `truncated: bool` field on `caldav::SyncResponse` and `carddav::SyncResponse`
  (AUDIT-015): `true` when the server truncated the sync result set (RFC 6578 §3.6 —
  a `507 Insufficient Storage` status inside the 207 multistatus, normally on the
  request-URI, which still surfaces in `items` with its per-item status). The returned
  `sync_token` remains valid for fetching the next page of changes. Additive but
  constructor-visible: struct literals must add the field (prefer `..Default::default()`
  or update them)
- **Breaking (0.11 window):** `BatchItem` (returned by `propfind_many`,
  `report_many`, `calendar_multiget_many`) gains a `pub hrefs: Vec<String>` field
  carrying the request hrefs of the batch — the single request path for
  `propfind_many`/`report_many`, the chunk's requested hrefs for
  `calendar_multiget_many` chunks — so a failed multiget chunk is attributable to
  the hrefs it should re-fetch (issue #140). `BatchItem` is `#[non_exhaustive]`, so
  external construction was already impossible; only exhaustive internal pattern
  matches are affected
- **Breaking (0.11 window):** `WebDavClient::sync_collection_resilient` and the
  crate-internal `sync_collection_resilient_report` now return a 4-tuple
  `(HeaderMap, Vec<DavItem>, Option<String>, bool)` whose last element (`resynced`)
  is `true` when the result came from an initial sync triggered by a stale sync
  token; `caldav::SyncResponse` and `carddav::SyncResponse` gain a `pub resynced:
  bool` field (always `false` for incremental syncs). Per RFC 6578 §3.4 an initial
  sync MUST NOT report deletions that predate the stale token, so callers must
  rebuild their caches when `resynced == true` (issue #140). The stale-token signal
  is now also recognized as `403 Forbidden` + `valid-sync-token` (RFC 6578 §3.2
  alternative signal), not only `410 Gone`
- New `#[non_exhaustive]` error variant `Error::UnexpectedStatusWithDav {
  operation, status, dav }` (issue #136): LOCK/UNLOCK error responses with a
  `<D:error>` body (e.g. `423 Locked` + `<D:no-conflicting-lock/>`, RFC 4918 §16)
  now carry the parsed precondition (`dav.precondition_code`); bodies without a
  `<D:error>` element keep surfacing as the unchanged `Error::UnexpectedStatus`.
  `webdav::WebDavError` gains a `Display` impl used in the variant's message
- `webdav::LockInfo` (issue #136) gains `lockroot: Option<String>` (text of the
  REQUIRED `<D:lockroot><D:href>`, RFC 4918 §14.2) and `depth: Option<Depth>`
  (from `<D:depth>`, RFC 4918 §14.3), parsed by `webdav::parse_lock_discovery_bytes`;
  `webdav::Depth` gains `Debug`/`PartialEq`/`Eq` to support the new field
- CalDAV `text-match` serialization is now protocol-correct (RFC 4791 §9.7.5):
  CalDAV `prop-filter`/`param-filter` children no longer emit `match-type` (the
  attribute does not exist in the CalDAV DTD) and omit the `collation` attribute
  when it is the CalDAV default `i;ascii-casemap` (an explicitly selected
  non-default collation is still sent). CardDAV serialization is unchanged
  (`match-type` and `collation` always present, RFC 6352 §10.4). The
  `Collation`/`MatchType` enums and their defaults are unchanged

### Fixed
- `send` no longer feeds empty response bodies to a decompressor (issue #142):
  a conforming `HEAD` response may carry `Content-Encoding` with an empty body
  (RFC 9110 §9.3.2), which previously failed with a decoder error; empty
  bodies (`HEAD`, `204`, `304`) are now returned as-is with their headers
  untouched (no `Content-Length` rewrite)
- A caller-supplied `Content-Encoding` on a request is now honored as-is
  (issue #142): the body is forwarded verbatim and automatic compression (and
  its probe) is skipped. Previously the header was stripped and the body
  re-compressed on top of the caller's encoding — silent double encoding
  behind a 2xx. Documented on `send`/`send_stream`
- The request-compression probe pins `Identity` when the base URL answers it
  with a redirect (issue #142): a 3xx is a stable property of the deployment,
  not a transient failure, so the probe no longer re-runs (and fails) before
  every body-carrying request
- The request-compression probe caches only proven encodings (issue #142):
  gzip is kept when the server's advertised `Accept-Encoding` preference names
  it; anything else (`br`/`zstd` picks included) caches `Identity`, so later
  PUTs cannot fail with `415` on an encoding the server never accepted
- An unrelated `400` no longer permanently disables request compression
  (issue #142): only compression-specific rejections (`415`, and `501`)
  pin `Identity` — a `400` can come from a malformed body and previously
  silenced compression for the client's lifetime
- The request-compression probe sends the configured `User-Agent`
  (issue #142, RFC 9110 §10.1.5), so User-Agent-aware servers do not treat it
  differently from real requests
- Documented on `send_stream`/`propfind_stream`/`report_stream` (and their
  CalDAV/CardDAV delegate copies) that streaming bodies may still be encoded
  (issue #142): the client advertises `Accept-Encoding` (RFC 9110 §12.5.3) but
  leaves response decoding to the caller — check `Content-Encoding` (e.g.
  `detect_encoding`) and wrap the body before parsing
- Silent sync-data loss (issue #140): the shared `sync-collection` mapping
  (CalDAV/CardDAV `map_sync_response`) consumed the data payload twice when a
  response element carried a per-item sync token and no etag — the taking closures
  (`calendar_data.take()` / `address_data.take()`) emptied the payload on the first
  call, so such members were delivered **without their data**. The payload is now
  computed exactly once and survives
- Multiget REPORTs are now sent with `Depth: 0` (issue #140): the batched multiget
  (`report_many_bodies`, used by `calendar_multiget_many`) and the single-request
  `calendar_multiget` / `addressbook_multiget` previously sent `Depth: 1`, which
  servers SHOULD NOT receive for multiget REPORTs (RFC 4791 §7.9, RFC 6352 §8.7)
- `Retry-After` handling on transient retries (issue #137): the honored delay
  is now clamped to the retry backoff cap (a hostile or overloaded server
  answering `429` with a huge `Retry-After` could previously park the request
  future indefinitely, holding its batch semaphore permit), and the header is
  now also honored on `503`/`504` (its canonical carriers per RFC 9110
  §10.2.1), not only on `429`
- **RFC 4791 request-XML validity (issue #138):** `calendar_query_timerange`,
  `calendar_multiget`, `calendar_multiget_many`, and the CalDAV `sync_collection`
  now reject an `expand` without an `end` **before any network I/O** with
  `Error::InvalidInput` — RFC 4791 §9.6.5 makes both `start` and `end`
  `#REQUIRED` attributes of `<C:expand>`, so the previously emitted start-only
  expand was invalid against conforming servers. Wherever expand/time-range
  `start`+`end` are both set, `end <= start` is rejected with
  `Error::InvalidDateTime` ("end must be after start", RFC 4791 §9.9) — also on
  `calendar_query` time-ranges and `free_busy_query`
- **RFC 4791 §9.7.2 / RFC 6352 §10.5.1 filter exclusivity (issue #138):**
  `CalDavClient::calendar_query` rejects `prop-filter`s violating the child
  exclusivity DTD before any network I/O with `Error::InvalidInput`
  (`is-not-defined` excludes `text-match`, `time-range`, and `param-filter`;
  `text-match` and `time-range` are mutually exclusive). `PropFilter::to_xml`
  and `CardDavFilter::to_filter_xml` enforce the same exclusivity by
  serialization precedence (`is-not-defined` alone; `text-match` wins over
  `time-range`), so direct `to_xml` callers can no longer emit invalid XML
- **Behavior change:** `CalDavClient::calendar_query` with a
  `prop-filter` that sets `is_not_defined` together with a `text-match`,
  `time-range`, or `param-filter` — or both `text_match` and `time_range` —
  now fails with `Error::InvalidInput` before any network I/O instead of
  sending a request conforming servers must reject
- CardDAV `put`/`put_if_none_match` no longer hardcode `version=4.0` in the
  `Content-Type` (issue #138): the version parameter is derived from the
  body's `VERSION` property (case-insensitive simple line scan, e.g. a vCard
  3.0 body is sent as `text/vcard; charset=utf-8; version=3.0`), falling back
  to `version=4.0` when the body declares none or a malformed one — a lying
  `Content-Type` could make `valid-address-data` reject the write
  (RFC 6352 §5.3.2.1)
- **Behavior change:** weak ETags (`W/"abc"`) are now rejected client-side by
  `put_if_match`, `put_if_match_prefer`, and `delete_if_match` (AUDIT-008) with
  `Error::InvalidEtag` and the new `EtagReason::Weak`, **before any network
  I/O**. Previously a weak tag was accepted and sent as `If-Match: W/"abc"`;
  because RFC 9110 mandates strong comparison for `If-Match`, weak validators
  never match, so the server was guaranteed to answer `412 Precondition
  Failed` — a permanently broken write path for servers that issue weak ETags.
  Weak ETags remain accepted on informational paths (`etag_from_headers`,
  `normalize_etag`)
- The request-compression probe (AUDIT-012) no longer permanently pins `Identity`
  after a transient failure: a failed probe (transport error, timeout, non-success
  status) leaves the negotiation state unset so the next body-carrying request
  re-probes, while the current request proceeds uncompressed. A completed probe
   still caches the server's answer — including `Identity` when the server
   advertises no compression support. The probe timeout now derives from the
   client's `timeout(...)` setting instead of a hardcoded 5 s
- `webdav::parse_error_body` (AUDIT-015) no longer silently returns a default
  `WebDavError` for a malformed `<D:error>` body: `webdav::WebDavError` gains a
  `parse_failed: bool` flag (`#[non_exhaustive]` struct), set when an error body was
  present but could not be parsed as XML — a hostile server can no longer suppress
  precondition diagnostics with garbage markup (`parse_failed == false, precondition_code == None`
  remains a well-formed response with no error body)
- Locking conformance (RFC 4918 class 2, issue #136): `lock` now sends an explicit
  `Depth: 0` header (previously omitted, which defaults to `Depth: infinity` per
  §9.10.4 — locking a collection silently locked its whole subtree); `Timeout: Second-N`
  values are clamped to `u32::MAX` seconds (§10.7); a successful `LOCK` response without
  a lock token fails with `Error::InvalidInput` (§9.10.9) instead of returning an empty
  token; `refresh_lock` falls back to the request token when the refreshed activelock
  omits `<D:locktoken>` (§9.10.2); lock tokens are validated before being embedded in a
  Coded-URL `If`/`Lock-Token` header (§10.5)
- LOCK/UNLOCK error responses carrying a `<D:error>` body now surface the failed
  precondition instead of dropping it (issue #136, RFC 4918 §16): see the new
  `Error::UnexpectedStatusWithDav` variant under Changed
- RFC 3986-conformant redirect resolution and URI handling (issue #139):
  `Location` references are now normalized with the RFC 3986 §5.2.4
  `remove_dot_segments` algorithm (`../caldav/` against `/.well-known/caldav`
  resolves to `/caldav/` instead of the literal `/.well-known/../caldav/`);
  network-path references (`Location: //mirror/dav/`, RFC 3986 §4.2) are
  resolved scheme-relatively instead of being requested as a garbage path
  from the current host; absolute schemes are matched case-insensitively
  (RFC 3986 §3.1, `HTTPS://…` is absolute and `Uri` canonicalizes the
  scheme); `same_origin` compares hosts ASCII-case-insensitively (RFC 3986
  §3.2.2), so a re-cased host no longer triggers needless credential
  stripping; cross-origin redirects additionally strip `If-Match` and
  `If-None-Match` (RFC 9110 §13.1.1) alongside `Authorization`/`Cookie`;
  and the WebDAV `Destination` header (`copy`/`move`) is validated as an
  absolute URI with scheme and authority (RFC 4918 §10.3 Simple-ref) before
  any network I/O — `Error::InvalidInput` otherwise; the value must already
  be percent-encoded by the caller and is sent verbatim
- **Behavior change:** an `https`→`http` redirect downgrade is never followed
  (issue #139; RFC 6764 §6 is TLS-first): the 3xx response is returned as-is
  so the caller can observe the redirect, instead of silently re-sending the
  request — body included — over plaintext
- `build_uri`/`encode_path_segments` now encode `?` and `#` inside resource
  names (`%3F`/`%23`), so a literal `?` can no longer change resource identity
  by acting as a query separator (issue #139); a query string is not part of
  the path contract, and already-valid `%XX` escapes keep passing through
  verbatim (documented loudly: pre-encoded input addresses the resource named
  by its encoded form)
- Service discovery (`discover_caldav`/`discover_carddav`) docs no longer
  claim redirects are always followed: with `follow_redirects(false)` the
  probe returns the 3xx and discovery fails with a descriptive error naming
  the cause (RFC 6764 §5 requires clients to handle `.well-known` redirects,
  so leave the builder default enabled) (issue #139)

### Security
- Userinfo in base URLs is now rejected (issue #141): a base URL like
  `https://user:password@dav.example.com/` was previously accepted even though
  the userinfo was never converted to Basic auth (inexplicable 401s) and was
  echoed verbatim into `Error::InvalidUrl` messages and, with the `tracing`
  feature, into every debug-level request log line (RFC 9110 §3.2 — senders
  MUST NOT generate userinfo). `WebDavClientBuilder::build` (and therefore the
  CalDAV and CardDAV builders, which delegate to it) now fails with
  `Error::InvalidConfig` before any I/O when the base URL carries userinfo;
  pass credentials via `basic_auth`/`bearer_token` instead. As belt-and-braces,
  userinfo is redacted to `***` wherever a URL is echoed: `Error::InvalidUrl`
  values and the `tracing` request/redirect log fields never contain
  credentials, even for URIs obtained from a remote server (e.g. redirect
  targets)

## [0.10.0] - 2026-09-01

### Added
- iCalendar validation before CalDAV `PUT` (issue #89): `put`, `put_if_match`, and
  `put_if_none_match` on `CalDavClient` now validate the body client-side **before any
  network I/O**. New API: the pure function `caldav::validate_icalendar` (seven structural
  checks — valid UTF-8, `BEGIN:VCALENDAR` at start, `END:VCALENDAR` at end, `VERSION:2.0`,
  `PRODID`, balanced `BEGIN`/`END` pairs, and a `UID` in every `VEVENT`/`VTODO`), the
  `#[non_exhaustive]` `caldav::ValidationLevel` enum (`None` / `Structural` / `Strict`),
  and the `validation_level(...)` builder option on `CalDavClientBuilder` (default
  `Structural`; `Structural` runs checks 1–6, `Strict` adds the `UID` check). When
  validation is enabled and the body declares a `VERSION`, the wire `Content-Type`
  becomes `text/calendar; charset=utf-8; version=<declared>`. **Behavior change:** with
  the default `Structural` level, structurally invalid bodies now fail client-side with
  the new `Error::InvalidICalendar` variant (carrying an `ICalendarViolation`) before any
  request is sent; set `ValidationLevel::None` for the previous behavior. CardDAV (vCard)
  requests are never validated as iCalendar
- `SyncLevel` enum (`One` / `Infinite`, RFC 6578 §3.3), re-exported from `fast_dav_rs::webdav`
  and the crate root, and `sync_collection_with_level` on `WebDavClient`, `CalDavClient`, and
  `CardDavClient` — a `sync-collection` REPORT with a configurable `sync-level` (the existing
  `sync_collection` keeps the `SyncLevel::One` behavior) (issue #88)
- `sync_collection_resilient` on `WebDavClient`, `CalDavClient`, and `CardDavClient` (issue #88):
  410-Gone recovery per RFC 6578 §3.11 — when the server rejects the incremental request with
  `410 Gone` (stale sync token), the report is automatically re-issued with an empty sync token
  (initial sync) and the full result set with the new token is returned; any other error
  propagates unchanged
- `Prefer` header support (RFC 7240, issue #87): new `Prefer` enum (`Minimal` /
  `Representation`, re-exported from `fast_dav_rs::webdav` and the crate root), a
  `prefer(Option<Prefer>)` builder option on `WebDavClientBuilder`, `CalDavClientBuilder`,
  and `CardDavClientBuilder` that injects the header on every request, lenient
  `Preference-Applied` response parsing via `preference_applied_from_headers` (absent,
  malformed, or unknown values yield `None`, never an error), and `put_if_match_prefer` on
  `CalDavClient`/`CardDavClient` — a conditional `PUT` with `Prefer: return=representation`
- `follow_redirects` (default `true`) and `max_redirects` (default `5`) builder options on
  `WebDavClientBuilder`, `CalDavClientBuilder`, and `CardDavClientBuilder`: HTTP redirects
  (301/302/303/307/308) are now followed in `send`/`send_stream`. On 303 the request is re-sent
  as `GET` without a body; when a redirect crosses origins the `Authorization` and `Cookie`
  headers are stripped for the remainder of the chain. Exceeding the limit fails with the new
  `Error::TooManyRedirects` variant
- `CalDavClient::free_busy_query` — `free-busy-query` REPORT (RFC 4791 §9.7, sent with `Depth: 1`)
  returning parsed `FreeBusyPeriod`s; the `FBTYPE` parameter maps to `FreeBusyType` (default
  `BUSY` when absent), unrecognized values and `start/duration` periods are skipped
- Optional `expand: Option<TimeRange>` parameter on `calendar_query_timerange`, `calendar_multiget`,
  and the CalDAV `sync_collection` for server-side recurrence expansion (RFC 4791 §9.6); when
  `expand` is `Some`, `include_data` is implied `true`. New types `FreeBusyPeriod` and
  `FreeBusyType`, re-exported from `fast_dav_rs::caldav`
- `CalendarInfo` now exposes the CalDAV collection properties `max_resource_size` (`Option<u64>`,
  RFC 4791 §5.2.3), `supported_calendar_data` (`Vec<MediaType>`, RFC 4791 §5.2.6), and
  `max_attendees_per_instance` (`Option<u32>`, RFC 4791 §5.2.4); `list_calendars` requests them in
  its PROPFIND. New public type `MediaType { content_type, version }`, re-exported from
  `fast_dav_rs::caldav` and the crate root; absent or malformed values yield the type defaults

### Fixed
- `send`/`send_stream` now follow HTTP redirects (301/302/303/307/308) per the
  `follow_redirects`/`max_redirects` options added above, re-sending 303s as bodyless `GET`s,
  stripping `Authorization`/`Cookie` on cross-origin hops, and failing with
  `Error::TooManyRedirects` beyond the limit (issue #77)
- `build_uri` percent-encodes each path segment (spaces, non-ASCII, control and reserved
  characters) while preserving `/` separators, existing valid `%XX` escapes, and any `?query`
  verbatim, so paths with unencoded characters no longer produce invalid request URIs (issue #77)
- `mkcalendar`, `mkaddressbook`, and `proppatch` now send an explicit `Depth: 0` header
  (RFC 4918 §9.2/§9.3): the operations apply to the target collection only (issue #77)
- The collection PROPFIND bodies no longer request the non-existent
  `<C:calendar-color/>` / `<C:addressbook-color/>` properties; the Apple
  `<A:calendar-color/>` / `<A:addressbook-color/>` versions are kept (issue #77)

### Security
- Bearer tokens and proxy credentials are zeroized in the intermediate
  `Authorization` header strings built by the client builder, so plaintext
  credentials no longer linger in freed heap memory (issue #79)
- Decompressed response bodies are capped at 256 MiB to prevent decompression
  bombs: `decompress_body` fails with the new `Error::BodyTooLarge` variant and
  `decompress_stream` errors once the cap is exceeded (issue #79, AUDIT-003)

### Changed
- **Breaking:** `fast_dav_rs::webdav::build_sync_collection_body` gained a trailing
  `sync_level: SyncLevel` parameter, replacing the hardcoded `<D:sync-level>1</D:sync-level>`
  (pass `SyncLevel::One` to keep the previous behavior; scheduled for the 0.10 breaking window)
- **Breaking:** `supports_webdav_sync` (and its CalDAV/CardDAV delegates) now returns
  `Result<SyncCapability>` instead of `Result<bool>`, with `SyncCapability` being
  `Supported`, `Unsupported` or `Unknown`: a transport or timeout error is reported as
  `SyncCapability::Unknown` instead of being silently swallowed as "unsupported"
  (issue #80, audit AUDIT-013)
- The request-compression caches (`request_compression_mode`,
  `negotiated_request_compression`) migrated from `std::sync::RwLock` to
  `parking_lot::RwLock` (new dependency), removing the `PoisonError` recovery shims
  (issue #80)
- The CardDAV `mkcol` body builder now extracts the `D:prop` element with `quick-xml`
  instead of a string search: any namespace prefix (including a default-namespace-bound
  unprefixed `prop`) and any attributes on the element are handled, nested elements are
  captured correctly, and self-closing elements yield an empty inner body (issue #80)
- **Breaking:** `calendar_query_timerange`, `calendar_multiget`, and CalDAV `sync_collection`
  gained a trailing `expand` parameter, and the free builders `build_calendar_query_body`,
  `build_calendar_multiget_body`, and the CalDAV `build_sync_collection_body` gained a trailing
  `expand` argument (pass `None` to keep the previous behavior; scheduled for the 0.10 breaking
  window)

### Removed
Everything deprecated during the 0.9 cycle (semver 0.x → breaking shipped as a minor bump):

- Deprecated request-compression setters on `WebDavClient`, `CalDavClient`, `CardDavClient`:
  `set_request_compression`, `set_request_compression_auto`, `disable_request_compression`.
  Replacement: the builder (`request_compression(...)`) or `set_request_compression_mode`.
- Deprecated associated helpers `etag_from_headers` / `normalize_etag` / `normalize_sync_token`
  on all three clients. Replacement: the free functions re-exported at the crate root
  (`fast_dav_rs::{etag_from_headers, normalize_etag, normalize_sync_token}`).
- Deprecated `impl_multistatus_on_end!` macro.
- Deprecated per-domain streaming aliases `caldav::streaming::{ElementName, element_from_bytes}`
  and their CardDAV counterparts. Replacement:
  `fast_dav_rs::webdav::streaming::{ElementName, element_from_bytes}` (also re-exported under
  the `caldav::streaming` / `carddav::streaming` modules).
- The `legacy` cargo feature and the gated legacy root module paths
  `fast_dav_rs::{client,streaming,types,compression}`. Replacement: `fast_dav_rs::caldav::*`,
  `fast_dav_rs::carddav::*`, `fast_dav_rs::webdav::*`, `fast_dav_rs::common::compression`, or
  the crate-root re-exports.

## [0.9.2] - 2026-09-01

### Fixed
- `sync_collection` (CalDAV and CardDAV) now sends `Depth: 0` as mandated by RFC 6578 §3.3 —
  `Depth: 1` returned 400 on strict servers; the `supports_webdav_sync` probe was fixed the same
  way (external audit AUDIT-001)
- The aggregated body read/decompress phase is now bounded by the per-request timeout instead of
  hanging forever on stalled servers; `Error::Timeout` and `send_stream` docs state the exact
  phase-scoped semantics (external audit AUDIT-002)

### Security
- The publish workflow only triggers on version tags and verifies the tag matches the crate
  version before publishing (external audit AUDIT-005)
- `.env` / `.env.*` added to `.gitignore` (external audit AUDIT-030)

### Changed
- Batch helpers keep their infallible `expect`/`unwrap` calls with documented invariants;
  the audit finding AUDIT-022 is closed as statically infallible (typed-error conversion
  deferred to the 0.10 breaking window)

## [0.9.1] - 2026-09-01

### Added
- Unified multistatus parser shared by CalDAV and CardDAV, exposed as the new public path
  `fast_dav_rs::webdav::streaming` (unified `ElementName` marked `#[non_exhaustive]`,
  `DavItem` field superset, `parse_multistatus_*` family and `parse_error_body`)
- Free functions `etag_from_headers`, `normalize_etag`, `normalize_sync_token` re-exported
  at the crate root (canonical replacements for the deprecated associated helpers)

### Deprecated
- `set_request_compression` / `set_request_compression_auto` / `disable_request_compression`
  on `WebDavClient`, `CalDavClient`, `CardDavClient` — use the builder `request_compression(...)`
  or `set_request_compression_mode`
- Associated helpers `etag_from_headers` / `normalize_etag` / `normalize_sync_token` on all
  three clients — use the crate-root free functions
- `caldav::streaming::ElementName` / `element_from_bytes` and their CardDAV counterparts —
  use `fast_dav_rs::webdav::streaming::{ElementName, element_from_bytes}`
- `impl_multistatus_on_end!` macro (internal code-generation helper)
- Legacy root module paths (`fast_dav_rs::{client,streaming,types,compression}`, behind the
  `legacy` cargo feature) — removal scheduled in 0.10

### Changed
- ~1,050 lines of duplicated CalDAV/CardDAV client and streaming code unified behind shared
  `webdav/` helpers (parser, delegate macro, sync mapping, compression stack). Public API
  verified non-breaking by `cargo-semver-checks`; parser outputs verified byte-identical
  against 0.9.0 on a differential fixture corpus.

## [0.9.0] - 2026-08-26

### Added
- `#[non_exhaustive]` on all public response types for semver stability
- `DavCapabilities` struct and `capabilities()` method for parsing the `DAV` response header (RFC 4918 §10.1)
- `PropStat` struct to distinguish multiple `<D:propstat>` groups per response (RFC 4918 §13.1)
- `WebDavError` struct for parsing `<D:error>` precondition/postcondition codes (RFC 4918 §14.12)
- `parse_dav_header()` function for parsing comma-separated DAV header values
- `parse_error_body()` function for extracting `<D:error>` child element names
- `Collation` and `MatchType` enums for CardDAV text-match filtering (RFC 6352 §7.3)
- `TextMatch`, `ParamFilter`, and `CardDavFilter` types for CardDAV addressbook-query filters
- `is-not-defined` support in CardDAV prop-filter (RFC 6352 §7.1)
- `negate-condition` attribute on CardDAV text-match
- MSRV verification job (Rust 1.85) in CI
- `cargo audit` security scanning job in CI
- `CHANGELOG.md` (Keep a Changelog format)

### Changed
- Default `User-Agent` header is now `fast-dav-rs/{version}` instead of none
- Default `pool_idle_timeout` is now 90 seconds instead of unbounded
- CardDAV PUT Content-Type now includes `version=4.0` parameter (RFC 6352 §6.2.1)
- `build_addressbook_query_filter` now accepts configurable collation, match-type, and negate parameters
- Streaming parser now tracks multiple propstat groups and response-level status separately
- `DavItemCommon` gains `propstats` and `response_status` fields
- CalDAV/CardDAV `DavItem` gains `propstats` and `response_status` fields

### Removed
- Duplicate `integration_tests.rs` files in caldav and carddav test modules

### Fixed
- `let_chains` replaced with MSRV 1.85 compatible code
- Shared `on_end` and `apply_common` logic deduplicated via macros

## [0.8.0] - 2025-01-01

### Added
- Typed error handling via `thiserror` — `Error` enum replaces `anyhow`
- `Error::InvalidConfig` variant for builder configuration validation
- `Error::InvalidEtag` with `EtagReason` for ETag validation errors
- `Error::InvalidComponentName` for component name validation errors
- `Error::InvalidDateTime` for date-time validation errors
- `Error::UnexpectedStatus { operation, status }` with `Operation` enum
- `Error::Timeout { limit }` for timeout errors
- `Error::Tls` for manually-wrapped TLS/certificate/PEM errors
- Comprehensive unit tests improving SonarCloud coverage to >= 80%
- Codecov and SonarCloud coverage integration in CI
- Client builder pattern with fluent API for `WebDavClient`, `CalDavClient`, and `CardDavClient`
- Request compression support (gzip, brotli, zstd) with `RequestCompressionMode`
- Connection pool configuration (`pool_max_idle_per_host`, `pool_idle_timeout`)
- Proxy support with optional Basic authentication
- Additional PEM-encoded trust roots for debugging (Proxyman/Charles/mitmproxy)
- `danger_accept_invalid_certs` option for testing/debug scenarios
- `connect_timeout` option for the TCP connector
- `force_http1` option to disable HTTP/2 negotiation
- Bearer token authentication (OAuth 2.0)
- Streaming XML parsing for large multistatus responses
- Backpressure-aware streaming operations

### Changed
- Migrated from `anyhow` to `thiserror` for typed error handling
- Bumped to Rust 2024 edition
- Minimum Supported Rust Version (MSRV) set to 1.85
- HTTP client now uses `hyper` 1.x with `hyper-util`
- TLS stack uses `rustls` with native cert loading and webpki fallback
- ETag extraction and formatting hardened before sending requests
- Sync-collection token handling normalized (unquoted in request bodies)
- Multistatus parsing hardened against malformed and stalled responses
- Calendar query inputs escaped and validated to prevent XML injection
- Sync-support detection and deletion status parsing corrected
- Lock poisoning handling improved
- Probe negotiation corrected
- `webpki-roots` bumped to 1.0
- Dev-dependencies version constraints relaxed

### Fixed
- Default feature correctly set on `thiserror`
- ETag and CTag reading normalized
- Streaming multistatus parsing hardened against malformed and stalled responses
- Sync-support detection and deletion status parsing corrected
- Lock poisoning handling improved
- Probe negotiation corrected
- Calendar query inputs escaped and validated against XML injection
- Credentials hardened against leakage

## [0.7.2] - 2024-12-01

### Changed
- Improved release configuration and package metadata

## [0.7.1] - 2024-11-01

### Changed
- `webpki-roots` bumped to 1.0
- Dev-dependencies version constraints relaxed

## [0.7.0] - 2024-10-01

### Added
- Client builder pattern for configurable auth, timeout, pool, TLS, and proxy

### Fixed
- ETag and CTag reading normalized

## [0.6.0] - 2024-09-01

### Added
- Client builder with fluent API

## [0.5.0] - 2024-08-01

### Fixed
- ETag extraction and formatting improved before sending requests
- Streaming multistatus parsing hardened

## [0.4.4] - 2024-07-01

### Fixed
- Streaming multistatus parsing hardened

## [0.4.0] - 2024-06-01

### Security
- Calendar query inputs escaped and validated to prevent XML injection
- Credentials hardened against leakage

### Fixed
- Sync-support detection and deletion status parsing corrected
- Lock poisoning handling improved
- Probe negotiation corrected
