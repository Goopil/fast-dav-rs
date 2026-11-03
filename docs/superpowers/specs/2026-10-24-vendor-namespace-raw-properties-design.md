# Universal raw vendor-namespace property capture — 0.20.0 design

## Goal

Make every property returned by a multistatus (PROPFIND, REPORT, multiget…)
readable — including properties in vendor namespaces the crate has never heard
of (CalendarServer, Apple iCal, Nextcloud/ownCloud, SabreDAV, Infomaniak, …) —
without adding typed fields per vendor.

Today only property *names* survive for unknown/foreign properties
(`PropStat.prop_names`); values are silently dropped, and the `(foreign-ns)`
collision rewrite destroys the namespace URI. The fix: universal raw capture,
always-on, additive.

## Decisions (validated with maintainer, 2026-10-23)

- **Universal raw access** (not per-vendor typing, not hybrid).
- **Flattened text value** — descendant text concatenated, entities/CDATA
  decoded (e.g. `me-card` → its `D:href` child's text).
- **Always-on capture** — no builder option, near-zero cost (empty `Vec` when
  nothing extra arrives).
- **Capture everything** — including properties that also map to typed fields
  (`getetag`, `displayname`, …). Typed fields remain projections; the raw list
  is stable across crate versions (a property never "leaves" the raw list when
  it gains a typed field).
- **Write side out of scope** — vendor request bodies keep using existing
  escape hatches (`mkcol(body)`, `acl(body)`, raw `send()`).

## Public API (all additive → 0.20.0)

```rust
/// A raw, untyped property captured from a multistatus response.
#[derive(Debug, Clone, PartialEq)]
pub struct RawProperty {
    /// Resolved namespace URI (`None` = undeclared prefix — existing
    /// tolerance preserved).
    pub namespace: Option<String>,
    /// Local name, prefix stripped.
    pub name: String,
    /// Flattened descendant text (entities/CDATA decoded), capped like other
    /// text fields.
    pub value: String,
}
```

- `RawProperty` on `DavItemCommon` → available on every `DavItem` (events,
  contacts, calendars discovery, inbox items, …).
- Passthrough on `CalendarInfo` and `AddressBookInfo` (where vendor props
  live: colors, order, ctags, provider-specific props).
- Re-exports: `src/webdav/mod.rs` + `src/lib.rs` (modular + legacy pattern).

## Capture semantics

- One `RawProperty` per property element in each propstat; document order
  preserved; duplicates kept (multiple propstats are real wire behavior).
- Namespace resolved via the existing `NsScopes` prefix→URI map.
- Self-closing / empty properties yield `value: ""` — the entry still exists
  (presence signal, e.g. server flags).
- The `(foreign-ns)` typed-dispatch mechanism is **unchanged**: a foreign
  `{ns}getetag` still cannot pollute typed fields, but now appears in the raw
  list with its true resolved namespace.
- Defensive caps in the existing spirit: `MAX_ITEM_TEXT_BYTES` per value, plus
  a cap on the number of captured properties per response item.

## Tests

- Unit wire fixtures (multivendor): `calendarserver.org/ns/`,
  `http://apple.com/ns/ical/`, `http://nextcloud.com/ns`,
  provider-A-style namespace — asserting namespace + name + value, document
  order, duplicates across propstats, collision case (true ns preserved,
  typed fields unpolluted), undeclared-namespace tolerance, self-closing
  empty value, caps.
- e2e: PROPFIND `allprop` on SabreDAV (emits sabre-namespaced props,
  CalendarServer `getctag`, Apple `calendar-order`). The provider-A tier
  stays credential-free by design (unauthenticated shape recording), so
  vendor-property inventory on provider A is done manually with real
  credentials — an unauthenticated PROPFIND there would only 401.
- All existing tests must stay green (purely additive change).

## Docs

- README section (feature + short example).
- Vendor-namespaces recipe in `docs/advanced-configuration.md`, with the
  sources table: `github.com/apple/ccs-calendarserver/doc/Extensions/`,
  sabre.io, Nextcloud `apps/dav/lib/Connector/Sabre/`,
  `davx5.com/tested-with`, and the empirical allprop method (no public list
  for Infomaniak → probe the provider-A fixture).
- `CHANGELOG.md` entry under 0.20.0.

## Compatibility

100% additive: new struct, new fields on structs, new re-exports. `cargo
semver-checks` must pass. Release target 0.20.0 (optionally combined with
#254 `srv`).
