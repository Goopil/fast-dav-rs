//! CalDAV multistatus streaming — thin re-export of the unified parser in
//! [`crate::webdav::streaming`].

pub use crate::webdav::streaming::{
    decode_text, element_from_bytes, parse_multistatus_bytes, parse_multistatus_bytes_visit,
    parse_multistatus_stream, parse_multistatus_stream_visit,
    parse_multistatus_stream_visit_with_timeout, parse_multistatus_stream_with_timeout,
    ElementName, ParseResult, STREAM_READ_IDLE_TIMEOUT,
};
