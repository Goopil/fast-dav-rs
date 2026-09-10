#![doc = include_str!("../README.md")]
#![doc = include_str!("../docs/error-handling.md")]
#![doc = include_str!("../docs/advanced-configuration.md")]
#![doc = include_str!("../docs/streaming-and-sync.md")]
#![doc = include_str!("../docs/e2e-testing.md")]

pub mod caldav;
pub mod carddav;
pub mod common;
mod error;
pub mod webdav;

pub use error::{Error, EtagReason, ICalendarViolation, Operation, Result, TokenRefreshReason};

// Backwards-compatible re-exports
pub use caldav::builder::CalDavClientBuilder;
pub use caldav::streaming::{
    parse_multistatus_bytes, parse_multistatus_bytes_visit, parse_multistatus_stream,
    parse_multistatus_stream_visit, parse_multistatus_stream_visit_with_timeout,
    parse_multistatus_stream_with_timeout,
};
// `SyncItem`/`SyncResponse`/`build_sync_collection_body`/`map_sync_response` are deliberately
// NOT re-exported here: CalDAV and CardDAV define distinct same-named items —
// import them from `caldav::` or `carddav::` instead.
pub use caldav::{
    BatchItem, CalDavClient, CalendarInfo, CalendarObject, DavItem, Depth, InboxItem,
    ManagedAttachment, MediaType, ScheduleEndpoints, SchedulingResponse, ValidationLevel,
    build_calendar_multiget_body, build_calendar_query_body, map_calendar_list,
    map_calendar_object, map_calendar_objects, validate_icalendar,
};
pub use carddav::builder::CardDavClientBuilder;
pub use carddav::{
    AddressBookInfo, AddressObject, CardDavClient, CardDavFilter, Collation, MatchType,
    ParamFilter, TextMatch, map_address_object, map_address_objects,
};
pub use common::compression::{
    ContentEncoding, add_accept_encoding, add_content_encoding, compress_payload, detect_encoding,
    detect_encodings, detect_request_compression_preference,
};
pub use webdav::builder::WebDavClientBuilder;
pub use webdav::{
    DavCapabilities, DavCompliance, DavStreamEvent, HyperClient, LockInfo, LockScope,
    OAuth2RefreshProvider, Prefer, Privilege, PropStat, RequestCompressionMode, SyncCapability,
    SyncDelta, SyncEntry, SyncLevel, SyncSession, SyncSnapshot, TokenProvider, WebDavClient,
    WebDavError, discover_caldav, discover_carddav, etag_from_headers, normalize_etag,
    normalize_sync_token, preference_applied_from_headers,
};
