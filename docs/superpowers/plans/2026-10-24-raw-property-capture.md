# Raw vendor-namespace property capture — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture every property of every multistatus response as a `RawProperty { namespace, name, value }` (flattened text), always-on, exposed on `DavItem` + passthrough on `CalendarInfo`/`AddressBookInfo` — so vendor-namespace properties (CalendarServer, Apple iCal, Nextcloud, Infomaniak, …) are readable without crate changes.

**Architecture:** The capture lives in `MultistatusParser` (src/webdav/streaming.rs), which already resolves namespaces (`NsScopes`) and drives `CommonParser` (owner of `DavItemCommon`). A property element is detected by the existing `[Response, Propstat, Prop, X]` stack shape; its text chunks are flattened into an in-progress `RawProperty` and the entry is finalized when the property element closes. Typed fields keep their exact current behavior — the raw list is purely additive.

**Tech Stack:** Rust, quick-xml streaming (existing), hyper 1.x. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-24-vendor-namespace-raw-properties-design.md`

## Global Constraints

- 100% additive API → targets 0.20.0; `cargo semver-checks` must stay clean.
- Always-on capture: no builder option, no feature flag.
- Capture EVERY property (including `getetag`/`displayname`), document order preserved, duplicates kept, empty values recorded as `""`.
- Typed fields are projections: their behavior must not change (the `(foreign-ns)` collision rewrite stays untouched).
- Per-value cap = existing `MAX_ITEM_TEXT_BYTES` (256 MiB, streaming.rs:271); property-count cap = new `MAX_RAW_PROPERTIES` = 1024 per response item.
- Gates (AGENTS.md): `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo nextest run --all-features --locked --test unit_tests`, `cargo test --doc --all-features`, `cargo build --examples --all-features`.
- Conventional commits (≤70 char header), branch `feat/raw-property-capture` (already checked out).
- No TODO/FIXME left; no caldav/carddav duplication (capture lives in webdav/).

---

### Task 1: `RawProperty` type + plumbing on `DavItemCommon`/`DavItem` + re-exports

**Files:**
- Modify: `src/webdav/types.rs` (after `PropStat`, ~line 514; `DavItemCommon` ~921; macro ~944; `DavItem` ~788 + `DavItem::new()` ~869)
- Modify: `src/webdav/mod.rs` (types re-export list ~line 30)
- Modify: `src/lib.rs` (webdav re-export list ~line 45-51)

**Interfaces:**
- Produces: `pub struct RawProperty { pub namespace: Option<String>, pub name: String, pub value: String }` (Debug, Clone, Default, PartialEq, `#[non_exhaustive]`), exported as `fast_dav_rs::webdav::RawProperty` and `fast_dav_rs::RawProperty`. `DavItem.properties: Vec<RawProperty>` and `DavItemCommon.properties: Vec<RawProperty>` (both public).

- [ ] **Step 1: Add the `RawProperty` struct in `src/webdav/types.rs`** right after the `PropStat` struct (line 514, before the `WebDavError` doc comment):

```rust
/// A raw, untyped property captured from a multistatus response.
///
/// Every property element of every `<D:propstat>` is recorded here —
/// including properties in vendor namespaces the crate has no typed
/// support for (CalendarServer, Apple iCal, Nextcloud, …) and properties
/// that also map to typed fields ([`DavItem::etag`], …). Typed fields are
/// projections of convenience; the raw list is stable across crate
/// versions and always reflects the wire.
///
/// ```
/// use fast_dav_rs::RawProperty;
///
/// let prop = RawProperty {
///     namespace: Some("http://calendarserver.org/ns/".to_string()),
///     name: "getctag".to_string(),
///     value: "3119-117".to_string(),
/// };
/// assert_eq!(prop.name, "getctag");
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct RawProperty {
    /// Resolved namespace URI (e.g. `http://calendarserver.org/ns/`).
    /// `None` when the element's prefix is undeclared in scope — the
    /// parser's documented tolerance for servers that emit multistatus
    /// bodies without namespace declarations.
    pub namespace: Option<String>,
    /// Local name, prefix stripped (e.g. `getctag`, `calendar-order`).
    pub name: String,
    /// Flattened text of the property element and its descendants
    /// (entity references and CDATA decoded, child-element text
    /// concatenated), trimmed. Empty for self-closing properties — the
    /// entry still records the property's presence.
    pub value: String,
}
```

- [ ] **Step 2: Add the field to `DavItemCommon`** (line ~940, after `response_status`):

```rust
    /// Raw capture of every property element in every propstat, in
    /// document order (duplicates kept, empty values recorded). See
    /// [`RawProperty`].
    pub properties: Vec<RawProperty>,
```

- [ ] **Step 3: Add the same field to `DavItem`** (after its `response_status` field — same doc comment) and to `DavItem::new()`:

```rust
            properties: Vec::new(),
```

- [ ] **Step 4: Extend the `apply_common_fields!` macro** (add after the `response_status` line):

```rust
        $self.properties = $common.properties;
```

- [ ] **Step 5: Re-export.** In `src/webdav/mod.rs` add `RawProperty` to the `pub use types::{…}` list (alphabetical: after `Privilege`, before `PropStat`). In `src/lib.rs` add `RawProperty` to the `pub use webdav::{…}` list the same way.

- [ ] **Step 6: Verify it compiles + doctest passes**

Run: `cargo build --all-features && cargo test --doc --all-features RawProperty`
Expected: build OK; the `RawProperty` doctest passes.

- [ ] **Step 7: Commit**

```bash
git add src/webdav/types.rs src/webdav/mod.rs src/lib.rs
git commit -m "feat(webdav): Add RawProperty type for raw property capture"
```

---

### Task 2: Capture in `MultistatusParser` + unit wire tests

**Files:**
- Modify: `src/webdav/streaming.rs` (struct `MultistatusParser` ~697, `new()` ~710, `on_start` ~739, `handle_text` ~944, `on_end` ~907; new const near `MAX_ITEM_TEXT_BYTES` ~271)
- Create: `tests/unit/webdav/raw_property_tests.rs`
- Modify: `tests/unit/webdav/mod.rs` (module list, alphabetical)

**Interfaces:**
- Consumes: `RawProperty` (Task 1), `MultistatusParser.raw_prop` state, `append_capped` (existing, streaming.rs), `path_ends_with` (existing), `local_name` (existing).
- Produces: `DavItem.properties` populated for every parsed multistatus (sync + streaming paths — both share `dispatch_event`). `const MAX_RAW_PROPERTIES: usize = 1024;`

- [ ] **Step 1: Write the failing tests** — create `tests/unit/webdav/raw_property_tests.rs`:

```rust
//! Raw property capture (universal vendor-namespace support): every
//! property element of a multistatus must be recorded as a
//! [`RawProperty`] with its resolved namespace, local name and flattened
//! text — independent of typed support.

use fast_dav_rs::webdav::streaming::parse_multistatus_bytes;

/// Multi-vendor multistatus: DAV + CalendarServer + Apple iCal +
/// Nextcloud + a provider-style namespace, with entities, CDATA and a
/// nested child element.
const MULTIVENDOR: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:"
               xmlns:CS="http://calendarserver.org/ns/"
               xmlns:A="http://apple.com/ns/ical/"
               xmlns:NC="http://nextcloud.com/ns"
               xmlns:IK="https://provider.example/dav/ns">
  <D:response>
    <D:href>/calendars/user/work/</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"abc-1"</D:getetag>
        <CS:getctag>3119-117</CS:getctag>
        <A:calendar-order>2</A:calendar-order>
        <NC:fileid>42</NC:fileid>
        <IK:space-id>1&nbsp;234</IK:space-id>
        <CS:me-card><D:href>/principals/test/</D:href></CS:me-card>
        <A:icon><![CDATA[<svg/>]]></A:icon>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

#[test]
fn raw_properties_capture_all_vendor_namespaces_in_document_order() {
    let result = parse_multistatus_bytes(MULTIVENDOR.as_bytes()).unwrap();
    assert_eq!(result.items.len(), 1);
    let props = &result.items[0].properties;
    assert_eq!(
        props,
        &[
            fast_dav_rs::RawProperty {
                namespace: Some("DAV:".to_string()),
                name: "getetag".to_string(),
                value: "\"abc-1\"".to_string(),
            },
            fast_dav_rs::RawProperty {
                namespace: Some("http://calendarserver.org/ns/".to_string()),
                name: "getctag".to_string(),
                value: "3119-117".to_string(),
            },
            fast_dav_rs::RawProperty {
                namespace: Some("http://apple.com/ns/ical/".to_string()),
                name: "calendar-order".to_string(),
                value: "2".to_string(),
            },
            fast_dav_rs::RawProperty {
                namespace: Some("http://nextcloud.com/ns".to_string()),
                name: "fileid".to_string(),
                value: "42".to_string(),
            },
            // `&nbsp;` decoded to U+00A0.
            fast_dav_rs::RawProperty {
                namespace: Some("https://provider.example/dav/ns".to_string()),
                name: "space-id".to_string(),
                value: "1\u{a0}234".to_string(),
            },
            // Descendant text flattened: the child <D:href> contributes
            // its text to the enclosing property's value.
            fast_dav_rs::RawProperty {
                namespace: Some("http://calendarserver.org/ns/".to_string()),
                name: "me-card".to_string(),
                value: "/principals/test/".to_string(),
            },
            fast_dav_rs::RawProperty {
                namespace: Some("http://apple.com/ns/ical/".to_string()),
                name: "icon".to_string(),
                value: "<svg/>".to_string(),
            },
        ]
    );
    // Typed fields remain projections and are unaffected.
    assert_eq!(result.items[0].etag.as_deref(), Some("\"abc-1\""));
}

#[test]
fn raw_property_preserves_true_namespace_on_typed_name_collision() {
    // A foreign-namespace element whose local name collides with a typed
    // element must NOT fill the typed field, but must appear in the raw
    // list with its true namespace and local name.
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:X="urn:evil">
  <D:response>
    <D:href>/cal/a.ics</D:href>
    <D:propstat>
      <D:prop>
        <X:getetag>injected</X:getetag>
        <D:getetag>"real"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    let item = &result.items[0];
    assert_eq!(item.etag.as_deref(), Some("\"real\""), "typed field must stay clean");
    assert_eq!(
        item.properties,
        &[
            fast_dav_rs::RawProperty {
                namespace: Some("urn:evil".to_string()),
                name: "getetag".to_string(),
                value: "injected".to_string(),
            },
            fast_dav_rs::RawProperty {
                namespace: Some("DAV:".to_string()),
                name: "getetag".to_string(),
                value: "\"real\"".to_string(),
            },
        ]
    );
}

#[test]
fn raw_property_self_closing_and_undeclared_prefix() {
    // Self-closing properties are recorded with an empty value (presence
    // signal); an undeclared prefix keeps namespace = None (existing
    // tolerance).
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/></D:resourcetype>
        <X:flag/>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    assert_eq!(
        result.items[0].properties,
        &[
            fast_dav_rs::RawProperty {
                namespace: Some("DAV:".to_string()),
                name: "resourcetype".to_string(),
                // The nested empty <D:collection/> contributes no text.
                value: String::new(),
            },
            fast_dav_rs::RawProperty {
                namespace: None,
                name: "flag".to_string(),
                value: String::new(),
            },
        ]
    );
    assert!(result.items[0].is_collection);
}

#[test]
fn raw_properties_keep_duplicates_across_propstats() {
    let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:CS="http://calendarserver.org/ns/">
  <D:response>
    <D:href>/cal/</D:href>
    <D:propstat>
      <D:prop><CS:getctag>first</CS:getctag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
    <D:propstat>
      <D:prop><CS:getctag>second</CS:getctag></D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
    let result = parse_multistatus_bytes(xml).unwrap();
    let values: Vec<&str> = result.items[0]
        .properties
        .iter()
        .map(|p| p.value.as_str())
        .collect();
    assert_eq!(values, ["first", "second"]);
}

#[test]
fn raw_properties_are_capped_per_response_item() {
    let mut xml = String::from(
        r#"<D:multistatus xmlns:D="DAV:" xmlns:V="urn:v"><D:response><D:href>/c/</D:href><D:propstat><D:prop>"#,
    );
    for i in 0..1100 {
        xml.push_str(&format!("<V:p{i}>x</V:p{i}>"));
    }
    xml.push_str("</D:prop></D:propstat></D:response></D:multistatus>");
    let result = parse_multistatus_bytes(xml.as_bytes()).unwrap();
    assert_eq!(result.items[0].properties.len(), 1024);
}
```

Register the module in `tests/unit/webdav/mod.rs` (alphabetical, after `proxy_parse_tests`):

```rust
pub mod raw_property_tests;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run --all-features --test unit_tests raw_property`
Expected: FAIL — `item.properties` is empty (capture not implemented), struct-literal field count mismatches compile only after Task 1 (already merged).

- [ ] **Step 3: Implement the capture in `src/webdav/streaming.rs`.**

3a. Near `MAX_ITEM_TEXT_BYTES` (line ~271):

```rust
/// Upper bound on raw properties captured per `<D:response>` item
/// (defensive: no real collection reports anywhere near this; bounds
/// memory on hostile bodies).
const MAX_RAW_PROPERTIES: usize = 1024;
```

3b. Add the import of the new type at the top of the file (line 3):

```rust
use crate::webdav::types::{DavItemCommon, Depth, LockInfo, LockScope, PropStat, RawProperty, WebDavError};
```

3c. In `struct MultistatusParser` (after `ns: NsScopes,`):

```rust
    /// In-progress raw property capture; `Some` while inside a property
    /// element's subtree.
    raw_prop: Option<RawProperty>,
```

and in `MultistatusParser::new()`: `raw_prop: None,`.

3d. In `MultistatusParser::on_start`, just before the final `Ok(())` (after the privilege-capture block):

```rust
        // Raw property capture: every direct child of <D:prop> becomes a
        // RawProperty (resolved namespace + local name + flattened text),
        // including properties that also map to typed fields — typed
        // fields are projections, the raw list stays stable across crate
        // versions. Nested elements only deepen the current capture.
        if self.raw_prop.is_none()
            && self.stack.len() >= 4
            && self.stack[self.stack.len() - 4] == ElementName::Response
            && self.stack[self.stack.len() - 3] == ElementName::Propstat
            && self.stack[self.stack.len() - 2] == ElementName::Prop
            && self.common.current.properties.len() < MAX_RAW_PROPERTIES
        {
            let name = binding.as_ref();
            self.raw_prop = Some(RawProperty {
                namespace: self
                    .ns
                    .resolve(name)
                    .map(|ns| String::from_utf8_lossy(ns).into_owned()),
                name: String::from_utf8_lossy(local_name(name)).into_owned(),
                value: String::new(),
            });
        }
```

(`binding` is the existing `let binding = event.name();` at the top of `on_start` — the ORIGINAL element name, before `ns_qualified_name` rewriting, so the true local name is recorded even for collision-rewritten elements.)

3e. In `MultistatusParser::handle_text`, as the FIRST statement (before the `calendar-data` branches — raw capture must see every chunk, including whitespace and the text of typed properties):

```rust
        // Flatten every text chunk into the in-progress raw property
        // (entities/CDATA arrive through the same buffer); typed
        // extraction below continues unchanged.
        if let Some(capture) = self.raw_prop.as_mut() {
            append_capped(&mut capture.value, &text, MAX_ITEM_TEXT_BYTES)?;
        }
```

3f. In `MultistatusParser::on_end`, inside the `if let Some(popped) = self.stack.pop()` block (after the `ElementName::Response` handling):

```rust
            // A raw property completes when its own element closes, i.e.
            // the stack is back to [Response, Propstat, Prop].
            if path_ends_with(
                &self.stack,
                &[ElementName::Response, ElementName::Propstat, ElementName::Prop],
            ) {
                if let Some(mut capture) = self.raw_prop.take() {
                    capture.value = capture.value.trim().to_string();
                    self.common.current.properties.push(capture);
                }
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run --all-features --test unit_tests raw_property`
Expected: PASS (5 tests).

- [ ] **Step 5: Verify no regression in the whole unit suite**

Run: `cargo nextest run --all-features --locked --test unit_tests`
Expected: all PASS (existing streaming/foreign-ns tests unchanged).

- [ ] **Step 6: Lint + format, then commit**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`

```bash
git add src/webdav/streaming.rs tests/unit/webdav/raw_property_tests.rs tests/unit/webdav/mod.rs
git commit -m "feat(webdav): Capture raw properties in multistatus parsing"
```

---

### Task 3: Passthrough on `CalendarInfo` and `AddressBookInfo`

**Files:**
- Modify: `src/caldav/types.rs` (`CalendarInfo`, ~line 34-56) and `src/caldav/client.rs` (`map_calendar_list`, ~1508-1541)
- Modify: `src/carddav/types.rs` (`AddressBookInfo`, ~line 112) and `src/carddav/client.rs` (`map_addressbook_list`, ~970-991)
- Test: `tests/unit/caldav/client_tests.rs`, `tests/unit/carddav/client_tests.rs` (append new tests)

**Interfaces:**
- Consumes: `DavItem.properties` (Task 2), `RawProperty` (Task 1).
- Produces: `CalendarInfo.properties: Vec<RawProperty>`, `AddressBookInfo.properties: Vec<RawProperty>` (public, `#[non_exhaustive]` structs — additive).

- [ ] **Step 1: Write the failing tests.** Append to `tests/unit/caldav/client_tests.rs`:

```rust
#[test]
fn map_calendar_list_passes_raw_properties_through() {
    let mut item = DavItem::new();
    item.is_calendar = true;
    item.properties.push(crate::helpers_or_scope::RawProperty {
        namespace: Some("http://calendarserver.org/ns/".to_string()),
        name: "getctag".to_string(),
        value: "3119-117".to_string(),
    });
    let infos = map_calendar_list(vec![item]);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].properties.len(), 1);
    assert_eq!(infos[0].properties[0].name, "getctag");
}
```

(Adjust the import path to however the test file already imports `DavItem`/`map_calendar_list` — follow the file's existing `use` block; `RawProperty` comes from `fast_dav_rs::RawProperty`.) Append the mirror test to `tests/unit/carddav/client_tests.rs`:

```rust
#[test]
fn map_addressbook_list_passes_raw_properties_through() {
    let mut item = DavItem::new();
    item.is_addressbook = true;
    item.properties.push(fast_dav_rs::RawProperty {
        namespace: Some("http://nextcloud.com/ns".to_string()),
        name: "fileid".to_string(),
        value: "42".to_string(),
    });
    let infos = map_addressbook_list(vec![item]);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].properties[0].name, "fileid");
}
```

- [ ] **Step 2: Run to verify failure** (compile error: no field `properties` on `CalendarInfo`/`AddressBookInfo`).

Run: `cargo nextest run --all-features --test unit_tests passes_raw_properties`

- [ ] **Step 3: Implement.**

`src/caldav/types.rs`, in `CalendarInfo` (after `privileges`):

```rust
    /// Raw capture of every property the server returned for this
    /// collection (vendor namespaces included) — see
    /// [`crate::RawProperty`].
    pub properties: Vec<crate::RawProperty>,
```

`src/caldav/client.rs`, in `map_calendar_list`'s struct literal (after `privileges`):

```rust
                properties: std::mem::take(&mut item.properties),
```

`src/carddav/types.rs`, in `AddressBookInfo` (after `supported_address_data`):

```rust
    /// Raw capture of every property the server returned for this
    /// address book (vendor namespaces included) — see
    /// [`crate::RawProperty`].
    pub properties: Vec<crate::RawProperty>,
```

`src/carddav/client.rs`, in `map_addressbook_list`'s struct literal:

```rust
                properties: item.properties,
```

- [ ] **Step 4: Run to verify pass + no regression**

Run: `cargo nextest run --all-features --locked --test unit_tests`
Expected: all PASS.

- [ ] **Step 5: Lint + format, then commit**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`

```bash
git add src/caldav/types.rs src/caldav/client.rs src/carddav/types.rs src/carddav/client.rs tests/unit/caldav/client_tests.rs tests/unit/carddav/client_tests.rs
git commit -m "feat(caldav,carddav): Expose raw properties on collection info"
```

---

### Task 4: e2e — SabreDAV allprop + provider-A smoke

**Files:**
- Modify: `tests/e2e/sabredav/webdav/discovery_tests.rs` (append test)
- Modify: `tests/e2e/provider_a/mod.rs` (append smoke test in the discovery section)

**Interfaces:**
- Consumes: `WebDavClient::propfind(path, depth, xml_body)` (src/webdav/client.rs:1701), `build_propfind_allprop()` (src/webdav/xml.rs:317), `Depth::One`, `RawProperty` (Task 1).

- [ ] **Step 1: Add the SabreDAV e2e test** to `tests/e2e/sabredav/webdav/discovery_tests.rs`:

```rust
/// Raw property capture (universal vendor-namespace support): an allprop
/// PROPFIND against the live fixture must surface every returned
/// property — names must match the typed propstat listing, and the DAV:
/// namespace must be resolved. SabreDAV returns structured properties
/// too (e.g. `resourcetype` with child elements): their flattened text
/// is empty but the entry must still be present.
#[tokio::test]
async fn test_allprop_propfind_captures_raw_properties() {
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let resp = client
        .propfind(
            "/calendars/test/",
            fast_dav_rs::Depth::One,
            &fast_dav_rs::webdav::build_propfind_allprop(),
        )
        .await
        .expect("allprop PROPFIND must succeed");
    assert_eq!(resp.status(), 207);
    let parsed = fast_dav_rs::webdav::streaming::parse_multistatus_bytes(&resp.into_body())
        .expect("body must parse");
    assert!(!parsed.items.is_empty());
    for item in &parsed.items {
        let names: Vec<&str> = item
            .propstats
            .iter()
            .flat_map(|ps| ps.prop_names.iter().map(String::as_str))
            .collect();
        assert_eq!(
            item.properties.len(),
            names.len(),
            "every property element must be captured exactly once for {}",
            item.href
        );
        for prop in &item.properties {
            assert_eq!(prop.namespace.as_deref(), Some("DAV:"));
        }
    }
}
```

(If the fixture's calendar path differs, reuse the path constant the file's other tests use; `/calendars/test/` is the SabreDAV fixture default.)

- [ ] **Step 2: Add the provider-A smoke** to `tests/e2e/provider_a/mod.rs`, next to the existing discovery smoke (same env gating as the file's other tests):

```rust
/// Raw capture smoke: an allprop PROPFIND must return at least one raw
/// property whose namespace resolves — the empirical inventory of
/// provider-specific properties this feature exists for.
#[tokio::test]
async fn test_allprop_captures_raw_properties() {
    if std::env::var("PROVIDER_A_DAV_URL").is_err() {
        eprintln!("Skipping: PROVIDER_A_DAV_URL not set");
        return;
    }
    let client = provider_a_client();
    let resp = client
        .propfind(
            &provider_a_calendar_path(),
            fast_dav_rs::Depth::One,
            &fast_dav_rs::webdav::build_propfind_allprop(),
        )
        .await
        .expect("allprop PROPFIND must succeed");
    let parsed = fast_dav_rs::webdav::streaming::parse_multistatus_bytes(&resp.into_body())
        .expect("body must parse");
    assert!(
        parsed.items.iter().any(|item| !item.properties.is_empty()),
        "expected at least one captured raw property"
    );
}
```

(Match the file's existing client/path helpers — replace `provider_a_client()`/`provider_a_calendar_path()` with the helpers already defined in `tests/e2e/provider_a/mod.rs`; do not invent new auth plumbing.)

- [ ] **Step 3: Run**

Run: `./radicale-test/setup.sh >/dev/null 2>&1; ./sabredav-test/setup.sh >/dev/null 2>&1; cargo test --test e2e_tests test_allprop_propfind_captures_raw_properties`
Expected: PASS against the live SabreDAV fixture. (Provider A smoke runs only with `PROVIDER_A_DAV_URL` set: `cargo test --test e2e_provider_a_smoke -- --ignored`.)

- [ ] **Step 4: Commit**

```bash
git add tests/e2e/sabredav/webdav/discovery_tests.rs tests/e2e/provider_a/mod.rs
git commit -m "test(e2e): Assert raw property capture on live fixtures"
```

---

### Task 5: Documentation

**Files:**
- Modify: `README.md` (feature list + short example near the existing PROPFIND helper example ~523-532)
- Modify: `docs/advanced-configuration.md` (new "Vendor namespace properties" section)
- Modify: `CHANGELOG.md` (new `## [Unreleased]` section above `## [0.19.0]`)

- [ ] **Step 1: README.** Add a bullet to the feature list and a short example (keep it 10-15 lines; use `no_run` if it needs a live server, mirroring the style of neighboring examples):

```rust
// Every property — vendor namespaces included — is captured raw:
for prop in &calendar.properties {
    println!("{} {} = {}", prop.namespace.as_deref().unwrap_or(""), prop.name, prop.value);
}
```

- [ ] **Step 2: `docs/advanced-configuration.md`.** Add a "Vendor namespace properties" section after the SRV/TXT exclusion notes (~line 203-211):

```markdown
## Vendor namespace properties

CalDAV/CardDAV servers attach vendor-specific properties to collections —
CalendarServer (`http://calendarserver.org/ns/`: `getctag`, `me-card`,
`push-*`), Apple iCal (`http://apple.com/ns/ical/`: `calendar-order`,
`refreshrate`, `timezone-id`), Nextcloud (`http://nextcloud.com/ns`:
`fileid`, …), or your provider's own namespace.

The crate captures **every** property of every multistatus response as a
`RawProperty { namespace, name, value }` — always on, no configuration:

- `value` is the flattened text of the property element (entities and CDATA
  decoded, child-element text concatenated, trimmed).
- Document order is preserved; duplicates are kept; empty values are
  recorded (a self-closing property still signals presence).
- Typed fields (`etag`, `displayname`, `calendar_color`, …) remain
  projections of convenience; the raw list is stable across crate versions.
- Capture is read-side only. To *write* vendor properties, use the raw-body
  escape hatches (`WebDavClient::proppatch`, `mkcol(Option<&str>)`).

Available on every `DavItem`, plus `CalendarInfo.properties` and
`AddressBookInfo.properties` from `list_calendars`/`list_addressbooks`.

To discover what a server exposes, send an allprop PROPFIND
(`build_propfind_allprop`) and inspect `item.properties`. References for
known vendor properties: CalendarServer extension docs
(`github.com/apple/ccs-calendarserver/doc/Extensions/`), sabre.io plugin
docs, Nextcloud `apps/dav/lib/Connector/Sabre/`, and
`davx5.com/tested-with` (which lists servers with vendor extensions).
```

- [ ] **Step 3: CHANGELOG.** New section:

```markdown
## [Unreleased]

### Added

- Universal raw property capture (vendor namespaces): every property of a
  multistatus response is recorded as `RawProperty { namespace, name, value }`
  (flattened text, document order, duplicates kept, empty values recorded) —
  on `DavItem` and passed through to `CalendarInfo`/`AddressBookInfo`.
  Always-on; typed fields are unchanged projections. Read-side only.
```

- [ ] **Step 4: Verify doctests + commit**

Run: `cargo test --doc --all-features`

```bash
git add README.md docs/advanced-configuration.md CHANGELOG.md
git commit -m "docs: Document raw vendor property capture"
```

---

### Task 6: Full gates + version bump check

**Files:** none (verification only)

- [ ] **Step 1: All gates**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --locked --test unit_tests
cargo test --doc --all-features
cargo build --examples --all-features --locked
```
Expected: all green.

- [ ] **Step 2: Semver check** (additive change must be clean)

Run: `cargo semver-checks 2>&1 | tail -5`
Expected: pass (new public struct + new fields on `#[non_exhaustive]` structs = minor-compatible).

- [ ] **Step 3: No stray TODOs**

Run: `grep -rn "TODO\|FIXME" src/ tests/unit/webdav/raw_property_tests.rs | grep -v target`
Expected: no new matches introduced by this change.

- [ ] **Step 4: Push + open PR** (title: `feat: Universal raw vendor-namespace property capture`; body references the spec doc; note 0.20.0 target)

```bash
git push -u origin feat/raw-property-capture
gh pr create --title "feat: Universal raw vendor-namespace property capture" --fill
```

---

## Self-review notes

- Spec coverage: RawProperty type (T1), capture semantics incl. flattening/empty/order/duplicates/foreign-ns/undeclared-ns (T2), caps (T2), CalendarInfo/AddressBookInfo passthrough (T3), e2e allprop + provider A smoke (T4), README + advanced-configuration recipe + CHANGELOG (T5), write-side explicitly out of scope (documented in T5 recipe).
- Type consistency: `RawProperty` field names identical across tasks (`namespace`, `name`, `value`); `MAX_RAW_PROPERTIES` defined in T2 and used only there.
- The capture reads `self.common.current.properties` — `CommonParser.current` is crate-private but both structs live in the same module (streaming.rs), so access is legal.
- Order-of-events check: `dispatch_event` flushes text BEFORE start/end handlers, so `handle_text` sees the stack state of the element containing the text; `Event::Empty` → `on_start` + `on_end` immediately, which finalizes self-closing properties with `value: ""`.
