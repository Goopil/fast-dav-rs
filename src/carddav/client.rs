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
    build_addressbook_query_filter("UID", uid)
}

pub fn build_addressbook_query_filter_email(email: &str) -> String {
    build_addressbook_query_filter("EMAIL", email)
}

pub fn build_addressbook_query_filter_fn(formatted_name: &str) -> String {
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

pub fn build_addressbook_multiget_body<I, S>(hrefs: I, include_data: bool) -> Option<String>
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
            .map(|s| s.contains("404") || s.contains("410"))
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
