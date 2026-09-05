//! Fuzz target 3: ETag / header grammar + URI helpers (issue #198).
//!
//! Surfaces: `normalize_etag`, `etag_from_headers` (RFC 9110 §8.8.3 entity
//! tags), `parse_dav_header` (DAV capability header), `preference_applied_from_headers`
//! (RFC 7240), `encode_path_segments`, `resolve_location` (RFC 3986 reference
//! resolution), `build_uri`, `same_origin`, `is_https_to_http_downgrade`.
//!
//! Oracle: panics only. Known semantic gaps in `resolve_location` (dot-segment
//! removal, network-path references — roast R17/R18) are documented issues,
//! not crashes: a wrong-but-total result is not a finding here.

#![no_main]

use fast_dav_rs::webdav::client::{
    encode_path_segments, is_https_to_http_downgrade, resolve_location, same_origin,
};
use fast_dav_rs::webdav::{
    WebDavClient, etag_from_headers, normalize_etag, parse_dav_header,
    preference_applied_from_headers,
};
use hyper::header::{HeaderMap, HeaderValue};
use libfuzzer_sys::fuzz_target;
use std::sync::LazyLock;

static CLIENT: LazyLock<WebDavClient> = LazyLock::new(|| {
    WebDavClient::new("https://dav.example.com/dav/", None, None).expect("static base is valid")
});

static BASE_URI: LazyLock<hyper::Uri> = LazyLock::new(|| {
    "https://dav.example.com/dav/cal/"
        .parse()
        .expect("static base is valid")
});

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // ETag / header grammar.
    std::hint::black_box(normalize_etag(s));
    if let Ok(value) = HeaderValue::from_str(s) {
        let mut headers = HeaderMap::new();
        headers.insert(hyper::header::ETAG, value.clone());
        std::hint::black_box(etag_from_headers(&headers));
        headers.insert("Preference-Applied", value);
        std::hint::black_box(preference_applied_from_headers(&headers));
    }
    let _ = std::hint::black_box(parse_dav_header(s));

    // URI helpers.
    std::hint::black_box(encode_path_segments(s));
    if let Some(resolved) = resolve_location(&BASE_URI, s) {
        std::hint::black_box(&resolved);
    }
    if let Ok(uri) = CLIENT.build_uri(s) {
        std::hint::black_box(&uri);
        if let Ok(other) = s.parse::<hyper::Uri>() {
            std::hint::black_box(same_origin(&uri, &other));
            std::hint::black_box(is_https_to_http_downgrade(&uri, &other));
        }
    }
});
