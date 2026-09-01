use bytes::Bytes;
use hyper::{HeaderMap, Method, Response, StatusCode, header};

use crate::Depth;
use crate::carddav::builder::CardDavClientBuilder;
use crate::carddav::streaming::parse_multistatus_bytes;
use crate::carddav::types::{
    AddressBookInfo, AddressObject, CardDavFilter, Collation, DavItem, MatchType, SyncItem,
    SyncResponse,
};
use crate::impl_dav_client_delegates;
use crate::webdav::client::{WebDavClient, if_match_header_value};
use crate::webdav::types::map_sync_rows;
use crate::{Error, Operation, Result};

pub use crate::webdav::client::RequestCompressionMode;

/// Content-Type for vCard PUT bodies, including the `version=4.0` parameter
/// required by RFC 6352 §6.2.1.
pub const VCARD_CONTENT_TYPE: &str = "text/vcard; charset=utf-8; version=4.0";

/// High-performance CardDAV client built on **hyper 1.x** + **rustls**.
///
/// Features:
/// - HTTP/2 multiplexing and connection pooling
/// - Automatic response decompression (br/zstd/gzip)
/// - Automatic request compression negotiation (br/zstd/gzip)
/// - Streaming-friendly APIs for large WebDAV responses
/// - Batch helpers with bounded concurrency
/// - ETag helpers for safe conditional writes/deletes
///
/// Cloning `CardDavClient` is cheap and reuses the same connection pool.

#[derive(Clone)]
pub struct CardDavClient {
    webdav: WebDavClient,
}

impl_dav_client_delegates!(CardDavClient);

impl CardDavClient {
    /// Create a new client from a **base URL** (collection/home-set) and optional **Basic** credentials.
    ///
    /// The base may be `https://` **or** `http://` (both are supported by the connector).
    ///
    /// # Security
    ///
    /// Basic credentials are sent as an `Authorization: Basic` header on **every**
    /// request. Base64 is an encoding, not encryption: over plain `http://` the
    /// credentials travel effectively in cleartext and can be read by anyone on the
    /// network path. Always use `https://` outside isolated test environments
    /// (e.g. a local Docker test server).
    ///
    /// # Errors
    ///
    /// Returns an error if the base URL is not a valid URI, if credentials
    /// cannot be encoded properly, or if TLS configuration fails.
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::CardDavClient;
    /// use fast_dav_rs::Result;
    ///
    /// # async fn example() -> Result<()> {
    /// let client = CardDavClient::new(
    ///     "https://card.example.com/dav/user01/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(base_url: &str, basic_user: Option<&str>, basic_pass: Option<&str>) -> Result<Self> {
        let mut builder = Self::builder(base_url);
        if let (Some(u), Some(p)) = (basic_user, basic_pass) {
            builder = builder.basic_auth(u, p);
        }
        builder.build()
    }

    /// Create a builder for configuring the client before construction.
    ///
    /// Only the base URL is required; every other option has a sensible
    /// default documented on [`CardDavClientBuilder`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::CardDavClient;
    /// use std::time::Duration;
    ///
    /// let client = CardDavClient::builder("https://card.example.com/dav/")
    ///     .basic_auth("user", "pass")
    ///     .timeout(Duration::from_secs(30))
    ///     .build()?;
    /// # Ok::<(), fast_dav_rs::Error>(())
    /// ```
    pub fn builder(base_url: impl Into<String>) -> CardDavClientBuilder {
        CardDavClientBuilder::new(base_url)
    }

    /// Send a `PUT` with a vCard body (`text/vcard`).
    ///
    /// Use [`put_if_match`] or [`put_if_none_match`] for safer conditional writes.
    pub async fn put(&self, path: &str, vcard_bytes: Bytes) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static(VCARD_CONTENT_TYPE),
        );
        self.send(Method::PUT, path, h, Some(vcard_bytes), None)
            .await
    }
    /// Conditional `PUT` guarded by `If-Match`.
    ///
    /// The write only succeeds if the current resource ETag matches.
    ///
    /// # Arguments
    ///
    /// * `path` - Resource path relative to the base URL
    /// * `vcard_bytes` - The vCard data to upload
    /// * `etag` - The ETag to match; quoted strong and weak ETags are accepted, and bare ETags
    ///   returned by some servers are quoted automatically
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path cannot be resolved to a valid URI
    /// - The ETag is empty or cannot form a valid HTTP entity-tag
    /// - Network or server errors occur
    pub async fn put_if_match(
        &self,
        path: &str,
        vcard_bytes: Bytes,
        etag: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static(VCARD_CONTENT_TYPE),
        );
        h.insert(header::IF_MATCH, if_match_header_value(etag)?);
        self.send(Method::PUT, path, h, Some(vcard_bytes), None)
            .await
    }
    /// Create-only `PUT` guarded by `If-None-Match: *`.
    ///
    /// Fails if the resource already exists.
    pub async fn put_if_none_match(
        &self,
        path: &str,
        vcard_bytes: Bytes,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static(VCARD_CONTENT_TYPE),
        );
        h.insert(header::IF_NONE_MATCH, header::HeaderValue::from_static("*"));
        self.send(Method::PUT, path, h, Some(vcard_bytes), None)
            .await
    }

    /// Send a CardDAV `MKADDRESSBOOK` to create an addressbook collection.
    pub async fn mkaddressbook(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        let resp = self
            .send(
                Method::from_bytes(b"MKADDRESSBOOK")?,
                path,
                h,
                Some(Bytes::from(xml_body.to_owned())),
                None,
            )
            .await?;

        if resp.status() == StatusCode::NOT_IMPLEMENTED
            || resp.status() == StatusCode::METHOD_NOT_ALLOWED
        {
            let fallback_body = build_mkcol_addressbook_body(xml_body);
            let mut h = HeaderMap::new();
            h.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/xml; charset=utf-8"),
            );
            return self
                .send(
                    Method::from_bytes(b"MKCOL")?,
                    path,
                    h,
                    Some(Bytes::from(fallback_body)),
                    None,
                )
                .await;
        }

        Ok(resp)
    }

    /// Discover the addressbook-home-set collection(s) for the provided principal path.
    pub async fn discover_addressbook_home_set(&self, principal_path: &str) -> Result<Vec<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <C:addressbook-home-set/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(principal_path, Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindAddressbookHomeSet,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        let mut homes = Vec::new();
        for mut item in parse_multistatus_bytes(&body)?.items {
            homes.append(&mut item.addressbook_home_set);
        }
        homes.sort();
        homes.dedup();
        Ok(homes)
    }

    /// List CardDAV collections under an addressbook home-set (`Depth: 1` PROPFIND).
    pub async fn list_addressbooks(&self, home_set_path: &str) -> Result<Vec<AddressBookInfo>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav" xmlns:A="http://apple.com/ns/ical/">
  <D:prop>
    <D:displayname/>
    <C:addressbook-description/>
    <C:addressbook-color/>
    <A:addressbook-color/>
    <C:supported-address-data/>
    <D:getetag/>
    <D:resourcetype/>
    <D:sync-token/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(home_set_path, Depth::One, body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindCollections,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        Ok(map_addressbook_list(parse_multistatus_bytes(&body)?.items))
    }

    /// Execute a CardDAV `addressbook-query` with a custom filter.
    pub async fn addressbook_query(
        &self,
        addressbook_path: &str,
        filter_xml: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let xml = build_addressbook_query_body(filter_xml, include_data);

        let resp = self.report(addressbook_path, Depth::One, &xml).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportAddressbookQuery,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        Ok(map_address_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Addressbook query helper: match a specific `UID`.
    pub async fn addressbook_query_uid(
        &self,
        addressbook_path: &str,
        uid: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let filter = build_addressbook_query_filter_uid(uid);
        self.addressbook_query(addressbook_path, &filter, include_data)
            .await
    }

    /// Addressbook query helper: match a specific `EMAIL`.
    pub async fn addressbook_query_email(
        &self,
        addressbook_path: &str,
        email: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let filter = build_addressbook_query_filter_email(email);
        self.addressbook_query(addressbook_path, &filter, include_data)
            .await
    }

    /// Addressbook query helper: match a specific `FN` (formatted name).
    pub async fn addressbook_query_fn(
        &self,
        addressbook_path: &str,
        formatted_name: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let filter = build_addressbook_query_filter_fn(formatted_name);
        self.addressbook_query(addressbook_path, &filter, include_data)
            .await
    }

    /// Fetch specific address objects via `addressbook-multiget`.
    pub async fn addressbook_multiget<I, S>(
        &self,
        addressbook_path: &str,
        hrefs: I,
        include_data: bool,
    ) -> Result<Vec<AddressObject>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let Some(body) = build_addressbook_multiget_body(hrefs, include_data) else {
            return Ok(Vec::new());
        };

        let resp = self.report(addressbook_path, Depth::One, &body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportAddressbookMultiget,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        Ok(map_address_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Incrementally synchronise an addressbook collection using `sync-collection`.
    pub async fn sync_collection(
        &self,
        addressbook_path: &str,
        sync_token: Option<&str>,
        limit: Option<u32>,
        include_data: bool,
    ) -> Result<SyncResponse> {
        let body = build_sync_collection_body(sync_token, limit, include_data);

        let resp = self.report(addressbook_path, Depth::Zero, &body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportSyncCollection,
                status: resp.status(),
            });
        }
        let headers = resp.headers().clone();
        let body = resp.into_body();

        let parsed = parse_multistatus_bytes(&body)?;
        Ok(map_sync_response(&headers, parsed.items, parsed.sync_token))
    }

    // ----------- ETag helpers -----------

    // ----------- Batch (limited concurrency) -----------

    // ----------- Public streaming helpers -----------
}

pub fn escape_xml(input: &str) -> String {
    crate::webdav::xml::escape_xml(input)
}

fn build_mkcol_addressbook_body(xml_body: &str) -> String {
    let prop_inner = extract_prop_inner(xml_body);
    let has_resourcetype = prop_inner
        .as_deref()
        .map(|inner| inner.to_ascii_lowercase().contains("resourcetype"))
        .unwrap_or(false);

    let mut prop = String::new();
    if !has_resourcetype {
        prop.push_str("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>");
    }
    if let Some(inner) = prop_inner {
        let trimmed = inner.trim();
        if !trimmed.is_empty() {
            prop.push_str(trimmed);
        }
    }
    if prop.is_empty() {
        prop.push_str("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>");
    }

    format!(
        r#"<D:mkcol xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav"><D:set><D:prop>{prop}</D:prop></D:set></D:mkcol>"#
    )
}

fn extract_prop_inner(xml_body: &str) -> Option<String> {
    let mut start = None;
    for open in ["<D:prop>", "<d:prop>"] {
        if let Some(idx) = xml_body.find(open) {
            start = Some(idx + open.len());
            break;
        }
    }
    let start = start?;
    let remaining = &xml_body[start..];

    let mut end = None;
    for close in ["</D:prop>", "</d:prop>"] {
        if let Some(idx) = remaining.find(close) {
            end = Some(idx);
            break;
        }
    }
    let end = end?;
    Some(remaining[..end].to_string())
}

pub fn build_addressbook_query_body(filter_xml: &str, include_data: bool) -> String {
    let mut prop = String::from("<D:prop><D:getetag/>");
    if include_data {
        prop.push_str("<C:address-data/>");
    }
    prop.push_str("</D:prop>");

    format!(
        r#"<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">{prop}{filter_xml}</C:addressbook-query>"#
    )
}

pub fn build_addressbook_query_filter_uid(uid: &str) -> String {
    build_addressbook_query_filter(
        "UID",
        uid,
        Collation::default(),
        MatchType::default(),
        false,
    )
}

pub fn build_addressbook_query_filter_email(email: &str) -> String {
    build_addressbook_query_filter(
        "EMAIL",
        email,
        Collation::default(),
        MatchType::default(),
        false,
    )
}

pub fn build_addressbook_query_filter_fn(formatted_name: &str) -> String {
    build_addressbook_query_filter(
        "FN",
        formatted_name,
        Collation::default(),
        MatchType::default(),
        false,
    )
}

pub fn build_addressbook_query_filter(
    prop: &str,
    value: &str,
    collation: Collation,
    match_type: MatchType,
    negate: bool,
) -> String {
    let filter = CardDavFilter {
        prop: prop.to_string(),
        value: value.to_string(),
        collation,
        match_type,
        negate,
        param_filters: vec![],
        is_not_defined: false,
    };
    filter.to_filter_xml()
}

pub fn build_addressbook_multiget_body<I, S>(hrefs: I, include_data: bool) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    crate::webdav::xml::build_multiget_body(
        hrefs,
        include_data,
        "urn:ietf:params:xml:ns:carddav",
        "addressbook-multiget",
        "address-data",
        None,
    )
}

pub fn build_sync_collection_body(
    sync_token: Option<&str>,
    limit: Option<u32>,
    include_data: bool,
) -> String {
    crate::webdav::xml::build_sync_collection_body(
        sync_token,
        limit,
        include_data,
        "urn:ietf:params:xml:ns:carddav",
        "address-data",
        None,
    )
}

pub fn map_addressbook_list(mut items: Vec<DavItem>) -> Vec<AddressBookInfo> {
    let mut addressbooks = Vec::new();
    for mut item in items.drain(..) {
        if item.is_addressbook {
            let description = item
                .addressbook_description
                .take()
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty());
            addressbooks.push(AddressBookInfo {
                href: item.href,
                displayname: item.displayname,
                description,
                color: item.addressbook_color,
                etag: item.etag,
                sync_token: item.sync_token,
                supported_address_data: item.supported_address_data,
            });
        }
    }
    addressbooks.sort_by(|a, b| a.href.cmp(&b.href));
    addressbooks
}

pub fn map_address_objects(items: Vec<DavItem>) -> Vec<AddressObject> {
    let mut out = Vec::with_capacity(items.len());
    for mut item in items {
        out.push(AddressObject {
            href: item.href,
            etag: item.etag,
            address_data: item.address_data.take(),
            status: item.status,
        });
    }
    out
}

pub fn map_sync_response(
    headers: &HeaderMap,
    items: Vec<DavItem>,
    top_level_sync_token: Option<String>,
) -> SyncResponse {
    let (sync_token, rows) = map_sync_rows(headers, items, top_level_sync_token, |item| {
        item.address_data.take()
    });
    SyncResponse {
        sync_token,
        items: rows
            .into_iter()
            .map(|r| SyncItem {
                href: r.href,
                etag: r.etag,
                address_data: r.data,
                status: r.status,
                is_deleted: r.is_deleted,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_prop_inner_d_uppercase() {
        let xml = "<outer><D:prop>inner content</D:prop></outer>";
        let result = extract_prop_inner(xml);
        assert_eq!(result.as_deref(), Some("inner content"));
    }

    #[test]
    fn extract_prop_inner_d_lowercase() {
        let xml = "<outer><d:prop>inner content</d:prop></outer>";
        let result = extract_prop_inner(xml);
        assert_eq!(result.as_deref(), Some("inner content"));
    }

    #[test]
    fn extract_prop_inner_absent_returns_none() {
        let xml = "<outer><D:something>content</D:something></outer>";
        let result = extract_prop_inner(xml);
        assert!(result.is_none());
    }

    #[test]
    fn extract_prop_inner_no_closing_tag_returns_none() {
        let xml = "<D:prop>content";
        let result = extract_prop_inner(xml);
        assert!(result.is_none());
    }

    #[test]
    fn extract_prop_inner_prefers_d_uppercase_first() {
        let xml = "<root><D:prop>upper</D:prop><d:prop>lower</d:prop></root>";
        let result = extract_prop_inner(xml);
        assert_eq!(result.as_deref(), Some("upper"));
    }

    #[test]
    fn extract_prop_inner_empty_inner() {
        let xml = "<root><D:prop></D:prop></root>";
        let result = extract_prop_inner(xml);
        assert_eq!(result.as_deref(), Some(""));
    }

    #[test]
    fn build_mkcol_addressbook_body_with_resourcetype_no_duplicate() {
        let xml = r#"<D:mkcol xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
            <D:set><D:prop>
                <D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>
                <D:displayname>My Book</D:displayname>
            </D:prop></D:set>
        </D:mkcol>"#;
        let body = build_mkcol_addressbook_body(xml);
        let count = body
            .matches("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>")
            .count();
        assert_eq!(count, 1);
        assert!(body.contains("<D:displayname>My Book</D:displayname>"));
    }

    #[test]
    fn build_mkcol_addressbook_body_with_other_props_adds_resourcetype() {
        let xml = r#"<D:mkcol xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
            <D:set><D:prop>
                <D:displayname>My Book</D:displayname>
            </D:prop></D:set>
        </D:mkcol>"#;
        let body = build_mkcol_addressbook_body(xml);
        assert!(body.contains("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>"));
        assert!(body.contains("<D:displayname>My Book</D:displayname>"));
    }

    #[test]
    fn build_mkcol_addressbook_body_empty_prop_inner_adds_resourcetype() {
        let xml = r#"<D:mkcol xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
            <D:set><D:prop></D:prop></D:set>
        </D:mkcol>"#;
        let body = build_mkcol_addressbook_body(xml);
        assert!(body.contains("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>"));
    }

    #[test]
    fn build_mkcol_addressbook_body_no_prop_adds_resourcetype() {
        let xml = r#"<D:mkcol xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
            <D:set><D:other>value</D:other></D:set>
        </D:mkcol>"#;
        let body = build_mkcol_addressbook_body(xml);
        assert!(body.contains("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>"));
    }

    #[test]
    fn build_mkcol_addressbook_body_contains_mkcol_root() {
        let xml = "";
        let body = build_mkcol_addressbook_body(xml);
        assert!(
            body.contains("<D:mkcol xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:carddav\">")
        );
        assert!(body.contains("<D:set><D:prop>"));
        assert!(body.contains("</D:prop></D:set></D:mkcol>"));
    }

    #[test]
    fn build_mkcol_addressbook_body_whitespace_only_inner_uses_resourcetype() {
        let xml = r#"<D:mkcol xmlns:D="DAV:"><D:set><D:prop>   </D:prop></D:set></D:mkcol>"#;
        let body = build_mkcol_addressbook_body(xml);
        assert!(body.contains("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>"));
    }
}
