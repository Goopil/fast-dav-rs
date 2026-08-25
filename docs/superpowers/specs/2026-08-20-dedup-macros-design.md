# Design: Reduce Code Duplication via Macros + Traits (Hybrid Approach)

**Date:** 2026-08-20  
**Status:** Approved  
**Branch:** `typed-errors`

## Context

The `fast-caldav-rs` crate implements three DAV protocol clients: WebDAV (base), CalDAV, and CardDAV. CalDAV and CardDAV clients are thin wrappers around `WebDavClient`. A first step toward deduplication was taken with the `impl_dav_builder!` macro (`src/webdav/builder.rs:559-656`) which generates `CalDavClientBuilder` and `CardDavClientBuilder` from a single canonical `WebDavClientBuilder`.

Despite this, significant duplication remains across four areas:

| Area | Files | Duplicated Lines | Type |
|------|-------|-----------------|------|
| Streaming parsers | `caldav/streaming.rs` (509) vs `carddav/streaming.rs` (509) | ~400 (~80%) | Verbatim |
| Client delegation boilerplate | `caldav/client.rs` vs `carddav/client.rs` | ~150 per file (~20 methods) | Verbatim |
| DavItem types | `caldav/types.rs` (106) vs `carddav/types.rs` (103) | ~60 per file | Near-identical |
| XML body builders + mapping functions | `caldav/client.rs` vs `carddav/client.rs` | ~100 total | Near-identical |

**Total: ~800 lines duplicated, ~1093 lines eliminable after refactoring.**

## Goal

Reduce duplication to the maximum extent while:
- Preserving **100% of the public API** (zero breaking changes)
- Preserving all features (streaming, compression, batch, ETag, sync, etc.)
- Maintaining `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` passing

## Approach: Trait `DavProtocol` + Macros (Hybrid)

### Architecture Overview

```
src/
├── webdav/
│   ├── streaming.rs       # NEW: trait DavProtocol, MultistatusParser<P, C>,
│   │                        #   generic parse_multistatus_* functions,
│   │                        #   ItemConsumer, ParseResult, decode_text, STREAM_READ_IDLE_TIMEOUT
│   ├── types.rs            # NEW: macro define_dav_item! + existing DavItemCommon
│   ├── xml.rs              # NEW: build_multiget_body (parameterized)
│   ├── builder.rs          # EXISTING: impl_dav_builder! (unchanged) + NEW: impl_dav_client!
│   └── client.rs           # EXISTING (unchanged)
├── caldav/
│   ├── streaming.rs        # REDUCED: ElementName enum + CalDavProtocol impl (~50 lines)
│   ├── types.rs            # REDUCED: define_dav_item! invocation (~15 lines) + CalendarInfo/CalendarObject/SyncItem/SyncResponse
│   ├── client.rs           # REDUCED: impl_dav_client! invocation + protocol-specific methods
│   └── builder.rs          # EXISTING: impl_dav_builder! (unchanged)
├── carddav/
│   ├── streaming.rs        # REDUCED: ElementName enum + CardDavProtocol impl (~50 lines)
│   ├── types.rs            # REDUCED: define_dav_item! invocation (~15 lines) + AddressBookInfo/AddressObject/SyncItem/SyncResponse
│   ├── client.rs           # REDUCED: impl_dav_client! invocation + protocol-specific methods
│   └── builder.rs          # EXISTING: impl_dav_builder! (unchanged)
└── common/                 # EXISTING (unchanged)
```

---

## Section 1: Trait `DavProtocol` for Streaming Parser

### Problem

`caldav/streaming.rs` and `carddav/streaming.rs` share ~400 lines verbatim. Only three things differ:
1. `on_start` — handling protocol-specific elements (Calendar/Comp vs Addressbook/AddressDataType)
2. `handle_text` — handling protocol-specific text (calendar_data/calendar_description vs address_data/addressbook_description)
3. `element_from_bytes` — mapping element names to enum variants

### Solution

Define a `DavProtocol` trait in `src/webdav/streaming.rs` that encapsulates the protocol-specific hooks. The `MultistatusParser` becomes generic over `P: DavProtocol`.

```rust
/// Protocol-specific hooks for multistatus parsing.
pub(crate) trait DavProtocol {
    /// The element name enum (common + protocol-specific variants).
    type Element: PartialEq + Copy + std::fmt::Debug;

    /// Convert a raw element name to a variant.
    fn element_from_bytes(raw: &[u8]) -> Self::Element;

    /// Returns true if the element is `Response` (triggers new item creation).
    fn is_response(elem: &Self::Element) -> bool;

    /// Called on `Start` for protocol-specific elements.
    /// Common elements are already handled by `CommonParser`.
    fn on_start(
        &mut self,
        elem: Self::Element,
        event: &BytesStart<'_>,
        decoder: Decoder,
        stack: &[Self::Element],
        current: &mut Self::Item,
    ) -> Result<()>;

    /// Called for protocol-specific text.
    /// Common fields are already handled by `CommonParser`.
    fn on_text(
        &mut self,
        text: &str,
        stack: &[Self::Element],
        current: &mut Self::Item,
    );

    /// The item type produced by this protocol's parser.
    type Item: Default;
}
```

### Generic MultistatusParser

```rust
pub(crate) struct MultistatusParser<P: DavProtocol, C: ItemConsumer<Item = P::Item>> {
    stack: Vec<P::Element>,
    current: P::Item,
    sync_token: Option<String>,
    common: CommonParser,
    protocol: P,
    sink: C,
}
```

### What stays protocol-specific (in `caldav/streaming.rs` and `carddav/streaming.rs`)

- The `ElementName` enum (common + protocol-specific variants)
- `element_from_bytes` (common match arms + protocol-specific match arms)
- `impl DavProtocol for CalDavProtocol` / `CardDavProtocol` (~50 lines each: `on_start` + `on_text`)
- Thin wrapper functions `parse_multistatus_stream`, `parse_multistatus_bytes`, etc. (~5 lines each) that instantiate the generic version with the protocol-specific type

### What is unified in `webdav/streaming.rs`

- `ItemConsumer` trait + impls (~20 lines)
- `ParseResult<C>` struct (~5 lines)
- `MultistatusParser` struct + `new`/`finish`/`on_end` (~40 lines)
- `parse_multistatus_stream_with` / `parse_multistatus_bytes_with` (~100 lines)
- The 6 public wrapper functions, now generic (~80 lines)
- `decode_text` (~5 lines)
- `STREAM_READ_IDLE_TIMEOUT` constant (~1 line)

### Estimated savings

~400 duplicated lines → ~250 shared + ~100 protocol-specific = **~350 lines eliminated**

---

## Section 2: Macro `impl_dav_client!` for Delegation

### Problem

~20 delegation methods are copied verbatim between `CalDavClient` and `CardDavClient`:
- `new`, `builder`, `from_webdav`
- 6 compression methods (`set_request_compression`, etc.)
- `build_uri`
- `send`, `send_stream`
- 10 HTTP verbs (`options`, `head`, `get`, `delete`, `delete_if_match`, `copy`, `move`, `propfind`, `proppatch`, `report`, `mkcol`)
- `propfind_many`, `report_many`
- `supports_webdav_sync`, `propfind_stream`, `report_stream`
- `etag_from_headers`, `normalize_etag`, `normalize_sync_token`

### Solution

A macro `impl_dav_client!` that generates all pure delegation methods. It takes the client name, the builder name, and generates the struct + all delegation methods.

```rust
#[macro_export]
macro_rules! impl_dav_client {
    (
        $(#[$meta:meta])*
        $vis:vis struct $client:ident {
            webdav: $webdav:ty;
        }
        builder = $builder:ident;
    ) => {
        $(#[$meta])*
        $vis struct $client {
            webdav: $webdav,
        }

        impl $client {
            // Constructors
            pub fn new(base_url: &str, basic_user: Option<&str>, basic_pass: Option<&str>)
                -> crate::Result<Self> { ... }
            pub fn builder(base_url: impl Into<String>) -> $builder { ... }
            pub(crate) fn from_webdav(webdav: $webdav) -> Self { ... }

            // Compression (6 methods)
            // Build URI
            // Send / Send stream
            // HTTP verbs (10+ methods)
            // Batch (propfind_many, report_many)
            // Streaming helpers (supports_webdav_sync, propfind_stream, report_stream)
            // ETag helpers (etag_from_headers, normalize_etag, normalize_sync_token)
        }
    };
}
```

### What stays protocol-specific (not generated by the macro)

- `put` / `put_if_match` / `put_if_none_match` — Content-Type differs (`text/calendar` vs `text/vcard`) + parameter name
- `mkcalendar` (CalDAV only) / `mkaddressbook` (CardDAV only, with MKCOL fallback)
- `discover_calendar_home_set` / `discover_addressbook_home_set`
- `list_calendars` / `list_addressbooks`
- `calendar_query_timerange` / `addressbook_query` (+ UID/EMAIL/FN helpers)
- `calendar_multiget` / `addressbook_multiget`
- `sync_collection`

### Note on `discover_current_user_principal`

This method is verbatim between caldav and carddav. With the generic `parse_multistatus_bytes` (Section 1), it can potentially be generated by the macro. This will be evaluated during implementation — if the generic parser makes it straightforward, it will be included in the macro; otherwise it stays as a protocol-specific method.

### Estimated savings

~150 lines × 2 = 300 lines → ~100 lines (macro) + ~4 lines (2 invocations) = **~196 lines eliminated**

---

## Section 3: Macro `define_dav_item!` for Flat DavItem Types

### Problem

`caldav/types.rs` and `carddav/types.rs` each define a `DavItem` struct with 10 common fields + 5-7 protocol-specific fields. The `new()` constructor, `apply_common()` method, and `Default` impl are near-identical.

### Solution

A macro `define_dav_item!` in `src/webdav/types.rs` that generates the common fields, `new()`, `apply_common()`, and `Default`, while the caller provides only the protocol-specific fields.

```rust
#[macro_export]
macro_rules! define_dav_item {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $($ext_field:vis $ext_name:ident : $ext_ty:ty),* $(,)?
        }
    ) => {
        $(#[$meta])*
        pub struct $name {
            // Common fields (generated)
            pub href: String,
            pub status: Option<String>,
            pub displayname: Option<String>,
            pub etag: Option<String>,
            pub is_collection: bool,
            pub current_user_principal: Vec<String>,
            pub owner: Option<String>,
            pub sync_token: Option<String>,
            pub content_type: Option<String>,
            pub last_modified: Option<String>,
            // Protocol-specific fields (provided by caller)
            $($ext_field $ext_name: $ext_ty),*
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    href: String::new(),
                    status: None,
                    displayname: None,
                    etag: None,
                    is_collection: false,
                    current_user_principal: Vec::new(),
                    owner: None,
                    sync_token: None,
                    content_type: None,
                    last_modified: None,
                    $($ext_name: Default::default()),*
                }
            }

            pub(crate) fn apply_common(&mut self, common: $crate::webdav::types::DavItemCommon) {
                self.href = common.href;
                self.status = common.status;
                self.displayname = common.displayname;
                self.etag = common.etag;
                self.is_collection = common.is_collection;
                self.sync_token = common.sync_token;
                self.current_user_principal = common.current_user_principal;
                self.owner = common.owner;
                self.content_type = common.content_type;
                self.last_modified = common.last_modified;
            }
        }

        impl Default for $name {
            fn default() -> Self { Self::new() }
        }
    };
}
```

### Usage

```rust
// caldav/types.rs
define_dav_item! {
    /// Item extracted from a WebDAV response
    pub struct DavItem {
        pub is_calendar: bool,
        pub supported_components: Vec<String>,
        pub calendar_data: Option<String>,
        pub calendar_home_set: Vec<String>,
        pub calendar_description: Option<String>,
        pub calendar_timezone: Option<String>,
        pub calendar_color: Option<String>,
    }
}
```

### API compatibility

The generated struct is **identical** to the current hand-written struct. All field accesses (`item.href`, `item.calendar_data`, `item.is_calendar`) work exactly the same. **Zero breaking change.**

### Other types

`CalendarInfo`/`AddressBookInfo`, `CalendarObject`/`AddressObject`, `SyncItem`, `SyncResponse` remain protocol-specific (they differ in field names and types). `SyncResponse` is identical and could be shared, but the effort is minimal (~5 lines) and not worth the complexity.

### Estimated savings

~60 lines × 2 = 120 lines → ~50 lines (macro) + ~15 × 2 (invocations) = **~40 lines eliminated**

---

## Section 4: Shared XML Body Builders and Mapping Functions

### 4a. `build_multiget_body` parameterized

Follow the existing pattern of `build_sync_collection_body` which already takes `namespace` and `data_element` parameters.

```rust
// src/webdav/xml.rs — NEW
pub fn build_multiget_body<I, S>(
    hrefs: I,
    include_data: bool,
    root_element: &str,    // "calendar-multiget" or "addressbook-multiget"
    data_element: &str,    // "calendar-data" or "address-data"
    namespace: &str,       // "urn:ietf:params:xml:ns:caldav" or carddav
) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{ ... }
```

Protocol-specific wrappers become one-liners:
```rust
// caldav/client.rs
pub fn build_calendar_multiget_body<I, S>(hrefs: I, include_data: bool) -> Option<String> {
    crate::webdav::xml::build_multiget_body(hrefs, include_data,
        "calendar-multiget", "calendar-data", "urn:ietf:params:xml:ns:caldav")
}
```

### 4b. `discover_current_user_principal` moved to shared code

This method is verbatim between caldav and carddav. The XML body and parsing logic are protocol-agnostic (only uses `current_user_principal` from `DavItemCommon`).

With the generic `parse_multistatus_bytes` from Section 1, this method can be generated by `impl_dav_client!` or implemented as a shared method. Decision deferred to implementation phase.

### 4c. `map_sync_response` generalized

`map_sync_response` differs only by:
- The data field (`calendar_data` vs `address_data`)
- The return type (`SyncItem` with `calendar_data` vs `address_data`)

Options:
1. A macro `define_map_sync_response!` that takes the field name as parameter
2. A generic function with a closure/trait for data extraction
3. Keep two protocol-specific implementations (simplest, ~40 lines each)

Given the complexity vs. savings trade-off, option 1 (macro) is preferred if it cleanly integrates; otherwise option 3 is acceptable.

### 4d. `escape_xml` wrapper simplification

The `escape_xml()` wrappers in `caldav/client.rs` and `carddav/client.rs` are one-liner delegations to `webdav::xml::escape_xml`. If they are part of the public API, keep them as re-exports. If internal only, callers can use `crate::webdav::xml::escape_xml` directly.

### Estimated savings

~100 lines eliminated (multiget body ~30, discover_principal ~28, map_sync ~40, escape_xml ~4)

---

## Summary

### Estimated line savings

| Area | Before | After | Eliminated |
|------|--------|-------|------------|
| Streaming parsers | 1018 (509×2) | ~350 (250 shared + 50×2) | ~668 |
| Client delegation | ~300 (150×2) | ~104 (100 macro + 2×2) | ~196 |
| DavItem types | ~209 (106+103) | ~80 (50 macro + 15×2) | ~129 |
| XML builders + mapping | ~200 (100×2) | ~100 (60 shared + 20×2) | ~100 |
| **Total** | **~1727** | **~634** | **~1093 lines** |

### Constraints preserved

- Zero breaking change on public API
- All features preserved (streaming, compression, batch, ETag, sync, conditional writes)
- `cargo fmt` passes
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- `cargo test --all-features` passes
- `cargo test --doc` passes
- All public APIs have documentation
- No TODO/FIXME comments in final code

### Implementation order (suggested)

1. `define_dav_item!` macro (Section 3) — foundational, unblocks streaming
2. `DavProtocol` trait + generic streaming (Section 1) — largest savings
3. `impl_dav_client!` macro (Section 2) — builds on streaming generics
4. XML builders + mapping (Section 4) — finishing touches
5. Full test suite verification at each step

### Risk mitigation

- Each section can be implemented and tested independently
- If any section proves too complex, it can be deferred without blocking others
- The `define_dav_item!` macro is the safest starting point (pure mechanical generation)
- The streaming trait is the highest-value but also highest-complexity change
