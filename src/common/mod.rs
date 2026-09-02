pub mod compression;
pub mod http;

pub use compression::{
    ContentEncoding, add_accept_encoding, add_content_encoding, compress_payload, decompress_body,
    decompress_stream, detect_encoding, detect_encodings,
};

// Internal, cfg-gated tracing macros (single definition site): each expands to
// the matching `tracing` call when the `tracing` feature is enabled and to
// nothing otherwise, so a disabled-feature build contains zero tracing
// references. Call sites stay one-liners; macro arguments are only tokenized
// (never resolved) in the disabled build.
#[cfg(feature = "tracing")]
macro_rules! dav_debug {
    ($($arg:tt)*) => { ::tracing::debug!($($arg)*) };
}
#[cfg(not(feature = "tracing"))]
macro_rules! dav_debug {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! dav_trace {
    ($($arg:tt)*) => { ::tracing::trace!($($arg)*) };
}
#[cfg(not(feature = "tracing"))]
macro_rules! dav_trace {
    ($($arg:tt)*) => {};
}

pub(crate) use dav_debug;
pub(crate) use dav_trace;

/// Replace userinfo credentials in a URL string with `***`.
///
/// Belt-and-braces redaction for logs and error messages: URLs obtained from
/// callers or remote servers (e.g. `Location` redirect targets) may carry
/// `user:password@` even though the builders reject userinfo in base URLs
/// (RFC 9110 §3.2 — senders MUST NOT generate userinfo).
pub(crate) fn redact_userinfo(url: impl std::fmt::Display) -> String {
    let url = url.to_string();
    let Some(scheme_end) = url.find("://") else {
        return url;
    };
    let authority_start = scheme_end + "://".len();
    let authority = &url[authority_start..];
    let authority_len = authority.find(['/', '?', '#']).unwrap_or(authority.len());
    match authority[..authority_len].rfind('@') {
        Some(at) => {
            let mut redacted = String::with_capacity(url.len());
            redacted.push_str(&url[..authority_start]);
            redacted.push_str("***");
            redacted.push_str(&url[authority_start + at..]);
            redacted
        }
        None => url,
    }
}

#[cfg(test)]
mod tests {
    use super::redact_userinfo;

    #[test]
    fn redact_userinfo_replaces_credentials() {
        assert_eq!(
            redact_userinfo("https://user:hunter2@dav.example.com/cal/"),
            "https://***@dav.example.com/cal/"
        );
    }

    #[test]
    fn redact_userinfo_leaves_urls_without_userinfo_unchanged() {
        assert_eq!(
            redact_userinfo("https://dav.example.com/cal/x?y=z#a@b"),
            "https://dav.example.com/cal/x?y=z#a@b"
        );
        assert_eq!(
            redact_userinfo("/relative/path?user=a@b"),
            "/relative/path?user=a@b"
        );
    }

    #[test]
    fn redact_userinfo_handles_bracketed_ipv6_host() {
        assert_eq!(
            redact_userinfo("http://u:p@[::1]:8080/x"),
            "http://***@[::1]:8080/x"
        );
    }
}
