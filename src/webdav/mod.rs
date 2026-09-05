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
    RequestCompressionMode, WebDavClient, etag_from_headers, normalize_etag, normalize_sync_token,
    preference_applied_from_headers,
};
pub use discovery::{discover_caldav, discover_carddav};
pub use streaming::{parse_error_body, parse_lock_discovery_bytes};
pub use sync::{SyncDelta, SyncEntry, SyncSession, SyncSnapshot};
pub use types::{
    BatchItem, DavCapabilities, DavCompliance, DavItem, DavItemCommon, Depth, LockInfo, LockScope,
    Prefer, Privilege, PropStat, SyncCapability, SyncLevel, WebDavError, parse_dav_header,
};
pub use xml::{build_sync_collection_body, escape_xml};
