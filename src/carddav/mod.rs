//! CardDAV client, streaming helpers, and types for addressbook discovery, queries, and sync.

pub mod builder;
pub mod client;
pub mod streaming;
pub mod types;

pub use crate::webdav::sync::{SyncDelta, SyncEntry, SyncSession, SyncSnapshot};
pub use builder::CardDavClientBuilder;
pub use client::{
    CardDavClient, VCARD_CONTENT_TYPE, build_addressbook_multiget_body,
    build_addressbook_query_body, build_addressbook_query_filter,
    build_addressbook_query_filter_email, build_addressbook_query_filter_fn,
    build_addressbook_query_filter_uid, build_sync_collection_body, map_address_object,
    map_address_objects, map_addressbook_list, map_sync_response,
};
// Deprecation propagated from the source items; see `streaming.rs`.
#[allow(deprecated)]
pub use streaming::{
    parse_multistatus_bytes, parse_multistatus_bytes_visit, parse_multistatus_stream,
    parse_multistatus_stream_visit, parse_multistatus_stream_visit_with_timeout,
    parse_multistatus_stream_with_timeout,
};
pub use types::{
    AddressBookInfo, AddressObject, BatchItem, CardDavFilter, Collation, DavItem, Depth, MatchType,
    ParamFilter, SyncItem, SyncResponse, TextMatch,
};
