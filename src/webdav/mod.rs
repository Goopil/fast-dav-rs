pub mod builder;
pub mod client;
pub mod streaming;
pub mod types;
pub mod xml;

pub use builder::WebDavClientBuilder;
pub use client::{
    RequestCompressionMode, WebDavClient, etag_from_headers, normalize_etag, normalize_sync_token,
    preference_applied_from_headers,
};
pub use streaming::{parse_error_body, parse_lock_discovery_bytes};
pub use types::{
    BatchItem, DavCapabilities, DavItem, DavItemCommon, Depth, LockInfo, LockScope, Prefer,
    PropStat, SyncCapability, SyncLevel, WebDavError, parse_dav_header,
};
pub use xml::{build_sync_collection_body, escape_xml};
