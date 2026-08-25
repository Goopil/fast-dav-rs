pub mod builder;
pub mod client;
pub(crate) mod streaming;
pub mod types;
pub mod xml;

pub use builder::WebDavClientBuilder;
pub use client::{RequestCompressionMode, WebDavClient};
pub use streaming::parse_error_body;
pub use types::{
    BatchItem, DavCapabilities, DavItemCommon, Depth, PropStat, WebDavError, parse_dav_header,
};
pub use xml::{build_sync_collection_body, escape_xml};
