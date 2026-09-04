pub use crate::webdav::types::{
    BatchItem, Collation, DavItem, Depth, MatchType, MediaType, ParamFilter, TextMatch,
};

use crate::webdav::xml;

/// An attachment stored via **managed attachments** (RFC 8607),
/// as returned by
/// [`CalDavClient::post_managed_attachment`](crate::CalDavClient::post_managed_attachment).
///
/// # Example
/// ```no_run
/// use fast_dav_rs::caldav::ManagedAttachment;
///
/// # fn print_attachment(att: &ManagedAttachment) {
/// println!("stored at {} (managed id {})", att.href, att.managed_id);
/// # }
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ManagedAttachment {
    /// Href of the stored attachment resource, taken verbatim from the
    /// `Location` response header.
    pub href: String,
    /// Opaque server-managed id, from the `Cal-Managed-ID` response header
    /// or the `managed-id` query parameter of the `Location` URL. Send it
    /// back as the `Cal-Managed-ID` header on updates/removals.
    pub managed_id: String,
}

/// Summary of a calendar (collection) returned by a `PROPFIND` depth=1.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CalendarInfo {
    pub href: String,
    pub displayname: Option<String>,
    pub description: Option<String>,
    pub timezone: Option<String>,
    pub color: Option<String>,
    pub etag: Option<String>,
    pub sync_token: Option<String>,
    pub supported_components: Vec<String>,
    /// Maximum resource size in octets the server accepts for this collection
    /// (RFC 4791 §5.2.3); `None` when the server does not advertise it.
    pub max_resource_size: Option<u64>,
    /// Media types the server accepts for calendar data in this collection
    /// (RFC 4791 §5.2.6), e.g. `text/calendar` version `2.0`.
    pub supported_calendar_data: Vec<MediaType>,
    /// Maximum number of attendees per scheduling instance the server accepts
    /// (RFC 4791 §5.2.4); `None` when the server does not advertise it.
    pub max_attendees_per_instance: Option<u32>,
}

/// Calendar object (event or task) returned by a `REPORT`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CalendarObject {
    pub href: String,
    pub etag: Option<String>,
    pub calendar_data: Option<String>,
    pub status: Option<String>,
}

/// Detail of an item returned by `sync-collection`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SyncItem {
    pub href: String,
    pub etag: Option<String>,
    pub calendar_data: Option<String>,
    pub status: Option<String>,
    pub is_deleted: bool,
}

/// Complete response to a `sync-collection` REPORT.
///
/// `truncated` is `true` when the server truncated the result set
/// (RFC 6578 §3.6): the multistatus then carries a `507 Insufficient Storage`
/// status — normally on the request-URI, which also surfaces in `items` with
/// that status. The returned `sync_token` remains valid for fetching the next
/// page of changes. Note the collection heuristic in
/// [`map_sync_response`](crate::caldav::map_sync_response): response elements
/// that echo a sync token without an etag/data payload are treated as the
/// collection entry and skipped, so a non-compliant server can hide member
/// changes that way (observable via the token, not via `truncated`).
#[derive(Debug, Clone)]
pub struct SyncResponse {
    pub sync_token: Option<String>,
    pub items: Vec<SyncItem>,
    /// `true` when the server truncated the result set (RFC 6578 §3.6, a
    /// `507 Insufficient Storage` status inside the multistatus).
    pub truncated: bool,
    /// `true` when this response is the result of an initial sync triggered
    /// by a stale sync token (`410 Gone` per RFC 6578 §3.11, or `403` +
    /// `valid-sync-token` per §3.2) — i.e. the report was re-issued with an
    /// empty token. Per RFC 6578 §3.4 such a response MUST NOT report
    /// deletions that predate the stale token, so callers must rebuild their
    /// caches from `items` instead of applying them incrementally. Always
    /// `false` for incremental syncs.
    pub resynced: bool,
}

/// One free/busy period reported by a `free-busy-query` REPORT (RFC 4791 §9.7).
///
/// Extracted from the `FREEBUSY` properties of the `VFREEBUSY` component the
/// server returns in `calendar-data`. Values are opaque server-provided strings
/// (typically iCalendar UTC date-times).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FreeBusyPeriod {
    /// Inclusive start of the period (opaque server-provided iCalendar date-time).
    pub start: String,
    /// End of the period (opaque server-provided string; may be a duration in rare servers).
    pub end: String,
    pub fb_type: FreeBusyType,
}

/// The `FBTYPE` classification of a [`FreeBusyPeriod`] (RFC 4791 §9.7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FreeBusyType {
    /// `FBTYPE=FREE` (or an unmapped-but-known value): the interval is free.
    Free,
    /// `FBTYPE=BUSY` — also the default when the parameter is absent.
    Busy,
    /// `FBTYPE=BUSY-TENTATIVE`.
    BusyTentative,
    /// `FBTYPE=BUSY-UNAVAILABLE`.
    BusyUnavailable,
}

/// A time-range filter for `comp-filter` and `prop-filter` (RFC 4791 §8.5).
///
/// When used as `expand` (RFC 4791 §9.6.5) **both** `start` and `end` are
/// mandatory; the client rejects an expand without `end` before any network
/// I/O. Wherever both bounds are set, `end` must be after `start`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TimeRange {
    /// Start of the time range (iCalendar UTC date-time, e.g. `20240101T000000Z`).
    pub start: String,
    /// Optional end of the time range. Mandatory when the range is used as
    /// `expand`; must be after `start` when present.
    pub end: Option<String>,
}

impl TimeRange {
    /// Create a new `TimeRange` with the given start and no end.
    pub fn new(start: impl Into<String>) -> Self {
        Self {
            start: start.into(),
            end: None,
        }
    }

    /// Set the end of the time range.
    pub fn with_end(mut self, end: impl Into<String>) -> Self {
        self.end = Some(end.into());
        self
    }

    /// Render this time-range as CalDAV XML (the `<C:time-range>` element).
    pub fn to_xml(&self) -> String {
        xml::time_range_xml(&self.start, self.end.as_deref())
    }
}

/// A property-level filter inside a `comp-filter` (RFC 4791 §8.2).
///
/// The RFC 4791 §9.7.2 DTD makes the children exclusive:
/// `is-not-defined | ((time-range | text-match)?, param-filter*)`.
/// [`PropFilter::to_xml`] enforces this by serialization precedence and
/// [`CalDavClient::calendar_query`] rejects a violating filter with
/// [`Error::InvalidInput`](crate::Error::InvalidInput) before any network I/O.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PropFilter {
    /// The iCalendar property name to filter on (e.g. `SUMMARY`, `DTSTART`).
    pub name: String,
    /// Optional text-match condition. Mutually exclusive with `time_range`
    /// (RFC 4791 §9.7.2): when both are set, `text_match` wins in the
    /// serialized XML and `calendar_query` rejects the filter.
    pub text_match: Option<TextMatch>,
    /// Optional time-range filter. Mutually exclusive with `text_match`
    /// (RFC 4791 §9.7.2). Also requires `end` > `start` when both bounds
    /// are set.
    pub time_range: Option<TimeRange>,
    /// Nested `param-filter` elements. Ignored (not serialized) when
    /// `is_not_defined` is set.
    pub param_filters: Vec<ParamFilter>,
    /// If `true`, matches resources where this property is **absent**.
    /// Excludes `text_match`, `time_range`, and `param_filters`
    /// (RFC 4791 §9.7.2).
    pub is_not_defined: bool,
}

impl PropFilter {
    /// Create a `PropFilter` that matches the property with a text-match.
    pub fn new(name: impl Into<String>, text_match: TextMatch) -> Self {
        Self {
            name: name.into(),
            text_match: Some(text_match),
            time_range: None,
            param_filters: vec![],
            is_not_defined: false,
        }
    }

    /// Create a `PropFilter` that matches resources where the property is **absent**.
    pub fn not_defined(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text_match: None,
            time_range: None,
            param_filters: vec![],
            is_not_defined: true,
        }
    }

    /// Set a time-range filter on this prop-filter.
    pub fn with_time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = Some(time_range);
        self
    }

    /// Add nested `param-filter` elements.
    pub fn with_param_filters(mut self, param_filters: Vec<ParamFilter>) -> Self {
        self.param_filters = param_filters;
        self
    }

    /// Render this prop-filter as CalDAV XML (the `<C:prop-filter>` element).
    ///
    /// Exclusivity is applied by precedence per the RFC 4791 §9.7.2 DTD
    /// (`is-not-defined | ((time-range | text-match)?, param-filter*)`):
    /// when `is_not_defined` is set only `<C:is-not-defined/>` is emitted,
    /// and `text_match` wins over `time_range` when both are set.
    /// [`CalDavClient::calendar_query`](crate::CalDavClient::calendar_query)
    /// rejects violating filters with an error before any network I/O.
    pub fn to_xml(&self) -> String {
        let mut inner = String::new();
        if self.is_not_defined {
            inner.push_str(xml::IS_NOT_DEFINED_XML);
        } else {
            if let Some(tm) = &self.text_match {
                inner.push_str(&tm.to_xml_for(true));
            } else if let Some(tr) = &self.time_range {
                inner.push_str(&tr.to_xml());
            }
            for pf in &self.param_filters {
                inner.push_str(&pf.to_xml_for(true));
            }
        }
        xml::prop_filter_xml(&self.name, &inner)
    }
}

/// A CalDAV calendar-query filter (RFC 4791 §8.1-8.5).
///
/// Combines a component-level `comp-filter` with optional nested
/// `prop-filter` elements and an `is-not-defined` test.
/// Use [`CalendarQueryFilter::to_filter_xml`] to produce the `<C:filter>`
/// XML fragment, or [`CalendarQueryFilter::to_query_body`] to produce the
/// complete `<C:calendar-query>` REPORT body.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CalendarQueryFilter {
    /// The iCalendar component name to filter on (e.g. `VEVENT`, `VTODO`).
    pub component: String,
    /// Optional time-range filter on the component.
    pub time_range: Option<TimeRange>,
    /// Nested `prop-filter` elements (AND semantics).
    pub prop_filters: Vec<PropFilter>,
    /// If `true`, matches resources where the component is **absent**.
    pub is_not_defined: bool,
}

impl CalendarQueryFilter {
    /// Create a new `CalendarQueryFilter` for the given component.
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            time_range: None,
            prop_filters: vec![],
            is_not_defined: false,
        }
    }

    /// Create a `CalendarQueryFilter` that matches resources where the component is **absent**.
    pub fn not_defined(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            time_range: None,
            prop_filters: vec![],
            is_not_defined: true,
        }
    }

    /// Set a time-range filter on the component.
    pub fn with_time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = Some(time_range);
        self
    }

    /// Add nested `prop-filter` elements.
    pub fn with_prop_filters(mut self, prop_filters: Vec<PropFilter>) -> Self {
        self.prop_filters = prop_filters;
        self
    }

    /// Build the `<C:filter>` XML fragment for a `calendar-query` REPORT.
    pub fn to_filter_xml(&self) -> String {
        let mut inner = String::new();
        if self.is_not_defined {
            inner.push_str(xml::IS_NOT_DEFINED_XML);
        } else {
            if let Some(tr) = &self.time_range {
                inner.push_str(&tr.to_xml());
            }
            for pf in &self.prop_filters {
                inner.push_str(&pf.to_xml());
            }
        }
        let comp = xml::comp_filter_xml(&self.component, &inner);
        let vcal = xml::comp_filter_xml("VCALENDAR", &comp);
        format!("<C:filter>{vcal}</C:filter>")
    }

    /// Build the complete `<C:calendar-query>` REPORT body.
    ///
    /// Set `include_data` to `true` to include `<C:calendar-data/>` in the
    /// requested properties alongside `<D:getetag/>`.
    pub fn to_query_body(&self, include_data: bool) -> String {
        let mut prop = String::from("<D:prop><D:getetag/>");
        if include_data {
            prop.push_str("<C:calendar-data/>");
        }
        prop.push_str("</D:prop>");
        format!(
            r#"<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">{prop}{}</C:calendar-query>"#,
            self.to_filter_xml()
        )
    }
}
