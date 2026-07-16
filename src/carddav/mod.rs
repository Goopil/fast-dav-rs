//! CardDAV client, streaming helpers, and types for addressbook discovery, queries, and sync.

pub mod client;
pub mod streaming;
pub mod types;

pub use client::CardDavClient;
pub use types::{AddressBookInfo, AddressObject, BatchItem, Depth, SyncItem, SyncResponse};
