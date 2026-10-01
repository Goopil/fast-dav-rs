//! CalDAV multistatus streaming — thin re-export of the unified parser in
//! [`crate::webdav::streaming`].

// The `parse_multistatus_stream*` family is deprecated at the source; the
// re-exports stay until the legacy surface is pruned (they keep propagating
// the deprecation to callers).
#[allow(deprecated)]
pub use crate::webdav::streaming::{
    ElementName, ParseResult, STREAM_READ_IDLE_TIMEOUT, decode_text, element_from_bytes,
    parse_multistatus_bytes, parse_multistatus_bytes_visit, parse_multistatus_stream,
    parse_multistatus_stream_visit, parse_multistatus_stream_visit_with_timeout,
    parse_multistatus_stream_with_timeout,
};
