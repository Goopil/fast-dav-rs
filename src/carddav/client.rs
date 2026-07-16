use anyhow::{Result, anyhow};
use bytes::Bytes;
use hyper::body::Incoming;
use hyper::{HeaderMap, Method, Response, StatusCode, Uri, header};
use std::sync::Arc;
use tokio::time::Duration;

use crate::carddav::streaming::parse_multistatus_bytes;
use crate::carddav::types::{
    AddressBookInfo, AddressObject, BatchItem, DavItem, Depth, SyncItem, SyncResponse,
};
use crate::common::compression::ContentEncoding;
use crate::webdav::client::WebDavClient;

pub use crate::webdav::client::RequestCompressionMode;

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

impl CardDavClient {
    /// Create a new client from a **base URL** (collection/home-set) and optional **Basic** credentials.
    ///
    /// The base may be `https://` **or** `http://` (both are supported by the connector).
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL for the CardDAV server (must be a valid URI)
    /// * `basic_user` - Optional username for Basic authentication
    /// * `basic_pass` - Optional password for Basic authentication
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The base URL is not a valid URI
    /// - Credentials are provided but cannot be encoded properly
    /// - TLS configuration fails
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::CardDavClient;
    /// use anyhow::Result;
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
        Ok(Self {
            webdav: WebDavClient::new(base_url, basic_user, basic_pass)?,
        })
    }

    /// Configure request compression for this client.
    ///
    /// # Arguments
    ///
    /// * `encoding` - The compression algorithm to use for outgoing requests
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::{CardDavClient, ContentEncoding};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut client = CardDavClient::new(
    ///     "https://card.example.com/dav/user01/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// client.set_request_compression(ContentEncoding::Gzip);
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_request_compression(&mut self, encoding: ContentEncoding) {
        self.webdav.set_request_compression(encoding);
    }

    /// Configure the request compression strategy.
    pub fn set_request_compression_mode(&mut self, mode: RequestCompressionMode) {
        self.webdav.set_request_compression_mode(mode);
    }

    /// Enable adaptive request compression (default behaviour).
    pub fn set_request_compression_auto(&mut self) {
        self.webdav.set_request_compression_auto();
    }

    /// Disable request compression entirely.
    pub fn disable_request_compression(&mut self) {
        self.webdav.disable_request_compression();
    }

    /// Get the current request compression strategy.
    pub fn request_compression_mode(&self) -> RequestCompressionMode {
        self.webdav.request_compression_mode()
    }

    /// Get the currently resolved request compression encoding.
    pub fn request_compression(&self) -> ContentEncoding {
        self.webdav.request_compression()
    }

    pub fn build_uri(&self, path: &str) -> Result<Uri> {
        self.webdav.build_uri(path)
    }

    // ----------- Aggregated send (Bytes) with automatic decompression -----------

    /// Generic **aggregated send** with automatic decompression (br/zstd/gzip).
    ///
    /// Returns a `Response<Bytes>` where the body is fully aggregated and already decompressed.
    ///
    /// # Example
    /// ```no_run
    /// # use fast_dav_rs::CardDavClient;
    /// # use hyper::{Method, HeaderMap};
    /// # use bytes::Bytes;
    /// #
    /// # async fn demo(cli: &CardDavClient) -> anyhow::Result<()> {
    /// let res = cli.send(Method::GET, "Calendars/Personal/", HeaderMap::new(), None, None).await?;
    /// assert!(res.status().is_success());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body_bytes: Option<Bytes>,
        per_req_timeout: Option<Duration>,
    ) -> Result<Response<Bytes>> {
        self.webdav
            .send(method, path, headers, body_bytes, per_req_timeout)
            .await
    }

    // ----------- Streaming send (for parsing on the fly) -----------

    /// Generic **streaming send**. Returns a `Response<Incoming>` (not aggregated).
    ///
    /// Use this when you want to parse the response on the fly, e.g. with
    /// [`crate::streaming::parse_multistatus_stream`].
    pub async fn send_stream(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body_bytes: Option<Bytes>,
        per_req_timeout: Option<Duration>,
    ) -> Result<Response<Incoming>> {
        self.webdav
            .send_stream(method, path, headers, body_bytes, per_req_timeout)
            .await
    }

    // ----------- HTTP/WebDAV Verbs -----------

    /// Send an `OPTIONS` request.
    ///
    /// Useful to discover server capabilities.
    pub async fn options(&self, path: &str) -> Result<Response<Bytes>> {
        self.webdav.options(path).await
    }
    /// Send a `HEAD` request.
    ///
    /// Often used to retrieve an `ETag` before a conditional update/delete.
    pub async fn head(&self, path: &str) -> Result<Response<Bytes>> {
        self.webdav.head(path).await
    }
    /// Send a `GET` request and return the fully aggregated (and decompressed) body.
    pub async fn get(&self, path: &str) -> Result<Response<Bytes>> {
        self.webdav.get(path).await
    }
    /// Send a `PUT` with a vCard body (`text/vcard`).
    ///
    /// Use [`put_if_match`] or [`put_if_none_match`] for safer conditional writes.
    pub async fn put(&self, path: &str, vcard_bytes: Bytes) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/vcard; charset=utf-8"),
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
    /// * `etag` - The ETag to match (should include quotes if required by server)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path cannot be resolved to a valid URI
    /// - The ETag value contains invalid characters for HTTP headers
    /// - Network or server errors occur
    pub async fn put_if_match(
        &self,
        path: &str,
        vcard_bytes: Bytes,
        etag: &str,
    ) -> Result<Response<Bytes>> {
        // Validate ETag doesn't contain forbidden characters
        if etag.is_empty() {
            return Err(anyhow!("ETag cannot be empty"));
        }

        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/vcard; charset=utf-8"),
        );
        h.insert(header::IF_MATCH, header::HeaderValue::from_str(etag)?);
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
            header::HeaderValue::from_static("text/vcard; charset=utf-8"),
        );
        h.insert(header::IF_NONE_MATCH, header::HeaderValue::from_static("*"));
        self.send(Method::PUT, path, h, Some(vcard_bytes), None)
            .await
    }
    /// Send a `DELETE` request.
    ///
    /// Prefer [`delete_if_match`] when you want to ensure you delete the expected version.
    pub async fn delete(&self, path: &str) -> Result<Response<Bytes>> {
        self.webdav.delete(path).await
    }
    /// Conditional `DELETE` guarded by `If-Match`.
    ///
    /// The delete only succeeds if the current resource ETag matches.
    pub async fn delete_if_match(&self, path: &str, etag: &str) -> Result<Response<Bytes>> {
        self.webdav.delete_if_match(path, etag).await
    }
    /// Send a WebDAV `COPY` from `src_path` to an absolute `Destination` URL.
    ///
    /// Set `overwrite = true` to allow replacing the destination if it exists.
    pub async fn copy(
        &self,
        src_path: &str,
        dest_absolute_url: &str,
        overwrite: bool,
    ) -> Result<Response<Bytes>> {
        self.webdav
            .copy(src_path, dest_absolute_url, overwrite)
            .await
    }
    /// Send a WebDAV `MOVE` from `src_path` to an absolute `Destination` URL.
    ///
    /// Set `overwrite = true` to allow replacing the destination if it exists.
    pub async fn r#move(
        &self,
        src_path: &str,
        dest_absolute_url: &str,
        overwrite: bool,
    ) -> Result<Response<Bytes>> {
        self.webdav
            .r#move(src_path, dest_absolute_url, overwrite)
            .await
    }

    /// Send a WebDAV `PROPFIND` with a custom XML body and `Depth` header.
    ///
    /// Typical properties to request include `DAV:displayname`, `DAV:getetag`,
    /// and `DAV:resourcetype`.
    pub async fn propfind(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Bytes>> {
        self.webdav.propfind(path, depth, xml_body).await
    }
    /// Send a WebDAV `PROPPATCH` with a custom XML body.
    pub async fn proppatch(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
        self.webdav.proppatch(path, xml_body).await
    }
    /// Send a CardDAV `REPORT` (e.g. `addressbook-query`) with a custom XML body and `Depth`.
    pub async fn report(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Bytes>> {
        self.webdav.report(path, depth, xml_body).await
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
    /// Send a WebDAV `MKCOL` to create a generic collection. Some servers accept an optional XML body.
    pub async fn mkcol(&self, path: &str, xml_body: Option<&str>) -> Result<Response<Bytes>> {
        self.webdav.mkcol(path, xml_body).await
    }

    /// Discover the current user's principal URL via `current-user-principal`.
    ///
    /// Returns `None` if the server omits the property.
    pub async fn discover_current_user_principal(&self) -> Result<Option<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:current-user-principal/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind("", Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "PROPFIND current-user-principal failed with {}",
                resp.status()
            ));
        }
        let body = resp.into_body();
        let mut principal = None;
        for item in parse_multistatus_bytes(&body)?.items {
            if let Some(found) = item
                .current_user_principal
                .into_iter()
                .find(|href| !href.is_empty())
            {
                principal = Some(found);
                break;
            }
        }
        Ok(principal)
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
            return Err(anyhow!(
                "PROPFIND addressbook-home-set failed with {}",
                resp.status()
            ));
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
            return Err(anyhow!(
                "PROPFIND addressbooks failed with {}",
                resp.status()
            ));
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
            return Err(anyhow!(
                "REPORT addressbook-query failed with {}",
                resp.status()
            ));
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
            return Err(anyhow!(
                "REPORT addressbook-multiget failed with {}",
                resp.status()
            ));
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

        let resp = self.report(addressbook_path, Depth::One, &body).await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "REPORT sync-collection failed with {}",
                resp.status()
            ));
        }
        let headers = resp.headers().clone();
        let body = resp.into_body();

        let parsed = parse_multistatus_bytes(&body)?;
        Ok(map_sync_response(&headers, parsed.items, parsed.sync_token))
    }

    // ----------- ETag helpers -----------

    /// Extract the `ETag` from a response header map, if present.
    pub fn etag_from_headers(headers: &HeaderMap) -> Option<String> {
        WebDavClient::etag_from_headers(headers)
    }

    // ----------- Batch (limited concurrency) -----------

    /// Run many `PROPFIND`s concurrently with a semaphore-bound concurrency limit.
    ///
    /// Returns results in the same order as inputs.
    pub async fn propfind_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        self.webdav
            .propfind_many(paths, depth, xml_body, max_concurrency)
            .await
    }

    /// Run many `REPORT`s concurrently with a semaphore-bound concurrency limit.
    ///
    /// Returns results in the same order as inputs.
    pub async fn report_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        self.webdav
            .report_many(paths, depth, xml_body, max_concurrency)
            .await
    }

    // ----------- Public streaming helpers -----------

    /// Check if the server supports WebDAV-Sync (RFC 6578).
    ///
    /// This method determines if the server supports the `sync-collection` REPORT
    /// method which enables efficient incremental synchronization.
    ///
    /// # Returns
    ///
    /// `Ok(true)` if WebDAV-Sync is supported, `Ok(false)` if not, or an error
    /// if the capability cannot be determined.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use fast_dav_rs::CardDavClient;
    /// # use anyhow::Result;
    /// #
    /// # async fn example() -> Result<()> {
    /// # let client = CardDavClient::new("https://example.com/", None, None)?;
    /// if client.supports_webdav_sync().await? {
    ///     println!("Server supports efficient incremental sync");
    /// } else {
    ///     println!("Server requires traditional polling");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn supports_webdav_sync(&self) -> Result<bool> {
        self.webdav.supports_webdav_sync().await
    }

    /// Streaming variant of `PROPFIND`, returning the non-aggregated body.
    ///
    /// Combine with [`crate::streaming::parse_multistatus_stream`].
    pub async fn propfind_stream(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Incoming>> {
        self.webdav.propfind_stream(path, depth, xml_body).await
    }
    /// Streaming variant of `REPORT`, returning the non-aggregated body.
    ///
    /// Combine with [`crate::streaming::parse_multistatus_stream`].
    pub async fn report_stream(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Incoming>> {
        self.webdav.report_stream(path, depth, xml_body).await
    }
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

pub(crate) fn build_addressbook_query_body(filter_xml: &str, include_data: bool) -> String {
    let mut prop = String::from("<D:prop><D:getetag/>");
    if include_data {
        prop.push_str("<C:address-data/>");
    }
    prop.push_str("</D:prop>");

    format!(
        r#"<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">{prop}{filter_xml}</C:addressbook-query>"#
    )
}

pub(crate) fn build_addressbook_query_filter_uid(uid: &str) -> String {
    build_addressbook_query_filter("UID", uid)
}

pub(crate) fn build_addressbook_query_filter_email(email: &str) -> String {
    build_addressbook_query_filter("EMAIL", email)
}

pub(crate) fn build_addressbook_query_filter_fn(formatted_name: &str) -> String {
    build_addressbook_query_filter("FN", formatted_name)
}

fn build_addressbook_query_filter(prop: &str, value: &str) -> String {
    let escaped = escape_xml(value);
    format!(
        "<C:filter>\
           <C:prop-filter name=\"{prop}\">\
             <C:text-match collation=\"i;unicode-casemap\" match-type=\"equals\">{escaped}</C:text-match>\
           </C:prop-filter>\
         </C:filter>"
    )
}

pub(crate) fn build_addressbook_multiget_body<I, S>(hrefs: I, include_data: bool) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut href_xml = String::new();
    let mut total = 0usize;
    for href in hrefs {
        let href = href.as_ref();
        if href.is_empty() {
            continue;
        }
        total += 1;
        href_xml.push_str("<D:href>");
        href_xml.push_str(&escape_xml(href));
        href_xml.push_str("</D:href>");
    }
    if total == 0 {
        return None;
    }

    let mut body = String::from(
        r#"<C:addressbook-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav"><D:prop><D:getetag/>"#,
    );
    if include_data {
        body.push_str("<C:address-data/>");
    }
    body.push_str("</D:prop>");
    body.push_str(&href_xml);
    body.push_str("</C:addressbook-multiget>");
    Some(body)
}

pub(crate) fn build_sync_collection_body(
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
    )
}

pub(crate) fn map_addressbook_list(mut items: Vec<DavItem>) -> Vec<AddressBookInfo> {
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

pub(crate) fn map_address_objects(items: Vec<DavItem>) -> Vec<AddressObject> {
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

pub(crate) fn map_sync_response(
    headers: &HeaderMap,
    items: Vec<DavItem>,
    top_level_sync_token: Option<String>,
) -> SyncResponse {
    // Prioritize top-level sync-token (RFC 6578), then headers, then per-item tokens
    let mut sync_token = top_level_sync_token.or_else(|| {
        headers
            .get("Sync-Token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });
    let mut out = Vec::new();

    for mut item in items {
        // Capture per-item sync token if we don't have a top-level one (fallback)
        if item.sync_token.is_some() && sync_token.is_none() {
            sync_token = item.sync_token.clone();
        }

        let is_collection = item.is_collection
            || (item.sync_token.is_some() && item.etag.is_none() && item.address_data.is_none());
        if is_collection {
            continue;
        }
        let status = item.status.clone();
        let is_deleted = status
            .as_deref()
            .map(|s| {
                s.split_whitespace()
                    .nth(1)
                    .and_then(|code| code.parse::<u16>().ok())
                    .map(|code| code == 404 || code == 410)
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        out.push(SyncItem {
            href: item.href,
            etag: item.etag,
            address_data: item.address_data.take(),
            status,
            is_deleted,
        });
    }

    SyncResponse {
        sync_token,
        items: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carddav::streaming::parse_multistatus_bytes;
    use hyper::HeaderMap;

    // --- XML builder tests ---

    #[test]
    fn test_build_addressbook_query_body() {
        let filter = build_addressbook_query_filter_uid("user-123");
        let body = build_addressbook_query_body(&filter, true);
        assert!(body.contains("<C:addressbook-query"));
        assert!(body.contains("<C:address-data/>"));
        assert!(body.contains("prop-filter name=\"UID\""));
        assert!(body.contains("user-123"));
    }

    #[test]
    fn test_build_addressbook_query_filters_escaping() {
        let email_filter = build_addressbook_query_filter_email("user@example.com");
        assert!(email_filter.contains("prop-filter name=\"EMAIL\""));
        assert!(email_filter.contains("user@example.com"));

        let fn_filter = build_addressbook_query_filter_fn("Ada Lovelace");
        assert!(fn_filter.contains("prop-filter name=\"FN\""));
        assert!(fn_filter.contains("Ada Lovelace"));
    }

    #[test]
    fn test_build_addressbook_multiget_and_escapes() {
        let body = build_addressbook_multiget_body(
            vec![
                "/addressbooks/user/contact1.vcf",
                "/addressbooks/user/contact&special.vcf",
            ],
            true,
        )
        .expect("Should create body");
        assert!(body.contains("<C:address-data/>"));
        assert!(body.contains("/addressbooks/user/contact1.vcf"));
        assert!(body.contains("contact&amp;special.vcf"));
    }

    #[test]
    fn test_build_addressbook_multiget_empty() {
        let body = build_addressbook_multiget_body(Vec::<String>::new(), true);
        assert!(body.is_none());
    }

    #[test]
    fn test_build_sync_collection_body() {
        let body =
            build_sync_collection_body(Some("http://example.com/sync-token-123"), Some(50), true);
        assert!(body.contains("<D:sync-token>http://example.com/sync-token-123</D:sync-token>"));
        assert!(body.contains("<C:address-data/>"));
        assert!(body.contains("<D:nresults>50</D:nresults>"));
    }

    // --- is_deleted tests (Fix 5 / v0.5.0) ---

    #[test]
    fn test_is_deleted_standard_404() {
        let mut item = DavItem::new();
        item.href = "/ab/contact.vcf".to_string();
        item.status = Some("HTTP/1.1 404 Not Found".to_string());

        let response = map_sync_response(&HeaderMap::new(), vec![item], None);
        assert!(response.items[0].is_deleted);
    }

    #[test]
    fn test_is_deleted_410_gone() {
        let mut item = DavItem::new();
        item.href = "/ab/contact.vcf".to_string();
        item.status = Some("HTTP/1.1 410 Gone".to_string());

        let response = map_sync_response(&HeaderMap::new(), vec![item], None);
        assert!(response.items[0].is_deleted);
    }

    #[test]
    fn test_is_not_deleted_200() {
        let mut item = DavItem::new();
        item.href = "/ab/contact.vcf".to_string();
        item.etag = Some("\"abc\"".to_string());
        item.status = Some("HTTP/1.1 200 OK".to_string());

        let response = map_sync_response(&HeaderMap::new(), vec![item], None);
        assert!(!response.items[0].is_deleted);
    }

    // --- From carddav_helpers.rs ---

    #[test]
    fn maps_carddav_multistatus_structures() {
        let books_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <C:addressbook-description>Friends</C:addressbook-description>
        <C:addressbook-color>#00ff00</C:addressbook-color>
        <D:getetag>"ab-etag"</D:getetag>
        <D:sync-token>http://sabre.io/ns/sync/42</D:sync-token>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
        <C:supported-address-data>
          <C:address-data-type content-type="text/vcard" version="4.0"/>
        </C:supported-address-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let books =
            map_addressbook_list(parse_multistatus_bytes(books_xml.as_bytes()).unwrap().items);
        assert_eq!(books.len(), 1);
        let book = &books[0];
        assert_eq!(book.href, "/dav/user01/AddressBooks/Personal/");
        assert_eq!(book.displayname.as_deref(), Some("Personal"));
        assert_eq!(
            book.sync_token.as_deref(),
            Some("http://sabre.io/ns/sync/42")
        );
        assert_eq!(
            book.supported_address_data,
            vec!["text/vcard;version=4.0".to_string()]
        );

        let multiget_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/contact.vcf</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"1234-1"</D:getetag>
        <C:address-data><![CDATA[BEGIN:VCARD
VERSION:4.0
UID:contact-1
END:VCARD]]></C:address-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let objects = map_address_objects(
            parse_multistatus_bytes(multiget_xml.as_bytes())
                .unwrap()
                .items,
        );
        assert_eq!(objects.len(), 1);
        let object = &objects[0];
        assert_eq!(object.href, "/dav/user01/AddressBooks/Personal/contact.vcf");
        assert!(
            object
                .address_data
                .as_ref()
                .unwrap()
                .contains("UID:contact-1")
        );

        let sync_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:sync-token>http://sabre.io/ns/sync/43</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/contact.vcf</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"1234-2"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/outdated.vcf</D:href>
    <D:propstat>
      <D:prop/>
      <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let mut headers = HeaderMap::new();
        headers.insert("Sync-Token", "http://sabre.io/ns/sync/43".parse().unwrap());
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);
        assert_eq!(
            sync.sync_token.as_deref(),
            Some("http://sabre.io/ns/sync/43")
        );
        assert_eq!(sync.items.len(), 2);
        assert!(sync.items.iter().any(|item| item.is_deleted));
    }

    #[test]
    fn test_top_level_sync_token_parsing() {
        // Test Apple CardDAV format where sync-token is at top level of multistatus
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/book/event1.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"abc123"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

        // Verify the sync token was captured at the top level
        assert_eq!(
            parsed.sync_token.as_deref(),
            Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
        );

        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        // The sync response should use the top-level token
        assert_eq!(
            sync.sync_token.as_deref(),
            Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
        );
        assert_eq!(sync.items.len(), 1);
        assert_eq!(sync.items[0].href, "/addressbooks/user/book/event1.vcf");
        assert_eq!(sync.items[0].etag.as_deref(), Some("\"abc123\""));
    }

    #[test]
    fn test_sync_token_no_changes() {
        // Test response with no changes - only sync token returned (Apple CardDAV behavior)
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<D:multistatus xmlns="DAV:">
    <sync-token>HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==</sync-token>
</D:multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

        // Verify the sync token was captured
        assert_eq!(
            parsed.sync_token.as_deref(),
            Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
        );

        // No items should be present
        assert_eq!(parsed.items.len(), 0);

        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        // The sync response should have the token but no items
        assert_eq!(
            sync.sync_token.as_deref(),
            Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
        );
        assert_eq!(sync.items.len(), 0);
    }

    #[test]
    fn test_sync_token_priority_top_level_over_header() {
        // Test that top-level sync token takes priority over header
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>top-level-token-123</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/event.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let mut headers = HeaderMap::new();
        headers.insert("Sync-Token", "header-token-456".parse().unwrap());
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        // Top-level token should win
        assert_eq!(sync.sync_token.as_deref(), Some("top-level-token-123"));
    }

    #[test]
    fn test_sync_token_priority_top_level_over_per_item() {
        // Test that top-level sync token takes priority over per-item tokens
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>top-level-token-abc</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/</D:href>
        <D:propstat>
            <D:prop>
                <D:sync-token>per-item-token-xyz</D:sync-token>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        // Top-level token should win over per-item token
        assert_eq!(sync.sync_token.as_deref(), Some("top-level-token-abc"));
    }

    #[test]
    fn test_sync_token_fallback_per_item() {
        // Test that per-item sync token is used when no top-level token exists
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/addressbooks/user/</D:href>
        <D:propstat>
            <D:prop>
                <D:sync-token>per-item-token-fallback</D:sync-token>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        // Per-item token should be used as fallback
        assert_eq!(sync.sync_token.as_deref(), Some("per-item-token-fallback"));
    }

    #[test]
    fn test_sync_token_fallback_header() {
        // Test that header sync token is used when no top-level or per-item tokens exist
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/addressbooks/user/event.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let mut headers = HeaderMap::new();
        headers.insert("Sync-Token", "header-token-fallback".parse().unwrap());
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        // Header token should be used as last fallback
        assert_eq!(sync.sync_token.as_deref(), Some("header-token-fallback"));
    }

    #[test]
    fn test_sync_with_deleted_items() {
        // Test sync response with deleted items (404 status)
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>sync-token-with-deletes</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/deleted1.vcf</D:href>
        <D:propstat>
            <D:prop/>
            <D:status>HTTP/1.1 404 Not Found</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/deleted2.vcf</D:href>
        <D:status>HTTP/1.1 410 Gone</D:status>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/updated.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"new-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        assert_eq!(sync.sync_token.as_deref(), Some("sync-token-with-deletes"));
        assert_eq!(sync.items.len(), 3);

        // Check that deleted items are marked correctly
        let deleted_count = sync.items.iter().filter(|item| item.is_deleted).count();
        assert_eq!(deleted_count, 2);

        // Check that updated item is not marked as deleted
        let updated = sync
            .items
            .iter()
            .find(|item| item.href.contains("updated.vcf"))
            .unwrap();
        assert!(!updated.is_deleted);
        assert_eq!(updated.etag.as_deref(), Some("\"new-etag\""));
    }

    #[test]
    fn test_sync_with_multiple_changes() {
        // Test sync response with multiple changed items
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>multi-change-token</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/book/</D:href>
        <D:propstat>
            <D:prop>
                <D:resourcetype>
                    <D:collection/>
                </D:resourcetype>
                <D:getetag>"cal-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/book/event1.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/book/event2.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag2"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        assert_eq!(sync.sync_token.as_deref(), Some("multi-change-token"));
        // Collection item should be filtered out, only event items remain
        assert_eq!(sync.items.len(), 2);
        assert!(sync.items.iter().all(|item| item.href.ends_with(".vcf")));
    }

    #[test]
    fn test_sync_token_without_namespace_prefix() {
        // Test Apple CardDAV format without namespace prefix (default namespace)
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<multistatus xmlns="DAV:">
    <sync-token>token-no-prefix</sync-token>
    <response>
        <href>/addressbooks/user/event.vcf</href>
        <propstat>
            <prop>
                <getetag>"etag-no-prefix"</getetag>
            </prop>
            <status>HTTP/1.1 200 OK</status>
        </propstat>
    </response>
</multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

        // Should parse correctly even without D: prefix
        assert_eq!(parsed.sync_token.as_deref(), Some("token-no-prefix"));
        assert_eq!(parsed.items.len(), 1);

        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);
        assert_eq!(sync.sync_token.as_deref(), Some("token-no-prefix"));
        assert_eq!(sync.items[0].etag.as_deref(), Some("\"etag-no-prefix\""));
    }

    #[test]
    fn test_sync_collection_filters_out_collections() {
        // Test that collection items are properly filtered out
        let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>filter-test-token</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/book/</D:href>
        <D:propstat>
            <D:prop>
                <D:resourcetype>
                    <D:collection/>
                </D:resourcetype>
                <D:getetag>"cal-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/book/event.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"event-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let headers = HeaderMap::new();
        let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

        // Both items should be parsed
        assert_eq!(parsed.items.len(), 2);

        let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

        // Collection should be filtered out, only event remains
        assert_eq!(sync.items.len(), 1);
        assert_eq!(sync.items[0].href, "/addressbooks/user/book/event.vcf");
    }

    #[test]
    fn test_apple_namespace_addressbook_color() {
        // Test that Apple's addressbook-color namespace is parsed correctly
        let books_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/addressbooks/user/personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <addressbook-color xmlns="http://apple.com/ns/ical/">#FF6B35FF</addressbook-color>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let books =
            map_addressbook_list(parse_multistatus_bytes(books_xml.as_bytes()).unwrap().items);
        assert_eq!(books.len(), 1);
        let book = &books[0];

        // Verify Apple namespace addressbook-color is parsed
        assert_eq!(book.color.as_deref(), Some("#FF6B35FF"));
        assert_eq!(book.displayname.as_deref(), Some("Personal"));
    }

    #[test]
    fn test_carddav_namespace_addressbook_color() {
        // Test standard CardDAV namespace addressbook-color
        let books_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/addressbooks/user/work/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Work</D:displayname>
        <C:addressbook-color>#0066CC</C:addressbook-color>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let books =
            map_addressbook_list(parse_multistatus_bytes(books_xml.as_bytes()).unwrap().items);
        assert_eq!(books.len(), 1);
        let book = &books[0];

        // Verify standard CardDAV namespace addressbook-color is parsed
        assert_eq!(book.color.as_deref(), Some("#0066CC"));
        assert_eq!(book.displayname.as_deref(), Some("Work"));
    }

    // --- From client_tests.rs (DavItem::new() tests) ---

    #[test]
    fn test_map_addressbook_list_filters_addressbooks() {
        let mut item = DavItem::new();
        item.href = "/addressbooks/user/personal/".to_string();
        item.displayname = Some("Personal".to_string());
        item.is_addressbook = true;

        let mut collection_item = DavItem::new();
        collection_item.href = "/addressbooks/user/collection/".to_string();
        collection_item.displayname = Some("Collection".to_string());
        collection_item.is_collection = true;

        let items = vec![item.clone(), collection_item.clone()];
        let books = map_addressbook_list(items);

        assert_eq!(books.len(), 1);
        assert_eq!(books[0].href, "/addressbooks/user/personal/");
        assert_eq!(books[0].displayname, Some("Personal".to_string()));
    }

    #[test]
    fn test_map_address_objects() {
        let mut item1 = DavItem::new();
        item1.href = "/addressbooks/user/contact1.vcf".to_string();
        item1.etag = Some("\"abc123\"".to_string());
        item1.address_data = Some("BEGIN:VCARD...END:VCARD".to_string());

        let mut item2 = DavItem::new();
        item2.href = "/addressbooks/user/contact2.vcf".to_string();
        item2.etag = Some("\"def456\"".to_string());
        item2.status = Some("HTTP/1.1 404 Not Found".to_string());

        let items = vec![item1.clone(), item2.clone()];
        let objects = map_address_objects(items);

        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].href, "/addressbooks/user/contact1.vcf");
        assert_eq!(objects[0].etag, Some("\"abc123\"".to_string()));
        assert_eq!(
            objects[0].address_data,
            Some("BEGIN:VCARD...END:VCARD".to_string())
        );
        assert_eq!(objects[1].href, "/addressbooks/user/contact2.vcf");
        assert_eq!(objects[1].etag, Some("\"def456\"".to_string()));
        assert_eq!(
            objects[1].status,
            Some("HTTP/1.1 404 Not Found".to_string())
        );
    }

    #[test]
    fn test_map_sync_response() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Sync-Token",
            "http://example.com/sync-token-456".parse().unwrap(),
        );

        let mut item1 = DavItem::new();
        item1.href = "/addressbooks/user/contact1.vcf".to_string();
        item1.etag = Some("\"abc123\"".to_string());
        item1.address_data = Some("BEGIN:VCARD...END:VCARD".to_string());

        let mut item2 = DavItem::new();
        item2.href = "/addressbooks/user/contact2.vcf".to_string();
        item2.status = Some("HTTP/1.1 404 Not Found".to_string());

        let mut collection_item = DavItem::new();
        collection_item.href = "/addressbooks/user/subbook/".to_string();
        collection_item.sync_token = Some("http://example.com/sync-token-789".to_string());
        collection_item.is_collection = true;

        let items = vec![item1, item2, collection_item];
        let response = map_sync_response(&headers, items, None);

        assert_eq!(
            response.sync_token,
            Some("http://example.com/sync-token-456".to_string())
        );
        assert_eq!(response.items.len(), 2); // Collection item should be filtered out

        // Check the first item (regular item with data)
        assert_eq!(response.items[0].href, "/addressbooks/user/contact1.vcf");
        assert_eq!(response.items[0].etag, Some("\"abc123\"".to_string()));
        assert!(!response.items[0].is_deleted); // Should not be deleted

        // Check second item (deleted item)
        assert_eq!(response.items[1].href, "/addressbooks/user/contact2.vcf");
        assert_eq!(
            response.items[1].status,
            Some("HTTP/1.1 404 Not Found".to_string())
        );
        assert!(response.items[1].is_deleted); // Should be marked as deleted
    }
}
