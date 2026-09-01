pub use crate::webdav::types::{
    BatchItem, Collation, DavItem, Depth, MatchType, ParamFilter, TextMatch,
};

use crate::webdav::xml;

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
#[derive(Debug, Clone)]
pub struct SyncResponse {
    pub sync_token: Option<String>,
    pub items: Vec<SyncItem>,
}

/// A time-range filter for `comp-filter` and `prop-filter` (RFC 4791 §8.5).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TimeRange {
    /// Start of the time range (iCalendar UTC date-time, e.g. `20240101T000000Z`).
    pub start: String,
    /// Optional end of the time range.
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
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PropFilter {
    /// The iCalendar property name to filter on (e.g. `SUMMARY`, `DTSTART`).
    pub name: String,
    /// Optional text-match condition.
    pub text_match: Option<TextMatch>,
    /// Optional time-range filter.
    pub time_range: Option<TimeRange>,
    /// Nested `param-filter` elements.
    pub param_filters: Vec<ParamFilter>,
    /// If `true`, matches resources where this property is **absent**.
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
    pub fn to_xml(&self) -> String {
        let mut inner = String::new();
        if self.is_not_defined {
            inner.push_str(xml::IS_NOT_DEFINED_XML);
        } else {
            if let Some(tm) = &self.text_match {
                inner.push_str(&tm.to_xml());
            }
            if let Some(tr) = &self.time_range {
                inner.push_str(&tr.to_xml());
            }
        }
        for pf in &self.param_filters {
            inner.push_str(&pf.to_xml());
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
