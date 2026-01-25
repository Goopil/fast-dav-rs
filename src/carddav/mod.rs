pub mod client;
pub mod streaming;
pub mod types;

pub use client::{
    CardDavClient, build_addressbook_multiget_body, build_addressbook_query_body,
    build_addressbook_query_filter_email, build_addressbook_query_filter_fn,
    build_addressbook_query_filter_uid, build_sync_collection_body, map_address_objects,
    map_addressbook_list, map_sync_response,
};
pub use streaming::{
    parse_multistatus_bytes, parse_multistatus_bytes_visit, parse_multistatus_stream,
    parse_multistatus_stream_visit,
};
pub use types::{
    AddressBookInfo, AddressObject, BatchItem, DavItem, Depth, SyncItem, SyncResponse,
};
