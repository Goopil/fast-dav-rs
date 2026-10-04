# RFC punch list (#225) — cycle 0.19 implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the 17 items of RFC coverage punch list #225 as 7 parallel
work streams, one PR per stream, released as 0.19.0.

**Architecture:** Each stream (S1–S7) is an independent git worktree + branch +
subagent; streams are clustered so their *exclusive* files are disjoint. All
API additions are strictly additive (no signature changes on existing public
methods). Integration (merge-as-ready, conflict resolution, release) is done by
the coordinator, not the stream agents.

**Tech Stack:** Rust (MSRV 1.85), hyper 1.x, quick-xml, tokio; tests via
`cargo nextest`, wire-mock helpers in `tests/unit/common/http_helpers.rs`.

**Spec:** `docs/superpowers/specs/2026-10-04-rfc-punch-list-019-design.md`
(stream S* in this plan ↔ stream S* in the spec).

## Global Constraints

- **100% additive**: no signature change on any existing public method/enum.
  New methods, new fields on `#[non_exhaustive]` types, new `Error`/`Operation`
  variants only.
- **Error handling**: `Error` enum from `src/error.rs` (`#[non_exhaustive]`,
  wildcard arm required when matching). New variants must get a doc comment.
- **No copy-paste** between `caldav/` and `carddav/` (Sonar ≤ 3% duplication on
  new code); share via `webdav/` or `common/`.
- **Coverage**: every new public method needs unit wire-mock tests (success /
  HTTP error / pre-I/O validation) in the gated `unit_tests` target.
- **Docs in sync**: README section, CHANGELOG `Unreleased/0.19` subsection,
  doc comments with `no_run` examples, re-exports in `mod.rs` + `lib.rs`.
- **Gates before PR**: `cargo fmt`, `cargo clippy --all-targets --all-features
  -- -D warnings`, `cargo nextest run --all-features --locked --test
  unit_tests`, `cargo test --doc --all-features`.
- **Wire-mock pattern** (all unit tests): `serve_capture` /
  `response_head("", len)` from `tests/unit/common/http_helpers.rs`; client via
  `CalDavClient::new(&base, None, None).unwrap()` (+ `set_request_compression_mode
  (RequestCompressionMode::Disabled)` when the response body is uncompressed);
  assert on `String::from_utf8_lossy(&captured.lock().unwrap())` for request
  shape. Reference example: `tests/unit/caldav/client_tests.rs:15-69`.
- **Provider A naming rule**: never name Provider A in repo artifacts.
- **No uids/tokens in log lines or assertion messages** (CodeQL).
- Each task commits with a conventional message (`feat(...)`/`test(...)`,
  ≤ 70 chars subject, `Refs #225` or the sub-issue number in the footer).

## Execution model (coordinator, not stream agents)

1. Create 7 GitHub sub-issues referencing #225 (one per stream, title
   `0.19/S<n>: <name>`, checklist = this plan's stream section).
2. Create 7 worktrees: `git worktree add ../fast-dav-rs-s<n>-<slug> -b
   feat/019-s<n>-<slug> main`. One `general` subagent per stream, given: the
   sub-issue URL, its stream section below, the Global Constraints, and its
   exclusive-file list.
3. Merge-as-ready; preferred order when several are ready at once:
   S5 → S4 → S3 → S7 → S2 → S6 → S1. Resolve append-conflicts (error.rs,
   webdav/client.rs, webdav/types.rs, caldav/client.rs, CHANGELOG, README,
   lib.rs re-exports) at merge time.
4. Release task at the end.

---

## Stream S1 — calendar-proxy + ACL (P1#1)

Exclusive files: `src/webdav/acl.rs` (new), `src/caldav/proxy.rs` (new).
Shared: `src/webdav/client.rs` (add `acl`), `src/webdav/streaming.rs` (parse
proxy-for props), `src/webdav/types.rs` (DavItem fields), `src/error.rs`
(`Operation`), `src/webdav/mod.rs` + `src/lib.rs` (re-exports), README,
CHANGELOG. e2e: `tests/e2e/sabredav/`.

### Task S1.1: `Operation::Acl` + low-level `WebDavClient::acl`

**Files:**
- Modify: `src/error.rs` (Operation enum + Display)
- Modify: `src/webdav/client.rs` (new method next to `proppatch`, ~line 1547)
- Test: `tests/unit/webdav/acl_tests.rs` (new)

**Interfaces:**
- Produces: `Operation::Acl` variant; `WebDavClient::acl(path: &str, body: &str)
  -> Result<Response<Bytes>>` (ACL method, `Depth: 0`, empty body).

- [ ] **Step 1: Write the failing tests** in `tests/unit/webdav/acl_tests.rs`:

```rust
use fast_dav_rs::{Error, WebDavClient};

#[tokio::test]
async fn acl_success_sends_method_and_body() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();

    let resp = client
        .acl("principals/users/test/calendar-proxy-read/", "<D:ace>...</D:ace>")
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let req = String::from_utf8_lossy(&captured.lock().unwrap());
    assert!(req.starts_with("ACL "), "expected ACL method: {req}");
    assert!(req.contains("Depth: 0"), "expected 'Depth: 0': {req}");
    assert!(req.contains("<D:ace>"), "expected body: {req}");
}

#[tokio::test]
async fn acl_non_success_maps_to_unexpected_status() {
    let base = crate::common::http_helpers::serve_once(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string(),
        Vec::new(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();
    let err = client.acl("p/", "<D:ace/>").await.unwrap_err();
    assert!(
        matches!(err, Error::UnexpectedStatus { .. }),
        "got: {err:?}"
    );
}
```

- [ ] **Step 2: Run** `cargo nextest run --test unit_tests acl_` — Expected:
  FAIL (no `acl`, no `Operation::Acl`).
- [ ] **Step 3: Implement** — in `src/error.rs` add to `Operation` (alphabetical
  position) with doc comment `/// \`ACL\` to modify the access control list of a
  resource (RFC 3744 §8.1).` variant `Acl,` + Display arm
  `Self::Acl => "ACL",`. In `src/webdav/client.rs` next to `proppatch`:

```rust
/// Send a WebDAV `ACL` request (RFC 3744 §8.1) with a pre-built XML body.
///
/// The body carries one or more `<D:ace>` elements; build it with
/// [`crate::webdav::acl::build_acl_body`]. A successful ACL returns `200 OK`
/// (or `204 No Content`); any other status is surfaced as
/// [`Error::UnexpectedStatus`] with [`Operation::Acl`].
pub async fn acl(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
    let mut h = HeaderMap::new();
    h.insert(header::DEPTH, header::HeaderValue::from_static("0"));
    let resp = self
        .send(
            Method::from_bytes(b"ACL")?,
            path,
            h,
            Some(xml_body.to_owned()),
            None,
        )
        .await?;
    if !resp.status().is_success() {
        return Err(Error::UnexpectedStatus {
            operation: crate::Operation::Acl,
            status: resp.status(),
        });
    }
    Ok(resp)
}
```

  (Adapt to the exact `send` signature used by `proppatch` at
  `src/webdav/client.rs:1547` — mirror it.)
- [ ] **Step 4: Run** the two tests — Expected: PASS. Commit
  `feat(webdav): Add low-level ACL method primitive` `Refs #225`.

### Task S1.2: typed ACE builder (`build_acl_body`)

**Files:**
- Create: `src/webdav/acl.rs`
- Modify: `src/webdav/mod.rs`, `src/lib.rs` (re-exports)
- Test: `tests/unit/webdav/acl_tests.rs`

**Interfaces:**
- Consumes: `Privilege` (`src/webdav/types.rs:679`).
- Produces:
  - `pub struct AcePrincipal` — enum-like: `Href(String)`, `SelfPrincipal`,
    `Unauthenticated`, `All` (`#[non_exhaustive]`, `Debug, Clone, PartialEq,
    Eq`).
  - `pub struct Ace { pub principal: AcePrincipal, pub grant: Vec<Privilege>,
    pub deny: Vec<Privilege>, pub protected: bool }` (`#[non_exhaustive]`).
  - `pub fn build_acl_body(aces: &[Ace]) -> Result<String>` — errors with
    `Error::InvalidInput` on an empty `aces` slice or an ACE granting and
    denying the same privilege (RFC 3744 §2.3 forbids conflicting ACEs).
  - `pub fn principal_href_for_acl(principal_url: &str) -> Result<String>` —
    validates the principal href is non-empty (used by S1.4 grant/revoke).

- [ ] **Step 1: Failing serialization tests** (append to
  `tests/unit/webdav/acl_tests.rs`):

```rust
#[test]
fn build_acl_body_grant_deny() {
    use fast_dav_rs::webdav::acl::{Ace, AcePrincipal};
    use fast_dav_rs::webdav::Privilege;

    let body = fast_dav_rs::webdav::acl::build_acl_body(&[Ace {
        principal: AcePrincipal::Href("/principals/users/bob/".into()),
        grant: vec![Privilege::Read],
        deny: vec![Privilege::WriteContent],
        protected: false,
    }])
    .unwrap();
    assert!(body.contains("<D:acl xmlns:D=\"DAV:\">"));
    assert!(body.contains("<D:ace>"));
    assert!(body.contains("<D:principal><D:href>/principals/users/bob/</D:href></D:principal>"));
    assert!(body.contains("<D:grant><D:privilege><D:read/></D:privilege></D:grant>"));
    assert!(body.contains("<D:deny><D:privilege><D:write-content/></D:privilege></D:deny>"));
}

#[test]
fn build_acl_body_rejects_conflicting_ace_and_empty() {
    use fast_dav_rs::webdav::acl::{Ace, AcePrincipal};
    use fast_dav_rs::webdav::Privilege;
    let ace = Ace {
        principal: AcePrincipal::All,
        grant: vec![Privilege::Read],
        deny: vec![Privilege::Read],
        protected: false,
    };
    assert!(fast_dav_rs::webdav::acl::build_acl_body(&[ace]).is_err());
    assert!(fast_dav_rs::webdav::acl::build_acl_body(&[]).is_err());
}
```

- [ ] **Step 2: Run** — FAIL (module missing).
- [ ] **Step 3: Implement** `src/webdav/acl.rs`. XML skeleton per RFC 3744
  §8.1.1:

```xml
<D:acl xmlns:D="DAV:">
  <D:ace>
    <D:principal><D:href>…</D:href></D:principal>   <!-- or <D:all/> / <D:unauthenticated/> / <D:self/> -->
    <D:grant><D:privilege><D:read/></D:privilege>…</D:grant>
    <D:deny><D:privilege>…</D:privilege></D:deny>
    <D:protected/>                                   <!-- when protected -->
  </D:ace>
</D:acl>
```

  Serialize `Privilege` to its XML element local name via a private
  `privilege_xml_name(&Privilege) -> Option<&str>` mapping (`Read` → `read`,
  `Write` → `write`, `WriteProperties` → `write-properties`, `WriteContent` →
  `write-content`, `Bind` → `bind`, `Unbind` → `unbind`, `Unlock` → `unlock`,
  `ReadFreeBusy` → `read-free-busy`, `All` → `all`, `Other(name)` → the raw
  name if it matches `[a-z-]+`, else `Error::InvalidInput`). Escape hrefs with
  `escape_xml` (`src/webdav/xml.rs`). Register the module in `src/webdav/mod.rs`
  and re-export `Ace`, `AcePrincipal`, `build_acl_body` from `webdav` and the
  crate root (`src/lib.rs`), following the existing re-export grouping.
- [ ] **Step 4: Run** — PASS. Commit `feat(webdav): Add typed ACL body builder`.

### Task S1.3: `DavItem` proxy fields + streaming parse

**Files:**
- Modify: `src/webdav/types.rs` (`DavItem`, after `managed_ids` ~line 739)
- Modify: `src/webdav/streaming.rs` (property parse match — mirror
  `calendar_timezone` handling)
- Test: `tests/unit/webdav/proxy_parse_tests.rs` (new)

**Interfaces:**
- Produces: `DavItem.calendar_proxy_read_for: Vec<String>`,
  `DavItem.calendar_proxy_write_for: Vec<String>` — hrefs from
  `calendar-proxy-read-for` / `calendar-proxy-write-for` (principal properties
  of the CalDAV calendar-proxy companion spec), default empty.

- [ ] **Step 1: Failing parse test** — build a multistatus with
  `<C:calendar-proxy-read-for><D:href>/principals/users/alice/</D:href></C:calendar-proxy-read-for>`
  (xmlns `C="urn:ietf:params:xml:ns:caldav"`), assert the field is populated;
  second test: absent property → empty vec. Use the raw-response →
  `parse_multistatus_bytes` path (mirror how `tests/unit/webdav/compliance_tests.rs`
  feeds bodies).
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement** — add both fields to `DavItem` + `Default`/`new`
  init; in `streaming.rs`, in the CalDAV property match arm (find
  `schedule_inbox` / `calendar_timezone` handling), collect child
  `<D:href>` text values verbatim.
- [ ] **Step 4: Run** — PASS. Commit `feat(caldav): Parse calendar-proxy-for principal properties`.

### Task S1.4: `caldav/proxy.rs` — list + resolve + grant/revoke

**Files:**
- Create: `src/caldav/proxy.rs`; register in `src/caldav/mod.rs`, `src/lib.rs`
- Test: `tests/unit/caldav/proxy_tests.rs` (new)

**Interfaces:**
- Consumes: `DavItem.calendar_proxy_read_for/write_for` (S1.3), `build_acl_body`
  (S1.2), `WebDavClient::acl` (S1.1), `CalDavClient` Deref to `WebDavClient`.
- Produces:
  - `pub struct CalendarProxyInfo { pub read_for: Vec<String>,
    pub write_for: Vec<String> }` (`#[non_exhaustive]`).
  - `CalDavClient::list_calendar_proxies(principal_path: &str) ->
    Result<CalendarProxyInfo>` — PROPFIND `Depth: 0` requesting
    `calendar-proxy-read-for` + `calendar-proxy-write-for`;
    `Operation::PropfindCalendarProxy`.
  - `CalDavClient::calendar_proxy_group_members(principal_path: &str, write:
    bool) -> Result<Vec<String>>` — PROPFIND on
    `<principal>/calendar-proxy-write|read` requesting `group-member-set`,
    returns hrefs; `Operation::PropfindCalendarProxy`.
  - `CalDavClient::grant_calendar_proxy(principal_path: &str, delegate_href:
    &str, write: bool) -> Result<()>` / `revoke_calendar_proxy(principal_path:
    &str, delegate_href: &str, write: bool) -> Result<()>` — resolve the proxy
    group's current `group-member-set`, then issue an ACL via the S1.1
    primitive granting (or removing) `read`/`write` to the delegate on the
    proxy group principal; **document in the method docs that the wire form is
    server-dependent and the e2e records observed behavior** (0.13 fallback
    convention). `Operation::Acl`.

- [ ] **Step 1: Failing wire tests** in `tests/unit/caldav/proxy_tests.rs`
  (pattern: `serve_capture`, `CalDavClient::new`): a PROPFIND mock returning a
  multistatus with the proxy-for props → `list_calendar_proxies` maps them; a
  `group-member-set` mock → members resolved; grant asserts the captured
  request contains `ACL` and the delegate href; revoke asserts the delegate
  href is absent from the captured ACL body.
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement.** PROPFIND bodies are small string constants
  (namespace `C="urn:ietf:params:xml:ns:caldav"`); map via
  `parse_multistatus_bytes`; status mapping → `Error::UnexpectedStatus`.
  Add `Operation::PropfindCalendarProxy` + Display `"PROPFIND calendar-proxy"`.
- [ ] **Step 4: Run** — PASS. Commit `feat(caldav): Add calendar-proxy listing, resolution and grant/revoke`.

### Task S1.5: e2e attempt + docs + re-exports

**Files:**
- Modify: `sabredav-test/public/index.php` (attempt: enable
  `Sabre\CalDAV\Principal` proxy support / ACL plugin)
- Modify: `tests/e2e/sabredav/caldav/proxy_tests.rs` (new)
- Modify: `README.md` (new "Calendar proxies (RFC 6638 companion)" subsection
  in the feature list area), `CHANGELOG.md` (`Unreleased / 0.19 / Added`),
  doc comments (`no_run` examples for `list_calendar_proxies`, `build_acl_body`)

- [ ] **Step 1:** Try enabling calendar-proxy in the SabreDAV fixture; write
  one e2e that discovers the principal, lists proxies, and (if the plugin
  round-trips) grants + revokes a proxy for a second fixture principal.
- [ ] **Step 2:** If the fixture cannot support it, keep the e2e file with a
  discovery-only test recording observed behavior (0.13 convention) and note
  the fallback in the PR description (Sonar exemption).
- [ ] **Step 3:** Docs sync (README section + feature table row,
  CHANGELOG entry, doc examples compile: `cargo test --doc --all-features`).
- [ ] **Step 4:** Gates. Commit `feat(caldav): Wire calendar-proxy e2e and docs`
  then PR `feat/019-s1-calendar-proxy-acl`.

---

## Stream S2 — schedule-tag retrieval + scheduling tests (P1#3, P3#14)

Exclusive files: none new. Shared: `src/webdav/client.rs`, `src/webdav/types.rs`,
`src/webdav/streaming.rs`, `src/caldav/scheduling.rs` (docs), README, CHANGELOG,
`tests/unit/caldav/scheduling_tests.rs`, `tests/e2e/sabredav/caldav/scheduling_tests.rs`.

### Task S2.1: `schedule_tag_from_headers`

**Files:**
- Modify: `src/webdav/client.rs` (next to `etag_from_headers`, line 170)
- Test: `tests/unit/webdav/header_helpers_tests.rs` (new; or append to an
  existing webdav unit file — create only if no natural home)

**Interfaces:**
- Produces: `pub fn schedule_tag_from_headers(headers: &HeaderMap) ->
  Option<String>` — normalized like `normalize_etag` (quotes stripped),
  `None` when absent/empty. Re-exported from `webdav` + crate root.

- [ ] **Step 1: Failing tests:**

```rust
#[test]
fn schedule_tag_from_headers_quoted_and_bare() {
    let mut h = hyper::HeaderMap::new();
    h.insert("Schedule-Tag", r#""tag-1""#.parse().unwrap());
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&h).as_deref(),
        Some("tag-1")
    );
    h.clear();
    h.insert("Schedule-Tag", "tag-2".parse().unwrap());
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&h).as_deref(),
        Some("tag-2")
    );
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&hyper::HeaderMap::new()),
        None
    );
}
```

- [ ] **Step 2: Run** — FAIL. **Step 3:** Implement mirroring
  `etag_from_headers` (body: `headers.get("Schedule-Tag")…map(normalize_etag)
  .filter(|s| !s.is_empty())`) with a doc comment + `no_run` example (RFC 6638
  §10.1.2 semantics: opaque token, send back via
  [`put_if_schedule_tag`](crate::CalDavClient::put_if_schedule_tag)).
  **Step 4:** PASS; commit `feat(webdav): Add schedule_tag_from_headers helper`.

### Task S2.2: `DavItem.schedule_tag` property parse

**Files:**
- Modify: `src/webdav/types.rs` (field after `managed_ids`)
- Modify: `src/webdav/streaming.rs` (CalDAV prop match arm)
- Test: append to `tests/unit/webdav/header_helpers_tests.rs`

**Interfaces:**
- Produces: `DavItem.schedule_tag: Option<String>` from
  `<C:schedule-tag>` (RFC 6638 §10.1.1) — the element text verbatim.

- [ ] **Step 1:** Failing parse test (multistatus with the prop → `Some`,
  absent → `None`). **Step 2:** FAIL. **Step 3:** Implement (mirror
  `calendar_timezone` parse arm; do not trim inner whitespace beyond the
  element's text node). **Step 4:** PASS; commit
  `feat(webdav): Parse schedule-tag property into DavItem`.

### Task S2.3: `split_params_value` unit test (P3#14 second half)

**Files:**
- Test: `tests/unit/caldav/scheduling_tests.rs` (append; function is private —
  test through its public caller `list_inbox` param parsing, or move the
  function to `common/` if a direct unit test needs it; prefer
  testing via the public `list_inbox` wire mock with a `CN="X";PARTSTAT=...`
  param string if the function stays private)

- [ ] **Step 1:** Read `split_params_value` (`src/caldav/client.rs:1150`) and
  its caller; write 4 assertions through the public path: no params, one
  quoted param (comma inside quotes must not split), two params, empty.
  **Step 2:** FAIL where it documents today's bug, else PASS + keep as
  regression lock. **Step 3:** Fix only if a real bug surfaces (record it in
  CHANGELOG `Fixed`). **Step 4:** commit `test(caldav): Cover split_params_value wire shape`.

### Task S2.4: outbox POST e2e (P3#14 first half) + docs

**Files:**
- Modify: `tests/e2e/sabredav/caldav/scheduling_tests.rs`
- Modify: `README.md` (scheduling section: mention schedule-tag retrieval
  helpers), `CHANGELOG.md`

- [ ] **Step 1:** e2e `test_outbox_post_well_formed_free_busy_request`:
  discover endpoints → POST a minimal valid iTIP VFREEBUSY REQUEST to the
  outbox via the existing outbox POST API → assert the documented success
  status (200/204) or record observed behavior (e.g. 400 with `<D:error>`) in
  the test comment + README provider quirks. One negative test: malformed iTIP
  → non-2xx asserted.
- [ ] **Step 2:** Docs sync. **Step 3:** Gates. **Step 4:** commit
  `test(e2e): Exercise schedule outbox POST on SabreDAV` then PR
  `feat/019-s2-schedule-tag`.

---

## Stream S3 — managed-attachment lifecycle (P1#2)

Exclusive files: none new. Shared: `src/caldav/client.rs` (attachment methods
near `post_managed_attachment` line 987), `src/error.rs` (`Operation`),
README, CHANGELOG, `tests/unit/caldav/attachment_tests.rs`,
`tests/e2e/radicale/caldav/attachment_tests.rs`.

### Task S3.1: `put_managed_attachment` / `delete_managed_attachment`

**Files:**
- Modify: `src/caldav/client.rs`, `src/error.rs`
- Test: `tests/unit/caldav/attachment_tests.rs` (new)

**Interfaces:**
- Consumes: `Operation::PostManagedAttachment` (existing) as the template.
- Produces:
  - `Operation::PutManagedAttachment` (Display: `"PUT managed attachment"`),
    `Operation::DeleteManagedAttachment` (`"DELETE managed attachment"`).
  - `CalDavClient::put_managed_attachment(href: &str, body: &[u8],
    content_type: &str, managed_id: &str) -> Result<Response<Bytes>>` — `PUT`
    `href` with headers `Cal-Managed-ID: <managed_id>` and
    `Content-Type: <content_type>` (RFC 8607 §5.2).
  - `CalDavClient::delete_managed_attachment(href: &str, managed_id: &str) ->
    Result<Response<Bytes>>` — `DELETE` `href` with the `Cal-Managed-ID`
    header (§5.3).
  - Pre-I/O validation: empty `href` / `managed_id` / `content_type` (PUT
    only) → `Error::InvalidInput`; the href must already be the attachment
    resource (from `ManagedAttachment.href`).

- [ ] **Step 1: Failing wire tests** (`serve_capture`):
  success 204 asserts method (`PUT`/`DELETE`), the `Cal-Managed-ID: mid-1`
  header and (PUT) the `Content-Type` header; 404 → `UnexpectedStatus`; empty
  `managed_id` → `InvalidInput` before I/O (no request captured).
- [ ] **Step 2: Run** — FAIL. **Step 3:** Implement (mirror
  `post_managed_attachment`'s header insertion; insert via
  `HeaderName::from_static("cal-managed-id")`). **Step 4:** PASS; commit
  `feat(caldav): Add managed-attachment update and removal`.

### Task S3.2: e2e Radicale round-trip + docs

**Files:**
- Modify: `tests/e2e/radicale/caldav/attachment_tests.rs` (new)
- Modify: `README.md` (RFC 8607 section: update/remove now exposed),
  `CHANGELOG.md`

- [ ] **Step 1:** e2e: create event → `post_managed_attachment` →
  `get` the returned `href` (asserts P1#2's GET-by-href) →
  `put_managed_attachment` (new content, same managed-id) → GET asserts new
  content → `delete_managed_attachment` → GET asserts 404.
- [ ] **Step 2:** If Radicale's live wire differs from the mock (e.g. extra
  header requirement), fix the mocks to match live and record the quirk in
  the README provider table (0.13 convention). **Step 3:** Gates. **Step 4:**
  commit `test(e2e): Cover managed-attachment round-trip on Radicale` then PR
  `feat/019-s3-attachments`.

---

## Stream S4 — body builders (P1#4, P2#8, P2#10)

Exclusive files: none new. Shared: `src/webdav/xml.rs` (exclusive within the
cycle for *new* functions), `src/caldav/client.rs` (one new method + one new
options struct), `src/caldav/types.rs` (`CalendarQueryOptions` may live in
`webdav` if shared — put it in `src/webdav/types.rs` to avoid the
caldav/carddav duplication gate), README, CHANGELOG,
`tests/unit/caldav/caldav_helpers.rs` + `tests/unit/caldav/client_tests.rs`.

### Task S4.1: `data_element_xml` limits (limit-recurrence-set / limit-freebusy-set)

**Files:**
- Modify: `src/webdav/types.rs` (`CalendarDataLimits`), `src/webdav/xml.rs`
- Test: `tests/unit/caldav/caldav_helpers.rs` (append) or new
  `tests/unit/webdav/body_builder_tests.rs`

**Interfaces:**
- Consumes: `TimeRange` (`src/webdav/types.rs`, fields `start`/`end`, used by
  `expand`).
- Produces:
  - `pub struct CalendarDataLimits { pub recurrence_set: Option<TimeRange>,
    pub freebusy_set: Option<TimeRange> }` (`#[non_exhaustive]`, `Debug,
    Clone`).
  - `pub fn data_element_xml_with_limits(data_element: &str, expand:
    Option<(&str, &str)>, limits: Option<&CalendarDataLimits>) -> String` —
    appends `<C:limit-recurrence-set start="…" end="…"/>` and/or
    `<C:limit-freebusy-set start="…" end="…"/>` inside
    `<C:calendar-data>` **after** `<C:expand>` (RFC 4791 §9.6.4). Existing
    `data_element_xml` unchanged (it delegates with `limits: None`).

- [ ] **Step 1: Failing builder test:**

```rust
#[test]
fn data_element_xml_with_limits_serializes_both_children() {
    let limits = fast_dav_rs::webdav::CalendarDataLimits {
        recurrence_set: Some(fast_dav_rs::TimeRange::new("20240101T000000Z").with_end("20241231T235959Z")),
        freebusy_set: None,
    };
    let xml = fast_dav_rs::webdav::xml::data_element_xml_with_limits(
        "calendar-data",
        Some(("20240101T000000Z", "20240301T000000Z")),
        Some(&limits),
    );
    assert!(xml.contains("<C:calendar-data>"));
    assert!(xml.contains("<C:expand start=\"20240101T000000Z\" end=\"20240301T000000Z\"/>"));
    assert!(xml.contains("<C:limit-recurrence-set start=\"20240101T000000Z\" end=\"20241231T235959Z\"/>"));
    // expand must precede limits (RFC 4791 §9.6 DTD order)
    assert!(xml.find("<C:expand").unwrap() < xml.find("<C:limit-recurrence-set").unwrap());
}
```

- [ ] **Step 2: Run** — FAIL. **Step 3:** Implement (read the existing
  `data_element_xml` at `src/webdav/xml.rs` and extend; keep
  `data_element_xml` as a one-line delegate so all existing callers are
  untouched). **Step 4:** PASS; commit
  `feat(caldav): Add limit-recurrence-set/limit-freebusy-set body support`.

### Task S4.2: `CalendarQueryOptions` + `calendar_query_options`

**Files:**
- Modify: `src/webdav/types.rs` (`CalendarQueryOptions`), `src/webdav/xml.rs`
  (`build_calendar_query_body_with_limits`), `src/caldav/client.rs` (one new
  method), `src/caldav/mod.rs` re-exports
- Test: `tests/unit/caldav/client_tests.rs` (append)

**Interfaces:**
- Consumes: S4.1, `validate_utc_datetime` / `validate_time_range_order`
  (`src/caldav/client.rs` private fns — reuse), `Operation::ReportCalendarQuery`.
- Produces:
  - `pub struct CalendarQueryOptions { pub component: String, pub start:
    Option<String>, pub end: Option<String>, pub include_data: bool, pub
    expand: Option<TimeRange>, pub limits: Option<CalendarDataLimits> }`
    (`#[non_exhaustive]`, lives in `webdav::types`, re-exported).
  - `pub fn build_calendar_query_body_with_limits(options:
    &CalendarQueryOptions) -> String` (public; pure XML, no validation).
  - `CalDavClient::calendar_query_options(calendar_path: &str, options:
    &CalendarQueryOptions) -> Result<Vec<CalendarObject>>` — validation
    mirrors `calendar_query_timerange` (`src/caldav/client.rs:448`): component
    name, UTC date-times, expand-without-end, end ≤ start → `Error::InvalidInput`
    / `Error::InvalidDateTime`; non-success → `UnexpectedStatus`.

- [ ] **Step 1: Failing tests** — builder test asserting
  `<C:limit-recurrence-set …/>` present in the query body; wire test
  (`serve_capture`) asserting the REPORT body and mapping to
  `Vec<CalendarObject>`; validation tests: empty component → `InvalidInput`;
  `expand` without end → `InvalidInput`; limits with `include_data: false`
  → still requests data (limits imply data, like expand — document).
- [ ] **Step 2: Run** — FAIL. **Step 3:** Implement — the new method is a thin
  wrapper: validate → build body → `self.report(...)` → map
  (`map_calendar_objects`). Existing methods untouched. **Step 4:** PASS;
  commit `feat(caldav): Add calendar_query_options with data limits`.

### Task S4.3: PROPFIND body helpers (P2#8)

**Files:**
- Modify: `src/webdav/xml.rs`
- Test: `tests/unit/webdav/body_builder_tests.rs`

**Interfaces:**
- Produces: `pub fn build_propfind_allprop() -> String`,
  `pub fn build_propfind_propname() -> String`,
  `pub fn build_propfind_props(props: &[(&str, &str)]) -> String` —
  `(namespace, local-name)` pairs, escaped, e.g.
  `build_propfind_props(&[("DAV:", "displayname"), ("urn:ietf:params:xml:ns:caldav", "calendar-description")])`.

- [ ] **Step 1: Failing tests** (assert full bodies:
  `<D:propfind xmlns:D="DAV:"><D:allprop/></D:propfind>` etc.; props version
  groups namespace declarations on the root and `<D:prop>` children as
  `<ns-prefix:name/>` — for `DAV:` use `D:`). **Step 2:** FAIL.
  **Step 3:** Implement. **Step 4:** PASS; commit
  `feat(webdav): Add PROPFIND allprop/propname/prop-list body helpers`.

### Task S4.4: `MkCalendarProps` typed builder (P2#10)

**Files:**
- Modify: `src/webdav/xml.rs`
- Test: `tests/unit/webdav/body_builder_tests.rs`

**Interfaces:**
- Produces: `pub struct MkCalendarProps { pub displayname: Option<String>,
  pub description: Option<String>, pub supported_components: Vec<String> }`
  (`#[non_exhaustive]`); `pub fn build_mkcalendar_body(props:
  &MkCalendarProps) -> Result<String>` — RFC 4791 §9.5 skeleton
  `<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop>…</D:prop></D:set></C:mkcalendar>`; component names validated
  (ASCII alphanumeric + `-`, non-empty) → `Error::InvalidInput`; empty
  `supported_components` allowed (no `supported-calendar-component-set`
  element). Usable with the existing `CalDavClient::mkcalendar(path, &body)`.

- [ ] **Step 1: Failing tests** (serialization; invalid component → err;
  empty props → minimal body). **Step 2:** FAIL. **Step 3:** Implement.
  **Step 4:** PASS; commit `feat(caldav): Add typed MKCALENDAR property builder`.

### Task S4.5: docs + gates

- [ ] README "Advanced" section: PROPFIND helpers + limits example;
  CHANGELOG `Added`. `no_run` doc example on `calendar_query_options` and
  `build_mkcalendar_body`. Gates. PR `feat/019-s4-body-builders`.

---

## Stream S5 — CardDAV address-data (P1#5)

Exclusive files: none new. Shared: `src/webdav/types.rs` (`Collation::Octet`),
`src/carddav/client.rs`, `src/carddav/mod.rs`, README, CHANGELOG,
`tests/unit/carddav/`.

### Task S5.1: `Collation::Octet` (i;octet)

**Files:**
- Modify: `src/webdav/types.rs` (`Collation`, line 158), doc comment
- Test: `tests/unit/carddav/client_tests.rs` (append; there is already a
  collation serialization test to extend)

- [ ] **Step 1:** Failing test: `Collation::Octet.as_str() == "i;octet"` and a
  `build_addressbook_query_filter` body with `collation="i;octet"`.
  **Step 2:** FAIL. **Step 3:** Add variant + `as_str` arm + doc line ("RFC
  4790 registry; RFC 6352 §7.3 — servers must support `i;ascii-casemap` and
  `i;octet`"). **Step 4:** PASS; commit `feat(carddav): Add i;octet collation`.

### Task S5.2: `AddressQueryOptions` + limited address-data + nresults

**Files:**
- Modify: `src/webdav/types.rs` (`AddressQueryOptions`), `src/carddav/client.rs`
  (`build_addressbook_query_body_with_options`, `addressbook_query_options`),
  re-exports
- Test: `tests/unit/carddav/client_tests.rs` (append), `carddav_helpers.rs`

**Interfaces:**
- Produces:
  - `pub struct AddressQueryOptions { pub filter_xml: String, pub
    include_data: bool, pub address_data_props: Vec<String>, pub limit:
    Option<u32> }` (`#[non_exhaustive]`, in `webdav::types`, re-exported).
  - `pub fn build_addressbook_query_body_with_options(options:
    &AddressQueryOptions) -> String` — limited form
    `<C:address-data><C:prop name="FN"/>…</C:address-data>` (RFC 6352 §10.4.2;
    prop names validated `[A-Za-z0-9-]+` by the caller method) and
    `<D:limit><D:nresults>N</D:nresults></D:limit>` as the **last** child
    (§10.6). `address_data_props` empty + `include_data` → `<C:address-data/>`
    (today's form).
  - `CardDavClient::addressbook_query_options(book_path: &str, options:
    &AddressQueryOptions) -> Result<Vec<AddressObject>>` — validation: prop
    names non-empty + ASCII, duplicates dropped (stable) → `Error::InvalidInput`
    otherwise; status → `UnexpectedStatus { operation: ReportAddressbookQuery }`.

- [ ] **Step 1: Failing tests** — builder: props + limit serialized; empty
  props + `include_data` keeps `<C:address-data/>`; wire test (`serve_capture`)
  mapping to `Vec<AddressObject>`; invalid prop name → `InvalidInput`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement — the new method mirrors
  `addressbook_query` (line 263) body assembly; keep the old function as a
  delegate (`build_addressbook_query_body(filter, include_data)` → options
  with empty props, no limit). **Step 4:** PASS; commit
  `feat(carddav): Add limited address-data and nresults query options`.

### Task S5.3: docs + gates

- [ ] README CardDAV section + `no_run` doc example on
  `addressbook_query_options`; CHANGELOG. Gates. PR `feat/019-s5-carddav-address-data`.

---

## Stream S6 — conditional writes + Depth + privileges (P2#7, #9, #11, #12, P3#16)

Shared files (append): `src/error.rs`, `src/webdav/client.rs`,
`src/webdav/types.rs`, `src/caldav/client.rs`, `src/caldav/types.rs`, README,
CHANGELOG, `tests/unit/webdav/`, `tests/unit/caldav/`.

### Task S6.1: typed 412 / 428 errors (P2#7)

**Design note (verified against the code):** the existing conditional writes
(`put_if_match`, `put_if_none_match`, `put_if_match_prefer`,
`delete_if_match`, `put_if_schedule_tag`, `delete_if_schedule_tag`) return
the raw `Response<Bytes>` **by design** — e.g. `put_if_schedule_tag`'s docs
say "any status (e.g. `204` on success or `412` on tag mismatch) is returned
to the caller". Changing that would be breaking. The typed handling is
therefore delivered as **new variants + a public classifier helper** callers
apply to the raw response — 100% additive.

**Files:**
- Modify: `src/error.rs` (two variants + Display, follow the struct-variant doc
  conventions at `src/error.rs:116-164`)
- Modify: `src/webdav/client.rs` (new public helper near `etag_from_headers`)
- Test: `tests/unit/webdav/conditional_tests.rs` (new)

**Interfaces:**
- Produces:
  - `Error::PreconditionFailed { operation: Operation }`
    (`#[non_exhaustive]`, doc: "412 on a conditional write — the validator
    (ETag/lock/schedule-tag) did not match; reload the item and retry"),
    `Error::PreconditionRequired { operation: Operation }` ("428 — the server
    requires a conditional header on this write").
  - `pub fn conditional_write_error(operation: Operation, resp:
    &Response<Bytes>) -> Result<()>` — `Ok(())` on 2xx;
    `Error::PreconditionFailed { operation }` on 412;
    `Error::PreconditionRequired { operation }` on 428;
    `Error::UnexpectedStatus { operation, status }` otherwise. Public,
    re-exported; works with every conditional write's raw response.

- [ ] **Step 1: Failing tests** — build a `Response<Bytes>` via
  `serve_once`/`Response::builder` for statuses 204 / 412 / 428 / 500 and
  assert `conditional_write_error(Operation::PutIfMatch, …)` maps each to
  `Ok` / `PreconditionFailed` / `PreconditionRequired` / `UnexpectedStatus`
  respectively. Add `Operation::PutIfMatch` and `Operation::DeleteIfMatch`
  (specific operations, + Display arms: `"PUT If-Match"`,
  `"DELETE If-Match"`) so classification carries context — the existing
  methods keep passing whatever they pass today (no behavior change); new
  callers use the new operations.
- [ ] **Step 2: Run** — FAIL (variants/helper missing).
- [ ] **Step 3:** Implement the variants, Display arms, and the helper (one
  `match` on `resp.status()`). No existing method body changes.
- [ ] **Step 4: Run** full unit suite — PASS (zero existing assertions
  change); commit `feat(webdav): Add typed 412/428 conditional-write classifier`.

### Task S6.2: `If` header builder (P2#7 second half)

**Files:**
- Modify: `src/webdav/client.rs` (near `etag_from_headers`)
- Test: `tests/unit/webdav/conditional_tests.rs`

**Interfaces:**
- Produces: `pub fn if_header_for_lock_token(lock_token: &str) ->
  Result<String>` — RFC 4918 §10.4 parenthesized form `(<lock-token>)`.
  Validation: non-empty, no `(`, `)`, control chars → `Error::InvalidInput`.

- [ ] **Step 1: Failing tests** (valid token → `(<urn:uuid:...>)`; empty /
  paren-bearing → `InvalidInput`). **Step 2:** FAIL. **Step 3:** Implement +
  `no_run` doc example pairing it with `lock_request` + `delete` with manual
  `If` header. **Step 4:** PASS; commit `feat(webdav): Add If header builder for lock-token-guarded writes`.

### Task S6.3: `copy_with_depth` / `move_with_depth` (P2#9)

**Files:**
- Modify: `src/webdav/client.rs` (private `copy_move` at line 1473 gains a
  `depth: Option<Depth>` internal parameter; existing `copy`/`move` pass
  `None`)
- Test: `tests/unit/webdav/copy_move_tests.rs` (new)

**Interfaces:**
- Produces: `pub async fn copy_with_depth(src_path: &str, dest_absolute_url:
  &str, overwrite: bool, depth: Depth) -> Result<Response<Bytes>>` — sends the
  `Depth` header verbatim (RFC 4918 §9.8.3: `0` = shallow copy). `pub async
  fn move_with_depth(src_path: &str, dest_absolute_url: &str, overwrite: bool,
  depth: Depth) -> Result<Response<Bytes>>` — only `Depth::Zero` accepted
  pre-I/O (`One`/`Infinity` → `Error::InvalidInput`, doc: RFC 4918 §9.9.3 —
  MOVE acts as `infinity` for collections; a `Depth` header is only
  meaningful as `0` for non-collection moves).

- [ ] **Step 1: Failing wire tests** — `serve_capture`: `copy_with_depth`
  with `Depth::Zero` asserts `Depth: 0` + `Destination` + `Overwrite: T`;
  `move_with_depth` with `Depth::One` → `InvalidInput` with no request
  captured; existing `copy`/`move` unaffected (no `Depth` header in captured
  request).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS; commit
  `feat(webdav): Add Depth parameter variants for COPY/MOVE`.

### Task S6.4: `Privilege::All` + doc caveat (P2#12, P3#16)

**Files:**
- Modify: `src/webdav/types.rs` (`Privilege` enum + doc comment)
- Modify: the privilege parse match (find where
  `current_user_privileges` is filled in `src/webdav/streaming.rs`)
- Test: `tests/unit/webdav/privilege_tests.rs` (new or existing home)

- [ ] **Step 1:** Failing parse test: multistatus containing
  `<D:current-user-privilege-set><D:privilege><D:all/></D:privilege>…` →
  `Privilege::All`. **Step 2:** FAIL. **Step 3:** Add variant + doc ("aggregate
  `all` — implies read and write; matches servers granting `all` instead of
  enumerating") + the `Other("all")` caveat in the enum doc comment (P3#16:
  "servers that advertise `all` used to map to `Other("all")` before 0.19;
  match on `All` for the aggregate"). **Step 4:** PASS; commit
  `feat(webdav): Add Privilege::All aggregate variant`.

### Task S6.5: `list_calendars` privileges (P2#11)

**Files:**
- Modify: `src/caldav/client.rs` (`list_calendars` PROPFIND body, line 398:
  add `<D:current-user-privilege-set/>`), `src/caldav/types.rs`
  (`CalendarInfo.privileges: Vec<Privilege>`), `map_calendar_list`
- Test: `tests/unit/caldav/client_tests.rs`

**Interfaces:**
- Produces: `CalendarInfo.privileges` (documented: empty when the server
  omitted the property; the PROPFIND now requests it — behavior change
  documented in CHANGELOG `Changed`).

- [ ] **Step 1:** Failing wire test: multistatus with a calendar + privilege
  set → `calendars[0].privileges` contains `Privilege::Read`. **Step 2:**
  FAIL. **Step 3:** Implement. **Step 4:** PASS; commit
  `feat(caldav): Surface current-user privileges on CalendarInfo`.

### Task S6.6: docs + gates

- [ ] README: conditional-errors table row (412/428), Depth COPY/MOVE,
  privileges on CalendarInfo; CHANGELOG `Added` + `Changed` (list_calendars
  extra property); `no_run` examples on the new public items. Gates. PR
  `feat/019-s6-conditional-depth-privileges`.

---

## Stream S7 — tests + docs (P3#13, #15, #17)

Exclusive files: none new. Shared: `tests/unit/webdav/` (new test file),
`src/webdav/discovery.rs` (docs), `src/webdav/types.rs` +
`src/webdav/streaming.rs` (`principal_url`), README, CHANGELOG.

### Task S7.1: `mkcol` wire tests (P3#13)

**Files:**
- Test: `tests/unit/webdav/mkcol_tests.rs` (new)

- [ ] **Step 1:** Write tests using `serve_capture`: (a) `mkcol(path, None)` →
  201, captured request has `MKCOL` and no body; (b) 405 response →
  `UnexpectedStatus`; (c) `mkcol(path, Some(body))` (RFC 5689 extended-MKCOL)
  → captured request carries the XML body and a `Content-Type: application/xml`
  header; (d) a 207-with-error-propstat response (server-dependent) → mapped
  to `UnexpectedStatus` (document if the current code treats 207 as failure —
  check `mkcol` at `src/webdav/client.rs:1588` and align the test with actual
  behavior, fixing code only if 207 success-with-errors is mishandled; record
  in CHANGELOG `Fixed` if so).
- [ ] **Step 2:** Run (they exercise existing code — expected PASS unless (d)
  finds a bug). Commit `test(webdav): Cover mkcol and extended-MKCOL wire shape`.

### Task S7.2: SRV + TXT documented exclusion (P3#15) + follow-up issue text

**Files:**
- Modify: `src/webdav/discovery.rs` (module doc line 16 + `discover_service_urls`
  doc line 160: explicit RFC 6764 §3 SRV *and* §6 TXT exclusion, rationale:
  lean dependency tree; pointers to the well-known path that covers §5)
- Modify: `README.md` (discovery section), `docs/advanced-configuration.md` if
  it documents discovery

- [ ] **Step 1:** Doc updates. **Step 2:** `cargo test --doc --all-features`
  (discovery docs are crate docs). **Step 3:** commit
  `docs(webdav): Document SRV/TXT exclusion per RFC 6764`.

### Task S7.3: `DavItem.principal_url` (P3#17)

**Files:**
- Modify: `src/webdav/types.rs` (field), `src/webdav/streaming.rs` (parse
  `<D:principal-URL><D:href>…</D:href></D:principal-URL>`)
- Test: `tests/unit/webdav/header_helpers_tests.rs` or a new parse test file
  (avoid colliding with S2's file if running concurrently — create
  `tests/unit/webdav/principal_url_tests.rs`)

- [ ] **Step 1:** Failing parse test (present → `Some(href)`; absent →
  `None`). **Step 2:** FAIL. **Step 3:** Implement + doc note: "legacy
  property (RFC 5397 §5.2); RFC 3744/5397 superseded by
  `current-user-principal` for discovery — populated when the server returns
  it". **Step 4:** PASS; commit `feat(webdav): Parse principal-URL into DavItem`.

### Task S7.4: docs + gates

- [ ] CHANGELOG; re-exports for `schedule_tag_from_headers` etc. if S7 lands
  first. Gates. PR `feat/019-s7-tests-docs`.

---

## Integration (coordinator only)

### Task I1: sub-issues + worktrees + dispatch

- [ ] Create 7 sub-issues (`gh issue create --repo Goopil/fast-dav-rs --title
  "0.19/S<n>: …" --body "…"` referencing #225).
- [ ] `git worktree add ../fast-dav-rs-s<n>-<slug> -b feat/019-s<n>-<slug> main`
  × 7.
- [ ] Dispatch 7 `general` subagents in parallel; prompt = sub-issue + stream
  section + Global Constraints + exclusive-file list.

### Task I2: merge-as-ready

- [ ] For each finished stream: review diff, run full gates locally, resolve
  shared-file conflicts (append), `git merge --no-ff` (or rebase then squash
  per repo convention), push, CI 14/14, close the sub-issue.
- [ ] Order hint when several ready: S5 → S4 → S3 → S7 → S2 → S6 → S1.

### Task I3: release 0.19.0

- [ ] Release PR: version bump 0.18.0 → 0.19.0, consolidated CHANGELOG
  (promote `Unreleased`), README/AGENTS sync check.
- [ ] CI 14/14 → tag `v0.19.0` → GitHub Release; crates.io publish by
  maintainer.
- [ ] Close #225; create the follow-up issue `feat(webdav): opt-in `srv`
  feature (DNS SRV/TXT lookup, RFC 6764 §3/§6)` with the design sketch
  (hickory-resolver behind a `Resolver` trait, SRV `_caldav(s)._tcp` +
  TXT context path, fallback well-known unchanged).

## Self-Review notes

- Spec coverage: P1#1→S1, P1#2→S3, P1#3→S2, P1#4→S4.1/S4.2, P1#5→S5,
  P1#6→excluded (S7.2 docs + I3 follow-up issue), P2#7→S6.1/S6.2, P2#8→S4.3,
  P2#9→S6.3, P2#10→S4.4, P2#11→S6.5, P2#12→S6.4, P3#13→S7.1, P3#14→S2.3/S2.4,
  P3#15→S7.2, P3#16→S6.4, P3#17→S7.3. All 17 items covered.
- Type consistency: `CalendarQueryOptions`/`AddressQueryOptions`/`CalendarDataLimits`
  live in `webdav::types` (single source, no caldav/carddav duplication);
  `AcePrincipal`/`Ace`/`build_acl_body` in `webdav::acl`; operation names
  consistent with the existing `Operation` naming style.
- The plan avoids changing any existing signature; `data_element_xml` and
  `build_addressbook_query_body` become delegates of their `_with_*` variants.
