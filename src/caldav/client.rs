use bytes::Bytes;
use hyper::body::Incoming;
use hyper::{HeaderMap, Method, Response, Uri, header};
use std::sync::Arc;
use tokio::time::Duration;

use crate::caldav::builder::CalDavClientBuilder;
use crate::caldav::streaming::parse_multistatus_bytes;
use crate::caldav::types::{
    BatchItem, CalendarInfo, CalendarObject, CalendarQueryFilter, DavItem, Depth, SyncItem,
    SyncResponse,
};
use crate::common::compression::ContentEncoding;
use crate::webdav::client::{WebDavClient, if_match_header_value, normalize_sync_token};
use crate::webdav::types::http_status_code;
use crate::webdav::xml::{validate_component_name, validate_utc_datetime};
use crate::{Error, Operation, Result};

pub use crate::webdav::client::RequestCompressionMode;

/// High-performance CalDAV client built on **hyper 1.x** + **rustls**.
///
/// Features:
/// - HTTP/2 multiplexing and connection pooling
/// - Automatic response decompression (br/zstd/gzip)
/// - Automatic request compression negotiation (br/zstd/gzip)
/// - Streaming-friendly APIs for large WebDAV responses
/// - Batch helpers with bounded concurrency
/// - ETag helpers for safe conditional writes/deletes
///
/// Cloning `CalDavClient` is cheap and reuses the same connection pool.

#[derive(Clone)]
pub struct CalDavClient {
    webdav: WebDavClient,
}

impl CalDavClient {
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
    /// # Arguments
    ///
    /// * `base_url` - The base URL for the CalDAV server (must be a valid URI)
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
    /// use fast_dav_rs::CalDavClient;
    /// use fast_dav_rs::Result;
    ///
    /// # async fn example() -> Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/user01/",
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
    /// default documented on [`CalDavClientBuilder`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::CalDavClient;
    /// use std::time::Duration;
    ///
    /// let client = CalDavClient::builder("https://cal.example.com/dav/")
    ///     .basic_auth("user", "pass")
    ///     .timeout(Duration::from_secs(30))
    ///     .build()?;
    /// # Ok::<(), fast_dav_rs::Error>(())
    /// ```
    pub fn builder(base_url: impl Into<String>) -> CalDavClientBuilder {
        CalDavClientBuilder::new(base_url)
    }

    /// Wrap a [`WebDavClient`] into a [`CalDavClient`].
    pub(crate) fn from_webdav(webdav: WebDavClient) -> Self {
        Self { webdav }
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
    /// use fast_dav_rs::{CalDavClient, ContentEncoding};
    ///
    /// # fn example() -> fast_dav_rs::Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/user01/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// client.set_request_compression(ContentEncoding::Gzip);
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_request_compression(&self, encoding: ContentEncoding) {
        self.webdav.set_request_compression(encoding);
    }

    /// Configure the request compression strategy.
    pub fn set_request_compression_mode(&self, mode: RequestCompressionMode) {
        self.webdav.set_request_compression_mode(mode);
    }

    /// Enable adaptive request compression (default behaviour).
    pub fn set_request_compression_auto(&self) {
        self.webdav.set_request_compression_auto();
    }

    /// Disable request compression entirely.
    pub fn disable_request_compression(&self) {
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
    /// # use fast_dav_rs::CalDavClient;
    /// # use hyper::{Method, HeaderMap};
    /// # use bytes::Bytes;
    /// #
    /// # async fn demo(cli: &CalDavClient) -> fast_dav_rs::Result<()> {
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
    /// [`crate::caldav::streaming::parse_multistatus_stream`].
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
    /// Send a `PUT` with an iCalendar body (`text/calendar`).
    ///
    /// Use [`put_if_match`] or [`put_if_none_match`] for safer conditional writes.
    pub async fn put(&self, path: &str, ical_bytes: Bytes) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/calendar; charset=utf-8"),
        );
        self.send(Method::PUT, path, h, Some(ical_bytes), None)
            .await
    }
    /// Conditional `PUT` guarded by `If-Match`.
    ///
    /// The write only succeeds if the current resource ETag matches.
    ///
    /// # Arguments
    ///
    /// * `path` - Resource path relative to the base URL
    /// * `ical_bytes` - The iCalendar data to upload
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
        ical_bytes: Bytes,
        etag: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/calendar; charset=utf-8"),
        );
        h.insert(header::IF_MATCH, if_match_header_value(etag)?);
        self.send(Method::PUT, path, h, Some(ical_bytes), None)
            .await
    }
    /// Create-only `PUT` guarded by `If-None-Match: *`.
    ///
    /// Fails if the resource already exists.
    pub async fn put_if_none_match(
        &self,
        path: &str,
        ical_bytes: Bytes,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/calendar; charset=utf-8"),
        );
        h.insert(header::IF_NONE_MATCH, header::HeaderValue::from_static("*"));
        self.send(Method::PUT, path, h, Some(ical_bytes), None)
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
    /// Send a CalDAV `REPORT` (e.g. `calendar-query`) with a custom XML body and `Depth`.
    ///
    /// This is the primary way to query events with time ranges.
    pub async fn report(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Bytes>> {
        self.webdav.report(path, depth, xml_body).await
    }
    /// Send a CalDAV `MKCALENDAR` to create a calendar collection.
    pub async fn mkcalendar(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send(
            Method::from_bytes(b"MKCALENDAR")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }
    /// Send a WebDAV `MKCOL` to create a generic collection. Some servers accept an optional XML body.
    pub async fn mkcol(&self, path: &str, xml_body: Option<&str>) -> Result<Response<Bytes>> {
        self.webdav.mkcol(path, xml_body).await
    }

    /// Discover the current user's principal URL via `current-user-principal`.
    ///
    /// Returns `None` if the server omits the property.
    pub async fn discover_current_user_principal(&self) -> Result<Option<String>> {
        self.webdav.discover_current_user_principal().await
    }

    /// Discover the calendar-home-set collection(s) for the provided principal path.
    pub async fn discover_calendar_home_set(&self, principal_path: &str) -> Result<Vec<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <C:calendar-home-set/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(principal_path, Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindCalendarHomeSet,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        let mut homes = Vec::new();
        for mut item in parse_multistatus_bytes(&body)?.items {
            homes.append(&mut item.calendar_home_set);
        }
        homes.sort();
        homes.dedup();
        Ok(homes)
    }

    /// List CalDAV collections under a calendar home-set (`Depth: 1` PROPFIND).
    pub async fn list_calendars(&self, home_set_path: &str) -> Result<Vec<CalendarInfo>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
  <D:prop>
    <D:displayname/>
    <C:calendar-description/>
    <C:calendar-timezone/>
    <C:calendar-color/>
    <A:calendar-color/>
    <C:supported-calendar-component-set/>
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
        Ok(map_calendar_list(parse_multistatus_bytes(&body)?.items))
    }

    /// Execute a CalDAV `calendar-query` with an optional time-range filter.
    ///
    /// `component` should be `VEVENT`, `VTODO`, … while `start`/`end` are ISO-8601
    /// timestamps in the format required by CalDAV (e.g. `20240101T000000Z`).
    ///
    /// Inputs are validated before any request is sent, and additionally
    /// XML-escaped when the request body is built (defense in depth).
    ///
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if:
    /// - `component` is empty or contains characters outside ASCII
    ///   alphanumerics and `-` (e.g. `VEVENT` and `X-CUSTOM` are valid)
    /// - `start` or `end` is provided but is not a structurally valid
    ///   iCalendar UTC date-time (`YYYYMMDDTHHMMSSZ`)
    ///
    /// Also returns an error if the REPORT request fails or the server
    /// responds with a non-success status.
    pub async fn calendar_query_timerange(
        &self,
        calendar_path: &str,
        component: &str,
        start: Option<&str>,
        end: Option<&str>,
        include_data: bool,
    ) -> Result<Vec<CalendarObject>> {
        validate_component_name(component, "invalid calendar-query component")?;
        if let Some(s) = start {
            validate_utc_datetime(s, "invalid calendar-query start")?;
        }
        if let Some(e) = end {
            validate_utc_datetime(e, "invalid calendar-query end")?;
        }

        let xml = build_calendar_query_body(component, start, end, include_data);

        let resp = self.report(calendar_path, Depth::One, &xml).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportCalendarQuery,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        Ok(map_calendar_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Execute a CalDAV `calendar-query` with a [`CalendarQueryFilter`].
    ///
    /// This is the full-featured query API supporting property-level filtering
    /// (`prop-filter`, `text-match`, `param-filter`, `is-not-defined`) per
    /// RFC 4791 §8.1-8.5.
    ///
    /// The component name in the filter is validated before any request is
    /// sent. All values are XML-escaped when the request body is built.
    ///
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if:
    /// - The component name is empty or contains characters outside ASCII
    ///   alphanumerics and `-`
    ///
    /// Also returns an error if the REPORT request fails or the server
    /// responds with a non-success status.
    pub async fn calendar_query(
        &self,
        calendar_path: &str,
        filter: &CalendarQueryFilter,
        include_data: bool,
    ) -> Result<Vec<CalendarObject>> {
        validate_component_name(&filter.component, "invalid calendar-query component")?;
        if let Some(tr) = &filter.time_range {
            validate_utc_datetime(&tr.start, "invalid calendar-query time-range start")?;
            if let Some(end) = &tr.end {
                validate_utc_datetime(end, "invalid calendar-query time-range end")?;
            }
        }
        for pf in &filter.prop_filters {
            if let Some(tr) = &pf.time_range {
                validate_utc_datetime(
                    &tr.start,
                    "invalid calendar-query prop-filter time-range start",
                )?;
                if let Some(end) = &tr.end {
                    validate_utc_datetime(
                        end,
                        "invalid calendar-query prop-filter time-range end",
                    )?;
                }
            }
        }

        let xml = filter.to_query_body(include_data);

        let resp = self.report(calendar_path, Depth::One, &xml).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportCalendarQuery,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        Ok(map_calendar_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Fetch specific calendar objects via `calendar-multiget`.
    pub async fn calendar_multiget<I, S>(
        &self,
        calendar_path: &str,
        hrefs: I,
        include_data: bool,
    ) -> Result<Vec<CalendarObject>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let Some(body) = build_calendar_multiget_body(hrefs, include_data) else {
            return Ok(Vec::new());
        };

        let resp = self.report(calendar_path, Depth::One, &body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportCalendarMultiget,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        Ok(map_calendar_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Incrementally synchronise a calendar collection using `sync-collection`.
    pub async fn sync_collection(
        &self,
        calendar_path: &str,
        sync_token: Option<&str>,
        limit: Option<u32>,
        include_data: bool,
    ) -> Result<SyncResponse> {
        let body = build_sync_collection_body(sync_token, limit, include_data);

        let resp = self.report(calendar_path, Depth::One, &body).await?;
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

    /// Extract the `ETag` from a response header map, if present.
    pub fn etag_from_headers(headers: &HeaderMap) -> Option<String> {
        WebDavClient::etag_from_headers(headers)
    }

    /// Normalize an ETag by stripping surrounding double quotes.
    ///
    /// `"abc"` becomes `abc`, `W/"abc"` becomes `W/abc`; bare values are
    /// returned unchanged.  Use the normalized value with [`Self::put_if_match`]
    /// / [`Self::delete_if_match`], which re-add quoting on the wire.
    pub fn normalize_etag(etag: &str) -> String {
        WebDavClient::normalize_etag(etag)
    }

    /// Normalize a sync token by trimming whitespace and stripping
    /// surrounding double quotes.
    pub fn normalize_sync_token(token: &str) -> String {
        WebDavClient::normalize_sync_token(token)
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
    /// # use fast_dav_rs::CalDavClient;
    /// # use fast_dav_rs::Result;
    /// #
    /// # async fn example() -> Result<()> {
    /// # let client = CalDavClient::new("https://example.com/", None, None)?;
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
    /// Combine with [`crate::caldav::streaming::parse_multistatus_stream`].
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
    /// Combine with [`crate::caldav::streaming::parse_multistatus_stream`].
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

pub fn build_calendar_query_body(
    component: &str,
    start: Option<&str>,
    end: Option<&str>,
    include_data: bool,
) -> String {
    let mut prop = String::from("<D:prop><D:getetag/>");
    if include_data {
        prop.push_str("<C:calendar-data/>");
    }
    prop.push_str("</D:prop>");

    let mut filter = format!(
        "<C:filter>\
           <C:comp-filter name=\"VCALENDAR\">\
             <C:comp-filter name=\"{}\">",
        escape_xml(component)
    );
    if start.is_some() || end.is_some() {
        filter.push_str("<C:time-range");
        if let Some(s) = start {
            filter.push_str(&format!(" start=\"{}\"", escape_xml(s)));
        }
        if let Some(e) = end {
            filter.push_str(&format!(" end=\"{}\"", escape_xml(e)));
        }
        filter.push_str("/>");
    }
    filter.push_str("</C:comp-filter></C:comp-filter></C:filter>");

    format!(
        r#"<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">{prop}{filter}</C:calendar-query>"#
    )
}

pub fn build_calendar_multiget_body<I, S>(hrefs: I, include_data: bool) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    crate::webdav::xml::build_multiget_body(
        hrefs,
        include_data,
        "urn:ietf:params:xml:ns:caldav",
        "calendar-multiget",
        "calendar-data",
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
        "urn:ietf:params:xml:ns:caldav",
        "calendar-data",
    )
}

pub fn map_calendar_list(mut items: Vec<DavItem>) -> Vec<CalendarInfo> {
    let mut calendars = Vec::new();
    for mut item in items.drain(..) {
        if item.is_calendar {
            let timezone = item
                .calendar_timezone
                .take()
                .map(|tz| tz.trim().to_string())
                .filter(|tz| !tz.is_empty());
            let description = item
                .calendar_description
                .take()
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty());
            calendars.push(CalendarInfo {
                href: item.href,
                displayname: item.displayname,
                description,
                timezone,
                color: item.calendar_color,
                etag: item.etag,
                sync_token: item.sync_token,
                supported_components: item.supported_components,
            });
        }
    }
    calendars.sort_by(|a, b| a.href.cmp(&b.href));
    calendars
}

pub fn map_calendar_objects(items: Vec<DavItem>) -> Vec<CalendarObject> {
    let mut out = Vec::with_capacity(items.len());
    for mut item in items {
        out.push(CalendarObject {
            href: item.href,
            etag: item.etag,
            calendar_data: item.calendar_data.take(),
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
    let mut sync_token = top_level_sync_token.or_else(|| {
        headers
            .get("Sync-Token")
            .and_then(|v| v.to_str().ok())
            .map(normalize_sync_token)
    });
    let mut out = Vec::new();

    for mut item in items {
        // Already normalized by the streaming parser's normalize_sync_token.
        if item.sync_token.is_some() && sync_token.is_none() {
            sync_token = item.sync_token.clone();
        }

        let is_collection = item.is_collection
            || (item.sync_token.is_some() && item.etag.is_none() && item.calendar_data.is_none());
        if is_collection {
            continue;
        }
        let status = item.status.clone();
        let code = status.as_deref().and_then(http_status_code);
        let is_deleted = matches!(code, Some(404) | Some(410));

        out.push(SyncItem {
            href: item.href,
            etag: item.etag,
            calendar_data: item.calendar_data.take(),
            status,
            is_deleted,
        });
    }

    SyncResponse {
        sync_token,
        items: out,
    }
}
