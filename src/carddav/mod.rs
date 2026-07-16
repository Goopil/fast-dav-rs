//! CardDAV client, streaming helpers, and types for addressbook discovery, queries, and sync.

pub mod client;
pub mod streaming;
pub mod types;

pub use client::{CardDavClient, map_address_objects, map_addressbook_list, map_sync_response};
pub use streaming::{
    parse_multistatus_bytes, parse_multistatus_bytes_visit, parse_multistatus_stream,
    parse_multistatus_stream_visit,
};
pub use types::{
    AddressBookInfo, AddressObject, BatchItem, DavItem, Depth, SyncItem, SyncResponse,
};
