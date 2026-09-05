//! Fuzz target 2: sync-collection (RFC 6578) response parsing (issue #198).
//!
//! Response side: sync REPORTs are `207` multistatus bodies carrying
//! `<D:sync-token>` elements and per-item `<D:status>` lines (`404`/`410`
//! deletions, `507` truncation) — driven through the same public multistatus
//! parser. `normalize_sync_token` runs on every observed token.
//!
//! Request side: hostile sync tokens / namespaces through
//! `build_sync_collection_body` (escaping surface).
//!
//! Oracle: panics only. The 404/410/507 → `SyncRow` mapping itself
//! (`map_sync_rows`) is `pub(crate)` and reached via HTTP; its semantic
//! behavior is pinned by unit tests and known issues (R27), so semantic
//! wrongness here is out of scope — only crashes count.

#![no_main]

use fast_dav_rs::webdav::streaming::parse_multistatus_bytes;
use fast_dav_rs::webdav::{SyncLevel, build_sync_collection_body, normalize_sync_token};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }

    if let Ok(parsed) = parse_multistatus_bytes(data) {
        for item in &parsed.items {
            std::hint::black_box(&item.href);
            std::hint::black_box(&item.etag);
            std::hint::black_box(&item.status);
            std::hint::black_box(item.is_collection);
            if let Some(token) = &item.sync_token {
                std::hint::black_box(normalize_sync_token(token));
            }
        }
        if let Some(token) = &parsed.sync_token {
            std::hint::black_box(normalize_sync_token(token));
        }
    }

    if let Ok(s) = std::str::from_utf8(data) {
        let token = normalize_sync_token(s);
        let body = build_sync_collection_body(
            Some(&token),
            Some(u32::MAX),
            true,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
            None,
            SyncLevel::One,
        );
        std::hint::black_box(&body);
    }
});
