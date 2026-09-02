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
