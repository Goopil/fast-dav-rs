//! CardDAV multistatus streaming — thin re-export of the unified parser in
//! [`crate::webdav::streaming`].

pub use crate::webdav::streaming::{
    ParseResult, STREAM_READ_IDLE_TIMEOUT, decode_text, parse_multistatus_bytes,
    parse_multistatus_bytes_visit, parse_multistatus_stream, parse_multistatus_stream_visit,
    parse_multistatus_stream_visit_with_timeout, parse_multistatus_stream_with_timeout,
};

/// CardDAV element names inside a `207 Multi-Status` body.
///
/// Retained for 0.9 source compatibility; the parser now dispatches on
/// [`crate::webdav::streaming::ElementName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[deprecated(
    since = "0.9.0",
    note = "use fast_dav_rs::webdav::streaming::ElementName; removed in 0.10"
)]
pub enum ElementName {
    Multistatus,
    Response,
    Propstat,
    Prop,
    Href,
    Status,
    Displayname,
    Getetag,
    Resourcetype,
    Collection,
    Addressbook,
    SupportedAddressData,
    AddressDataType,
    AddressData,
    AddressbookDescription,
    AddressbookColor,
    SyncToken,
    AddressbookHomeSet,
    CurrentUserPrincipal,
    Owner,
    Getcontenttype,
    Getlastmodified,
    Other,
}

/// Map a raw XML element name to the deprecated CardDAV [`ElementName`].
#[deprecated(
    since = "0.9.0",
    note = "use fast_dav_rs::webdav::streaming::element_from_bytes; removed in 0.10"
)]
#[allow(deprecated)]
pub fn element_from_bytes(raw: &[u8]) -> ElementName {
    let local = match raw.iter().position(|b| *b == b':') {
        Some(idx) => &raw[idx + 1..],
        None => raw,
    };

    if local.eq_ignore_ascii_case(b"multistatus") {
        ElementName::Multistatus
    } else if local.eq_ignore_ascii_case(b"response") {
        ElementName::Response
    } else if local.eq_ignore_ascii_case(b"propstat") {
        ElementName::Propstat
    } else if local.eq_ignore_ascii_case(b"prop") {
        ElementName::Prop
    } else if local.eq_ignore_ascii_case(b"href") {
        ElementName::Href
    } else if local.eq_ignore_ascii_case(b"status") {
        ElementName::Status
    } else if local.eq_ignore_ascii_case(b"displayname") {
        ElementName::Displayname
    } else if local.eq_ignore_ascii_case(b"getetag") {
        ElementName::Getetag
    } else if local.eq_ignore_ascii_case(b"resourcetype") {
        ElementName::Resourcetype
    } else if local.eq_ignore_ascii_case(b"collection") {
        ElementName::Collection
    } else if local.eq_ignore_ascii_case(b"addressbook") {
        ElementName::Addressbook
    } else if local.eq_ignore_ascii_case(b"supported-address-data") {
        ElementName::SupportedAddressData
    } else if local.eq_ignore_ascii_case(b"address-data-type") {
        ElementName::AddressDataType
    } else if local.eq_ignore_ascii_case(b"address-data") {
        ElementName::AddressData
    } else if local.eq_ignore_ascii_case(b"addressbook-description") {
        ElementName::AddressbookDescription
    } else if local.eq_ignore_ascii_case(b"addressbook-color") {
        ElementName::AddressbookColor
    } else if local.eq_ignore_ascii_case(b"sync-token") {
        ElementName::SyncToken
    } else if local.eq_ignore_ascii_case(b"addressbook-home-set") {
        ElementName::AddressbookHomeSet
    } else if local.eq_ignore_ascii_case(b"current-user-principal") {
        ElementName::CurrentUserPrincipal
    } else if local.eq_ignore_ascii_case(b"owner") {
        ElementName::Owner
    } else if local.eq_ignore_ascii_case(b"getcontenttype") {
        ElementName::Getcontenttype
    } else if local.eq_ignore_ascii_case(b"getlastmodified") {
        ElementName::Getlastmodified
    } else {
        ElementName::Other
    }
}
