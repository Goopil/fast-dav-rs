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
pub use streaming::parse_error_body;
pub use types::{
    BatchItem, DavCapabilities, DavItem, DavItemCommon, Depth, Prefer, PropStat, SyncCapability,
    WebDavError, parse_dav_header,
};
pub use xml::{build_sync_collection_body, escape_xml};
