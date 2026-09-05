use bytes::Bytes;
use hyper::{HeaderMap, Method, Response, header};
use percent_encoding::utf8_percent_encode;

use crate::BatchItem;
use crate::Depth;
use crate::caldav::builder::CalDavClientBuilder;
use crate::caldav::streaming::parse_multistatus_bytes;
use crate::caldav::types::{
    CalendarInfo, CalendarObject, CalendarQueryFilter, DavItem, FreeBusyPeriod, FreeBusyType,
    ManagedAttachment, SyncItem, SyncResponse, TimeRange,
};
use crate::caldav::validation::{ValidationLevel, validate_icalendar_level};
use crate::impl_dav_client_delegates;
use crate::webdav::client::WebDavClient;
use crate::webdav::types::map_sync_rows;
use crate::webdav::xml::{
    data_element_xml, time_range_xml, validate_component_name, validate_utc_datetime,
};
use crate::{Error, Operation, Result};

pub use crate::webdav::client::RequestCompressionMode;

/// Content-Type for iCalendar `PUT` bodies, without the `version` parameter
/// (appended automatically when the body declares a `VERSION`).
pub const ICAL_CONTENT_TYPE: &str = "text/calendar; charset=utf-8";

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
    validation_level: ValidationLevel,
}

impl_dav_client_delegates!(
    CalDavClient,
    ICAL_CONTENT_TYPE,
    "urn:ietf:params:xml:ns:caldav",
    "calendar-data",
    crate::caldav::types::SyncResponse,
    crate::caldav::client::map_sync_response,
    validation_level: validation_level: crate::caldav::validation::ValidationLevel,
    validate: prepare_ical_put
);

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

    /// Client-side iCalendar gate behind the CalDAV `PUT` methods: validates
    /// `body` per the configured [`ValidationLevel`] and returns the wire
    /// `Content-Type` (with the `version` parameter the body declares).
    ///
    /// Runs before any network I/O; an invalid body fails with
    /// [`Error::InvalidICalendar`].
    fn prepare_ical_put(&self, body: &[u8]) -> Result<header::HeaderValue> {
        if self.validation_level == ValidationLevel::None {
            return Ok(header::HeaderValue::from_static(ICAL_CONTENT_TYPE));
        }
        let version = validate_icalendar_level(body, self.validation_level)?;
        Ok(header::HeaderValue::from_str(&format!(
            "{ICAL_CONTENT_TYPE}; version={version}"
        ))?)
    }

    /// Send a `PUT` with an iCalendar body (`text/calendar`).
    ///
    /// The body is validated client-side per the configured
    /// [`ValidationLevel`](crate::caldav::ValidationLevel) (default
    /// `Structural`) **before any network I/O**; invalid bodies fail with
    /// [`Error::InvalidICalendar`](crate::Error::InvalidICalendar). On a body
    /// that declares a `VERSION`, the wire `Content-Type` gains a matching
    /// `version` parameter.
    ///
    /// Use [`put_if_match`] or [`put_if_none_match`] for safer conditional writes.
    pub async fn put(&self, path: &str, ical_bytes: Bytes) -> Result<Response<Bytes>> {
        let content_type = self.prepare_ical_put(&ical_bytes)?;
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, content_type);
        self.send(Method::PUT, path, h, Some(ical_bytes), None)
            .await
    }

    /// Create-only `PUT` guarded by `If-None-Match: *`.
    ///
    /// Fails if the resource already exists. The body is validated client-side
    /// per the configured [`ValidationLevel`](crate::caldav::ValidationLevel)
    /// **before any network I/O**, exactly as in [`put`].
    pub async fn put_if_none_match(
        &self,
        path: &str,
        ical_bytes: Bytes,
    ) -> Result<Response<Bytes>> {
        let content_type = self.prepare_ical_put(&ical_bytes)?;
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, content_type);
        h.insert(header::IF_NONE_MATCH, header::HeaderValue::from_static("*"));
        self.send(Method::PUT, path, h, Some(ical_bytes), None)
            .await
    }

    /// Send a CalDAV `MKCALENDAR` to create a calendar collection.
    ///
    /// Sent with an explicit `Depth: 0` header (the operation applies to the
    /// collection being created only).
    pub async fn mkcalendar(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_static("0"));
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

    /// Read a calendar's `calendar-timezone` property (RFC 4791 §5.2.2).
    ///
    /// Sends a `Depth: 0` `PROPFIND` for `<C:calendar-timezone/>` against
    /// `calendar_path` and returns the property value verbatim: an iCalendar
    /// object with exactly one `VTIMEZONE` component (or `None` when the
    /// server does not store the property — e.g. Radicale). The same value is
    /// also surfaced per calendar in
    /// [`CalendarInfo::timezone`](crate::caldav::CalendarInfo::timezone) via
    /// [`list_calendars`](Self::list_calendars); this method reads it for a
    /// single calendar without listing the whole home set.
    ///
    /// Parse the returned `VTIMEZONE` with a dedicated iCalendar crate (e.g.
    /// `icalendar`) to derive the UTC offset rules; this library intentionally
    /// does not interpret the component. The write path (RFC 4791 §5.2.2
    /// `MKCALENDAR`/`PROPPATCH` with a `calendar-timezone` value) is not
    /// exposed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnexpectedStatus`] with
    /// [`Operation::PropfindCalendarTimezone`] if the `PROPFIND` fails or the
    /// server responds with a non-success status.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use fast_dav_rs::CalDavClient;
    /// # async fn example(client: &CalDavClient) -> Result<(), fast_dav_rs::Error> {
    /// if let Some(timezone) = client.calendar_timezone("calendars/personal/").await? {
    ///     // `timezone` is the raw iCalendar VTIMEZONE object stored by the server.
    ///     println!("calendar timezone object: {timezone}");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn calendar_timezone(&self, calendar_path: &str) -> Result<Option<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <C:calendar-timezone/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(calendar_path, Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(Error::unexpected_status(
                Operation::PropfindCalendarTimezone,
                resp.status(),
            ));
        }
        let body = resp.into_body();
        Ok(parse_multistatus_bytes(&body)?
            .items
            .into_iter()
            .find_map(|item| item.calendar_timezone))
    }

    /// List CalDAV collections under a calendar home-set (`Depth: 1` PROPFIND).
    pub async fn list_calendars(&self, home_set_path: &str) -> Result<Vec<CalendarInfo>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
  <D:prop>
    <D:displayname/>
    <C:calendar-description/>
    <C:calendar-timezone/>
    <A:calendar-color/>
    <C:supported-calendar-component-set/>
    <C:max-resource-size/>
    <C:supported-calendar-data/>
    <C:max-attendees-per-instance/>
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
    /// `expand` asks the server to expand recurring components into their
    /// individual instances covering the given range (RFC 4791 §9.6,
    /// `<C:expand>`). When it is `Some`, `include_data` is implied `true`
    /// (the server returns expanded calendar data).
    ///
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if `component` is empty or
    /// contains characters outside ASCII alphanumerics and `-`, if `start`
    /// /`end` is provided but is not a valid iCalendar UTC date-time
    /// (`YYYYMMDDTHHMMSSZ`), or if `expand` is provided but its start/end are
    /// not valid iCalendar UTC date-times. When `expand` is provided without
    /// an `end` (mandatory per RFC 4791 §9.6.5) the call fails with
    /// [`Error::InvalidInput`]; when `end <= start` (or the time-range `end`
    /// precedes `start`) it fails with [`Error::InvalidDateTime`]. Also
    /// returns an error if the REPORT request fails or the server responds
    /// with a non-success status.
    pub async fn calendar_query_timerange(
        &self,
        calendar_path: &str,
        component: &str,
        start: Option<&str>,
        end: Option<&str>,
        include_data: bool,
        expand: Option<TimeRange>,
    ) -> Result<Vec<CalendarObject>> {
        validate_component_name(component, "invalid calendar-query component")?;
        if let Some(s) = start {
            validate_utc_datetime(s, "invalid calendar-query start")?;
        }
        if let Some(e) = end {
            validate_utc_datetime(e, "invalid calendar-query end")?;
        }
        validate_expand("invalid calendar-query", expand.as_ref())?;
        if let (Some(s), Some(e)) = (start, end) {
            validate_time_range_order("invalid calendar-query time-range", s, e)?;
        }

        let xml = build_calendar_query_body(component, start, end, include_data, expand.as_ref());

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
    /// empty or contains characters outside ASCII alphanumerics and `-`, if
    /// any time-range value is not a valid iCalendar UTC date-time, if a
    /// `prop-filter` violates the RFC 4791 §9.7.2 child exclusivity
    /// (`is-not-defined` excludes everything else; `text-match` and
    /// `time-range` are mutually exclusive) with [`Error::InvalidInput`], or
    /// if a time-range `end` precedes its `start` with
    /// [`Error::InvalidDateTime`]. Also returns an error if the REPORT
    /// request fails or the server responds with a non-success status.
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
                validate_time_range_order("invalid calendar-query time-range", &tr.start, end)?;
            }
        }
        for pf in &filter.prop_filters {
            if pf.is_not_defined
                && (pf.text_match.is_some()
                    || pf.time_range.is_some()
                    || !pf.param_filters.is_empty())
            {
                return Err(Error::InvalidInput(format!(
                    "calendar-query prop-filter `{}`: is-not-defined excludes \
                     text-match, time-range, and param-filter children (RFC 4791 §9.7.2)",
                    pf.name
                )));
            }
            if pf.text_match.is_some() && pf.time_range.is_some() {
                return Err(Error::InvalidInput(format!(
                    "calendar-query prop-filter `{}`: text-match and time-range \
                     are mutually exclusive (RFC 4791 §9.7.2)",
                    pf.name
                )));
            }
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
                    validate_time_range_order(
                        "invalid calendar-query prop-filter time-range",
                        &tr.start,
                        end,
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
    ///
    /// The REPORT is sent with `Depth: 0` (RFC 4791 §7.9) and answers with one
    /// multistatus element per requested href.
    ///
    /// `expand` asks the server to expand recurring components into their
    /// individual instances covering the given range (RFC 4791 §9.6,
    /// `<C:expand>`). When it is `Some`, `include_data` is implied `true`
    /// (the server returns expanded calendar data).
    ///
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if `expand` is provided
    /// but its start/end are not valid iCalendar UTC date-times, if `expand`
    /// has no `end` (mandatory per RFC 4791 §9.6.5 — [`Error::InvalidInput`]),
    /// or if its `end` is not after its `start` ([`Error::InvalidDateTime`]).
    /// Also returns an error if the REPORT request fails or the server
    /// responds with a non-success status.
    pub async fn calendar_multiget<I, S>(
        &self,
        calendar_path: &str,
        hrefs: I,
        include_data: bool,
        expand: Option<TimeRange>,
    ) -> Result<Vec<CalendarObject>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        validate_expand("invalid calendar-multiget", expand.as_ref())?;

        let Some(body) = build_calendar_multiget_body(hrefs, include_data, expand.as_ref()) else {
            return Ok(Vec::new());
        };

        let resp = self.report(calendar_path, Depth::Zero, &body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportCalendarMultiget,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        Ok(map_calendar_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Fetch specific calendar objects via `calendar-multiget`, split into
    /// concurrent batches.
    ///
    /// `hrefs` is chunked into slices of `batch_size`; one `calendar-multiget`
    /// REPORT is issued per chunk, with at most `max_concurrency` REPORTs in
    /// flight at any time (a `max_concurrency` of 0 is treated as 1). This
    /// avoids the single huge request/response pair of
    /// [`calendar_multiget`](Self::calendar_multiget) for large fetch lists
    /// and parallelizes the server-side work.
    ///
    /// `expand` asks the server to expand recurring components into their
    /// individual instances covering the given range (RFC 4791 §9.6,
    /// `<C:expand>`). When it is `Some`, `include_data` is implied `true`.
    ///
    /// # Result shape and ordering
    ///
    /// Each item of the returned vector is one [`CalendarObject`] wrapped in a
    /// [`BatchItem`]: `pub_path` is the `calendar_path` the REPORT was sent
    /// to, `hrefs` holds the exact hrefs the chunk's REPORT requested, and
    /// the object's own URL is in [`CalendarObject::href`]. Results
    /// are **deterministically ordered by chunk index first**, then by the
    /// order in which the server returned the objects within that chunk's
    /// multistatus (which matches the request href order for compliant
    /// servers). A chunk that yields no objects contributes no items.
    ///
    /// Each item also carries [`BatchItem::missing_hrefs`]: the requested
    /// hrefs the server did not answer with a `<D:response>` element (exact
    /// href string comparison — a compliant server echoes every requested
    /// href, possibly with an error status). A non-empty value signals a
    /// non-compliant server; the answered objects are still delivered.
    ///
    /// Empty hrefs are dropped from `hrefs` **before** chunking (they never
    /// reach a REPORT, and they are not recorded in any `BatchItem::hrefs`);
    /// an input with no non-empty href yields `Ok(Vec::new())` without any
    /// network I/O.
    ///
    /// # Partial failure
    ///
    /// A failed chunk (transport error, non-success status, or an
    /// unparsable response body) produces exactly **one** error [`BatchItem`];
    /// sibling chunks are unaffected and still contribute their results. The
    /// failing chunk's `hrefs` field carries the requested hrefs, so callers
    /// know exactly which objects to re-fetch. The
    /// method itself only fails before any network I/O (see below).
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidConfig`] **before any network I/O** if
    /// `batch_size` is 0. Returns [`Error::InvalidDateTime`] or
    /// [`Error::InvalidInput`] **before any network I/O** if `expand` is
    /// provided but its start/end are not valid iCalendar UTC date-times,
    /// `end` is missing (mandatory per RFC 4791 §9.6.5), or `end` is not
    /// after `start`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::CalDavClient;
    ///
    /// # async fn example(client: &CalDavClient) -> fast_dav_rs::Result<()> {
    /// let hrefs: Vec<String> = (0..250)
    ///     .map(|i| format!("/calendars/user/work/event-{i}.ics"))
    ///     .collect();
    /// // 100 hrefs per REPORT, at most 4 REPORTs in flight.
    /// let items = client
    ///     .calendar_multiget_many("calendars/user/work/", &hrefs, true, None, 100, 4)
    ///     .await?;
    /// for item in &items {
    ///     match &item.result {
    ///         Ok(obj) => println!("{} -> {:?}", obj.href, obj.etag),
    ///         Err(e) => eprintln!("batch failed: {e}"),
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn calendar_multiget_many(
        &self,
        calendar_path: &str,
        hrefs: &[String],
        include_data: bool,
        expand: Option<TimeRange>,
        batch_size: usize,
        max_concurrency: usize,
    ) -> Result<Vec<BatchItem<CalendarObject>>> {
        if batch_size == 0 {
            return Err(Error::InvalidConfig(
                "calendar_multiget_many: batch_size must be greater than zero".to_owned(),
            ));
        }
        validate_expand("invalid calendar-multiget", expand.as_ref())?;

        // Shared engine: empty-href filtering, chunking, chunked REPORTs,
        // ordering, per-chunk failure isolation and missing-hrefs
        // reconciliation.
        crate::webdav::multiget::multiget_many(
            &self.webdav,
            Operation::ReportCalendarMultiget,
            calendar_path,
            hrefs,
            batch_size,
            max_concurrency,
            |chunk| build_calendar_multiget_body(chunk.iter(), include_data, expand.as_ref()),
            map_calendar_objects,
        )
        .await
    }

    /// Incrementally synchronise a calendar collection using `sync-collection`.
    ///
    /// `expand` asks the server to expand recurring components into their
    /// individual instances covering the given range (RFC 4791 §9.6,
    /// `<C:expand>`). When it is `Some`, `include_data` is implied `true`
    /// (the server returns expanded calendar data).
    ///
    /// # Truncation
    ///
    /// If the server truncates the result set (RFC 6578 §3.6), the returned
    /// [`SyncResponse`] has `truncated == true` and the request-URI appears
    /// in `items` with a `HTTP/1.1 507 Insufficient Storage` status. The
    /// returned sync token is valid for fetching the next page of changes.
    ///
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if `expand` is provided
    /// but its start/end are not valid iCalendar UTC date-times, if `expand`
    /// has no `end` (mandatory per RFC 4791 §9.6.5 — [`Error::InvalidInput`]),
    /// or if its `end` is not after its `start` ([`Error::InvalidDateTime`]).
    /// Also returns an error if the REPORT request fails or the server
    /// responds with a non-success status.
    pub async fn sync_collection(
        &self,
        calendar_path: &str,
        sync_token: Option<&str>,
        limit: Option<u32>,
        include_data: bool,
        expand: Option<TimeRange>,
    ) -> Result<SyncResponse> {
        validate_expand("invalid sync-collection", expand.as_ref())?;

        let body = build_sync_collection_body(sync_token, limit, include_data, expand.as_ref());

        let (headers, items, token) = self
            .webdav
            .sync_collection_report(calendar_path, &body)
            .await?;
        Ok(map_sync_response(&headers, items, token))
    }

    /// Query free/busy information for a calendar via a `free-busy-query`
    /// REPORT (RFC 4791 §9.7).
    ///
    /// The server reports the busy periods it knows about for `start`..`end`
    /// (iCalendar UTC date-times, e.g. `20240101T000000Z`). Periods are
    /// extracted from the `FREEBUSY` properties of the returned `VFREEBUSY`
    /// component; periods in `start/duration` form and periods with an
    /// unrecognized `FBTYPE` are skipped (free/busy line folding is not
    /// unfolded). The REPORT is sent with `Depth: 1` as mandated by
    /// RFC 4791 §9.7.
    ///
    /// Both response shapes are handled: the RFC 4791 §7.10.2 multistatus with
    /// a `calendar-data` property, and the bare `text/calendar` `VFREEBUSY`
    /// body served by Sabre/DAV (which skips the multistatus wrapper).
    ///
    /// # Errors
    ///
    /// Returns an error **before any network I/O** if `start` or `end` is not
    /// a valid iCalendar UTC date-time (`YYYYMMDDTHHMMSSZ`) or if `end` is not
    /// after `start` ([`Error::InvalidDateTime`]). Also returns an error if
    /// the REPORT request fails or the server responds with a non-success
    /// status.
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::CalDavClient;
    /// use fast_dav_rs::caldav::FreeBusyType;
    ///
    /// # async fn example(client: &CalDavClient) -> fast_dav_rs::Result<()> {
    /// let periods = client
    ///     .free_busy_query("calendars/user/work/", "20240101T000000Z", "20240108T000000Z")
    ///     .await?;
    /// for period in &periods {
    ///     if period.fb_type != FreeBusyType::Free {
    ///         println!("busy: {} -> {}", period.start, period.end);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn free_busy_query(
        &self,
        calendar_path: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<FreeBusyPeriod>> {
        validate_utc_datetime(start, "invalid free-busy-query start")?;
        validate_utc_datetime(end, "invalid free-busy-query end")?;
        validate_time_range_order("invalid free-busy-query time-range", start, end)?;

        let body = format!(
            r#"<C:free-busy-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">{}</C:free-busy-query>"#,
            time_range_xml(start, Some(end))
        );

        let resp = self.report(calendar_path, Depth::One, &body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ReportFreeBusyQuery,
                status: resp.status(),
            });
        }
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_ascii_lowercase())
            .unwrap_or_default();
        let body = resp.into_body();
        let mut periods = Vec::new();
        if content_type.starts_with("text/calendar") {
            // Sabre/DAV serves free-busy-query results as a bare iCalendar
            // body instead of a multistatus; parse the VFREEBUSY directly.
            periods.extend(parse_free_busy_periods(&String::from_utf8_lossy(&body)));
        } else {
            for item in parse_multistatus_bytes(&body)?.items {
                if let Some(data) = item.calendar_data {
                    periods.extend(parse_free_busy_periods(&data));
                }
            }
        }
        Ok(periods)
    }

    /// Create an in-memory sync session for `collection`
    /// ([`SyncSession`](crate::webdav::SyncSession)): RFC 6578
    /// `sync-collection` deltas with transparent full-list fallback,
    /// fetching `calendar-data` alongside the etags. The caller persists the
    /// returned sync token between runs and restores it with
    /// `with_sync_token`; see the `SyncSession` docs for the algorithm.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::CalDavClient;
    ///
    /// # async fn example(client: &CalDavClient) -> fast_dav_rs::Result<()> {
    /// let session = client.sync_session("calendars/user/work/");
    /// let snapshot = session.initial().await?;
    /// println!("{} items, token {:?}", snapshot.items.len(), snapshot.sync_token);
    /// # Ok(())
    /// # }
    /// ```
    pub fn sync_session(&self, collection: impl Into<String>) -> crate::webdav::SyncSession {
        crate::webdav::sync::SyncSession::new(self.webdav.clone(), collection)
            .with_data_spec(crate::webdav::sync::CALENDAR_DATA_SPEC)
    }

    /// Store an attachment via **managed attachments** (RFC 8607).
    ///
    /// Sends `POST <calendar_path>?action=attachment-add&uid=<ics_uid>`
    /// (plus `&recurrence-id=<recurrence_id>` when given) with `body`
    /// verbatim as the attachment content and `content_type` as its
    /// `Content-Type`. This collection-targeted request shape is the
    /// non-IETF form deployed by Apple CalendarServer; RFC 8607 §3.4
    /// instead targets the calendar object resource itself (e.g.
    /// `POST /events/64.ics?action=attachment-add`, selecting recurrence
    /// instances via a comma-separated `rid` query parameter), and that
    /// conforming alternative is not implemented.
    ///
    /// On success the server-stored attachment is returned:
    /// `href` (from the `Location` header) identifies the attachment
    /// resource for later `GET`/`PUT`/`DELETE`, and `managed_id` (from the
    /// `Cal-Managed-ID` header, RFC 8607 §5.1, or the `managed-id` query
    /// parameter of the `Location` when the header is absent) must be sent
    /// back as the `Cal-Managed-ID` header on subsequent updates/removals
    /// of the attachment.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) **before
    /// any network I/O** when `content_type` cannot form a valid HTTP
    /// header value. Returns
    /// [`Error::UnexpectedStatus`](crate::Error::UnexpectedStatus) with
    /// [`Operation::PostManagedAttachment`](crate::Operation::PostManagedAttachment)
    /// when the server responds with a non-success status, and
    /// [`Error::other`](crate::Error::other) when a success response carries
    /// neither a `Cal-Managed-ID` header nor a `managed-id` `Location`
    /// query parameter (the server does not implement managed attachments).
    ///
    /// # Example
    /// ```no_run
    /// use bytes::Bytes;
    /// use fast_dav_rs::{CalDavClient, Result};
    ///
    /// # async fn example(client: &CalDavClient) -> Result<()> {
    /// let attachment = client
    ///     .post_managed_attachment(
    ///         "calendars/work/",
    ///         "20010712T182145Z-123401@example.com",
    ///         None,
    ///         Bytes::from_static(b"agenda attachment"),
    ///         "text/plain",
    ///     )
    ///     .await?;
    /// println!("stored at {} (managed id {})", attachment.href, attachment.managed_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn post_managed_attachment(
        &self,
        calendar_path: &str,
        ics_uid: &str,
        recurrence_id: Option<&str>,
        body: Bytes,
        content_type: &str,
    ) -> Result<ManagedAttachment> {
        let content_type_value = header::HeaderValue::from_str(content_type).map_err(|err| {
            Error::InvalidInput(format!(
                "content-type cannot form a valid header value: {err}"
            ))
        })?;
        // `build_uri` treats its input as a path (`?` is percent-encoded), so
        // resolve the collection to an absolute URI first and append the
        // attachment-add query afterwards — an absolute URL passes through
        // `send` verbatim.
        let mut url = format!(
            "{}?action=attachment-add&uid={}",
            self.build_uri(calendar_path)?,
            utf8_percent_encode(ics_uid, QUERY_UNRESERVED)
        );
        if let Some(recurrence_id) = recurrence_id {
            url.push_str("&recurrence-id=");
            url.push_str(&utf8_percent_encode(recurrence_id, QUERY_UNRESERVED).to_string());
        }
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, content_type_value);
        let resp = self.send(Method::POST, &url, h, Some(body), None).await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PostManagedAttachment,
                status,
            });
        }
        let headers = resp.headers();
        let href = headers
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let managed_id = headers
            .get("cal-managed-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .or_else(|| href.as_deref().and_then(managed_id_from_location));
        match (href, managed_id) {
            (Some(href), Some(managed_id)) => Ok(ManagedAttachment { href, managed_id }),
            (_, None) => Err(Error::other("attachment POST returned no managed id")),
            (None, Some(_)) => Err(Error::other("attachment POST returned no Location header")),
        }
    }
}

pub fn escape_xml(input: &str) -> String {
    crate::webdav::xml::escape_xml(input)
}

/// RFC 3986 unreserved characters — the safe set for URL query values.
const QUERY_UNRESERVED: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Extract the percent-decoded `managed-id` query parameter from a
/// `Location` URL (fallback for servers that omit the `Cal-Managed-ID`
/// response header, RFC 8607 §5.1).
fn managed_id_from_location(location: &str) -> Option<String> {
    let query = location.split_once('?')?.1.split('#').next()?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "managed-id").then(|| {
            percent_encoding::percent_decode_str(value)
                .decode_utf8_lossy()
                .into_owned()
        })
    })
}

/// Reject `end <= start` for structurally valid iCalendar UTC date-times
/// (RFC 4791 §9.9). Both values are fixed-format `YYYYMMDDTHHMMSSZ`, so
/// lexicographic order is chronological.
fn validate_time_range_order(context: &str, start: &str, end: &str) -> Result<()> {
    if end <= start {
        return Err(Error::InvalidDateTime {
            context: context.to_owned(),
            value: end.to_owned(),
            reason: "end must be after start",
        });
    }
    Ok(())
}

/// Validate an `expand` time-range before any network I/O (RFC 4791 §9.6.5):
/// both `start` and `end` are `#REQUIRED` attributes of `<C:expand>` and
/// `end` must be after `start`.
fn validate_expand(context: &str, expand: Option<&TimeRange>) -> Result<()> {
    let Some(tr) = expand else {
        return Ok(());
    };
    validate_utc_datetime(&tr.start, &format!("{context} expand start"))?;
    let Some(end) = tr.end.as_deref() else {
        return Err(Error::InvalidInput(format!(
            "{context} expand requires an `end`: RFC 4791 §9.6.5 makes both \
             start and end mandatory on <C:expand>"
        )));
    };
    validate_utc_datetime(end, &format!("{context} expand end"))?;
    validate_time_range_order(&format!("{context} expand"), &tr.start, end)
}

/// Extract `FreeBusyPeriod`s from the `FREEBUSY` properties of a `VFREEBUSY`
/// component (minimal line-based parser — no full iCalendar parser).
///
/// A `FREEBUSY` line looks like
/// `FREEBUSY;FBTYPE=BUSY-UNAVAILABLE:start/end,start/end`. A missing `FBTYPE`
/// defaults to `Busy`; an unrecognized `FBTYPE` value skips the line's
/// periods. Periods in `start/duration` form are skipped.
fn parse_free_busy_periods(calendar_data: &str) -> Vec<FreeBusyPeriod> {
    let mut out = Vec::new();
    for line in calendar_data.lines() {
        let Some(rest) = line.trim().strip_prefix("FREEBUSY") else {
            continue;
        };
        if !rest.starts_with(':') && !rest.starts_with(';') {
            continue;
        }
        let Some((params, value)) = rest.split_once(':') else {
            continue;
        };
        let fb_type = fbtype_from_params(params);
        for period in value.split(',') {
            let Some((start, end)) = period.split_once('/') else {
                continue;
            };
            // ponytail: start/duration periods (end starting with `P`) are skipped;
            // parse DURATION values if real servers start emitting them
            if end.starts_with('P') || end.starts_with('p') {
                continue;
            }
            if start.is_empty() || end.is_empty() {
                continue;
            }
            let Some(fb_type) = fb_type else {
                continue;
            };
            out.push(FreeBusyPeriod {
                start: start.to_owned(),
                end: end.to_owned(),
                fb_type,
            });
        }
    }
    out
}

/// Map the `FBTYPE` parameter of a `FREEBUSY` property (RFC 4791 §9.7.3).
///
/// Returns `Some` with the mapped type — defaulting to `Busy` when the
/// parameter is absent — or `None` when the value is unrecognized (the
/// caller skips those periods).
fn fbtype_from_params(params: &str) -> Option<FreeBusyType> {
    for param in params.split(';') {
        let Some((name, value)) = param.split_once('=') else {
            continue;
        };
        if !name.eq_ignore_ascii_case("FBTYPE") {
            continue;
        }
        return match value.trim().to_ascii_uppercase().as_str() {
            "BUSY" => Some(FreeBusyType::Busy),
            "BUSY-TENTATIVE" => Some(FreeBusyType::BusyTentative),
            "BUSY-UNAVAILABLE" => Some(FreeBusyType::BusyUnavailable),
            "FREE" => Some(FreeBusyType::Free),
            _ => None,
        };
    }
    Some(FreeBusyType::Busy)
}

pub fn build_calendar_query_body(
    component: &str,
    start: Option<&str>,
    end: Option<&str>,
    include_data: bool,
    expand: Option<&TimeRange>,
) -> String {
    let mut prop = String::from("<D:prop><D:getetag/>");
    if include_data || expand.is_some() {
        prop.push_str(&data_element_xml(
            "calendar-data",
            expand.map(|tr| (tr.start.as_str(), tr.end.as_deref())),
        ));
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

/// Build a `calendar-multiget` REPORT request body (RFC 4791 §7.9).
///
/// The body carries `<D:getetag/>` plus `<C:calendar-data/>` when
/// `include_data` is set (implied when `expand` is given — the server returns
/// expanded calendar data, RFC 4791 §9.6). Returns `None` when `hrefs`
/// contains no non-empty href (such a request would be invalid; callers such
/// as [`calendar_multiget`](crate::CalDavClient::calendar_multiget) skip the
/// network round-trip entirely). Empty hrefs inside `hrefs` are dropped and
/// XML metacharacters are escaped.
///
/// # Example
///
/// ```
/// use fast_dav_rs::caldav::build_calendar_multiget_body;
///
/// let body = build_calendar_multiget_body(["/cal/a.ics", ""], true, None)
///     .expect("at least one non-empty href");
/// assert!(body.contains("<C:calendar-multiget"));
/// assert!(body.contains("<D:href>/cal/a.ics</D:href>"));
/// assert!(!body.contains("<D:href></D:href>"), "empty hrefs are dropped");
/// assert!(body.contains("<C:calendar-data/>"));
/// ```
pub fn build_calendar_multiget_body<I, S>(
    hrefs: I,
    include_data: bool,
    expand: Option<&TimeRange>,
) -> Option<String>
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
        expand.map(|tr| (tr.start.as_str(), tr.end.as_deref())),
    )
}

pub fn build_sync_collection_body(
    sync_token: Option<&str>,
    limit: Option<u32>,
    include_data: bool,
    expand: Option<&TimeRange>,
) -> String {
    crate::webdav::xml::build_sync_collection_body(
        sync_token,
        limit,
        include_data,
        "urn:ietf:params:xml:ns:caldav",
        "calendar-data",
        expand.map(|tr| (tr.start.as_str(), tr.end.as_deref())),
        crate::webdav::types::SyncLevel::One,
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
                max_resource_size: item.max_resource_size,
                supported_calendar_data: std::mem::take(&mut item.supported_calendar_data),
                max_attendees_per_instance: item.max_attendees_per_instance,
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

/// Map raw multistatus items into a CalDAV [`SyncResponse`] (RFC 6578).
///
/// The sync token is resolved top-level first, then from the `Sync-Token`
/// response header, then from the first per-item token. `truncated` is set
/// when any response element carries a `507 Insufficient Storage` status
/// (RFC 6578 §3.6 result truncation — normally on the request-URI).
///
/// Collection heuristic: response elements flagged as collections, or echoing
/// a sync token without an etag and without a calendar-data payload, are
/// treated as the collection entry and skipped. A non-compliant server can
/// abuse this to hide member changes; the `truncated` flag and the returned
/// token are the observable signals.
pub fn map_sync_response(
    headers: &HeaderMap,
    items: Vec<DavItem>,
    top_level_sync_token: Option<String>,
) -> SyncResponse {
    let (sync_token, rows, truncated) =
        map_sync_rows(headers, items, top_level_sync_token, |item| {
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
        truncated,
        resynced: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_period_lines_with_fbtype() {
        let ical = "BEGIN:VFREEBUSY\r\nFREEBUSY;FBTYPE=BUSY-UNAVAILABLE:19970101T180000Z/19970102T070000Z,19970103T180000Z/19970104T070000Z\r\nEND:VFREEBUSY";
        let periods = parse_free_busy_periods(ical);
        assert_eq!(periods.len(), 2);
        assert_eq!(periods[0].fb_type, FreeBusyType::BusyUnavailable);
        assert_eq!(periods[0].start, "19970101T180000Z");
        assert_eq!(periods[0].end, "19970102T070000Z");
        assert_eq!(periods[1].fb_type, FreeBusyType::BusyUnavailable);
        assert_eq!(periods[1].start, "19970103T180000Z");
        assert_eq!(periods[1].end, "19970104T070000Z");
    }

    #[test]
    fn missing_fbtype_defaults_to_busy() {
        let periods = parse_free_busy_periods("FREEBUSY:19970105T100000Z/19970105T120000Z");
        assert_eq!(periods.len(), 1);
        assert_eq!(periods[0].fb_type, FreeBusyType::Busy);
    }

    #[test]
    fn maps_all_fbtype_values_case_insensitively() {
        let ical = "FREEBUSY;FBTYPE=FREE:a/b\r\n\
            FREEBUSY;FBTYPE=busy:c/d\r\n\
            FREEBUSY;FBTYPE=BUSY-TENTATIVE:e/f\r\n\
            FREEBUSY;fbtype=BUSY-UNAVAILABLE:g/h";
        let periods = parse_free_busy_periods(ical);
        assert_eq!(periods.len(), 4);
        assert_eq!(periods[0].fb_type, FreeBusyType::Free);
        assert_eq!(periods[1].fb_type, FreeBusyType::Busy);
        assert_eq!(periods[2].fb_type, FreeBusyType::BusyTentative);
        assert_eq!(periods[3].fb_type, FreeBusyType::BusyUnavailable);
    }

    #[test]
    fn unrecognized_fbtype_skips_period() {
        let ical = "FREEBUSY;FBTYPE=WOBBLE:19970105T100000Z/19970105T120000Z\r\n\
            FREEBUSY;FBTYPE=BUSY:19970106T100000Z/19970106T120000Z";
        let periods = parse_free_busy_periods(ical);
        assert_eq!(periods.len(), 1);
        assert_eq!(periods[0].fb_type, FreeBusyType::Busy);
        assert_eq!(periods[0].start, "19970106T100000Z");
    }

    #[test]
    fn start_duration_periods_are_skipped() {
        let ical = "FREEBUSY;FBTYPE=BUSY:19970105T100000Z/PT2H";
        assert!(parse_free_busy_periods(ical).is_empty());
    }

    #[test]
    fn non_freebusy_lines_are_ignored() {
        let ical = "BEGIN:VFREEBUSY\r\nDTSTART:19970101T000000Z\r\nSUMMARY:Busy times\r\nFREEBUSY:19970105T100000Z/19970105T120000Z\r\nEND:VFREEBUSY";
        let periods = parse_free_busy_periods(ical);
        assert_eq!(periods.len(), 1);
        assert_eq!(periods[0].start, "19970105T100000Z");
    }
}
