//! Fuzz target 1: multistatus / PROPFIND / REPORT / lock-discovery / error
//! XML parsing (issue #198).
//!
//! Oracle: panics only. All entry points return `Result` — any `Err` is
//! expected on hostile input and ignored. A crash, hang, or OOM is a finding.
//!
//! The async streaming entry points (`parse_multistatus_stream*`) need a
//! `hyper::body::Incoming`, which has no public constructor from raw bytes;
//! they feed the exact same `MultistatusParser` state machine as the `*_bytes`
//! entry points below, so the parser core is fully covered here.

#![no_main]

use fast_dav_rs::webdav::streaming::{
    decode_text, parse_error_body, parse_lock_discovery_bytes, parse_multistatus_bytes,
    parse_multistatus_bytes_visit,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Cap input processing so hostile XML cannot burn the fuzzer.
    if data.len() > 1_048_576 {
        return;
    }

    if let Ok(parsed) = parse_multistatus_bytes(data) {
        std::hint::black_box(&parsed.items);
        std::hint::black_box(parsed.sync_token);
    }

    // Visit variant exercises the sink-callback dispatch path.
    let _ = parse_multistatus_bytes_visit(data, |item| {
        std::hint::black_box(&item);
        Ok(())
    });

    let _ = std::hint::black_box(parse_lock_discovery_bytes(data));
    let _ = std::hint::black_box(parse_error_body(data));
    let _ = std::hint::black_box(decode_text(data));
});
