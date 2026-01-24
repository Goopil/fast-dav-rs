pub mod client;
pub mod types;
pub mod xml;

pub use client::{RequestCompressionMode, WebDavClient};
pub use types::{BatchItem, DavItemCommon, Depth};
pub use xml::{build_sync_collection_body, escape_xml};
