pub mod builder;
pub mod client;
pub mod streaming;
pub mod types;
pub mod validation;

pub use crate::error::ICalendarViolation;
pub use crate::webdav::sync::{SyncDelta, SyncEntry, SyncSession, SyncSnapshot};
pub use builder::CalDavClientBuilder;
pub use client::{
    CalDavClient, ICAL_CONTENT_TYPE, build_calendar_multiget_body, build_calendar_query_body,
    build_sync_collection_body, map_calendar_list, map_calendar_objects, map_sync_response,
};
pub use streaming::{
    parse_multistatus_bytes, parse_multistatus_bytes_visit, parse_multistatus_stream,
    parse_multistatus_stream_visit, parse_multistatus_stream_visit_with_timeout,
    parse_multistatus_stream_with_timeout,
};
pub use types::{
    BatchItem, CalendarInfo, CalendarObject, CalendarQueryFilter, Collation, DavItem, Depth,
    FreeBusyPeriod, FreeBusyType, MatchType, MediaType, ParamFilter, PropFilter, SyncItem,
    SyncResponse, TextMatch, TimeRange,
};
pub use validation::{ValidationLevel, validate_icalendar};
