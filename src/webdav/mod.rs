pub mod auth;
pub mod builder;
pub mod client;
pub mod discovery;
pub(crate) mod multiget;
pub mod retry;
pub mod streaming;
pub mod sync;
pub mod types;
pub mod xml;

pub use crate::common::http::HyperClient;
pub use auth::{OAuth2RefreshProvider, TokenProvider};
pub use builder::WebDavClientBuilder;
pub use client::{
    RequestCompressionMode, WebDavClient, conditional_write_error, etag_from_headers,
    if_header_for_lock_token, normalize_etag, normalize_sync_token,
    preference_applied_from_headers, schedule_tag_from_headers,
};
pub use discovery::{discover_caldav, discover_carddav};
pub use streaming::{
    DavStreamEvent, ItemStream, STREAM_READ_IDLE_TIMEOUT, multistatus_events,
    multistatus_events_with_timeout, parse_error_body, parse_lock_discovery_bytes,
};
pub use sync::{SyncDelta, SyncEntry, SyncSession, SyncSnapshot};
pub use types::CalendarDataLimits;
pub use types::{
    AddressQueryOptions, BatchItem, DavCapabilities, DavCompliance, DavItem, DavItemCommon, Depth,
    LockInfo, LockScope, Prefer, Privilege, PropStat, SyncCapability, SyncLevel, WebDavError,
    parse_dav_header,
};
pub use xml::{
    MkCalendarProps, build_mkcalendar_body, build_propfind_allprop, build_propfind_propname,
    build_propfind_props, build_sync_collection_body, data_element_xml_with_limits, escape_xml,
};
