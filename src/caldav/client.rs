use bytes::Bytes;
use hyper::{HeaderMap, Method, Response, header};

use crate::Depth;
use crate::caldav::builder::CalDavClientBuilder;
use crate::caldav::streaming::parse_multistatus_bytes;
use crate::caldav::types::{
    CalendarInfo, CalendarObject, CalendarQueryFilter, DavItem, SyncItem, SyncResponse,
};
use crate::impl_dav_client_delegates;
use crate::webdav::client::{WebDavClient, if_match_header_value};
use crate::webdav::types::map_sync_rows;
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

impl_dav_client_delegates!(CalDavClient);

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
    /// # Errors
    ///
    /// Returns an error if the base URL is not a valid URI, if credentials
    /// cannot be encoded properly, or if TLS configuration fails.
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
    /// # Errors
    ///
    /// Returns an error if the path cannot be resolved to a valid URI, the
    /// ETag is empty or malformed, or a network/server error occurs.
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
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if `component` is empty or
    /// contains characters outside ASCII alphanumerics and `-`, or if `start`
    /// /`end` is provided but is not a valid iCalendar UTC date-time
    /// (`YYYYMMDDTHHMMSSZ`). Also returns an error if the REPORT request fails
    /// or the server responds with a non-success status.
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
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if the component name is
    /// empty or contains characters outside ASCII alphanumerics and `-`, or if
    /// any time-range value is not a valid iCalendar UTC date-time. Also
    /// returns an error if the REPORT request fails or the server responds
    /// with a non-success status.
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
    let (sync_token, rows) = map_sync_rows(headers, items, top_level_sync_token, |item| {
        item.calendar_data.take()
    });
    SyncResponse {
        sync_token,
        items: rows
            .into_iter()
            .map(|r| SyncItem {
                href: r.href,
                etag: r.etag,
                calendar_data: r.data,
                status: r.status,
                is_deleted: r.is_deleted,
            })
            .collect(),
    }
}
