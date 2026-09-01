use crate::Result;
use crate::webdav::xml;

/// Outcome of the WebDAV-Sync (RFC 6578) support probe.
///
/// Distinguishes a server that does not implement the `sync-collection`
/// report from one the client could not reach — a network failure no longer
/// masquerades as "unsupported".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyncCapability {
    /// The server advertised or accepted the `sync-collection` report.
    Supported,
    /// The server answered and does not support the `sync-collection` report.
    Unsupported,
    /// Network error — the probe could not determine support.
    Unknown,
}

/// Scope of a `sync-collection` REPORT (RFC 6578 §3.3).
///
/// `One` restricts the sync to the members of the collection itself;
/// `Infinite` includes the collection and all its descendants (only
/// honored by servers that advertise infinite-depth sync support).
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::SyncLevel;
///
/// assert_eq!(SyncLevel::One.as_str(), "1");
/// assert_eq!(SyncLevel::Infinite.as_str(), "infinite");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyncLevel {
    /// `<D:sync-level>1</D:sync-level>` — sync the collection members only.
    One,
    /// `<D:sync-level>infinite</D:sync-level>` — sync the collection and
    /// all its descendants.
    Infinite,
}

impl SyncLevel {
    /// Returns the `<D:sync-level>` element value for this level.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Infinite => "infinite",
        }
    }
}

/// HTTP `Prefer` header preference (RFC 7240) supported by this client.
///
/// v1.0 supports the `return` preference only. Other preferences (`wait`,
/// `handling`, …) can still be sent manually through the `HeaderMap`
/// accepted by the low-level `send`/`send_stream` methods.
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::Prefer;
///
/// assert_eq!(Prefer::Minimal.as_str(), "return=minimal");
/// assert_eq!(Prefer::Representation.as_str(), "return=representation");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Prefer {
    /// `return=minimal` — the server should keep the response body minimal.
    Minimal,
    /// `return=representation` — the server should return the full resource
    /// representation (e.g. the stored body and new `ETag` of a `PUT`).
    Representation,
}

impl Prefer {
    /// Returns the `Prefer` header value for this preference.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Minimal => "return=minimal",
            Self::Representation => "return=representation",
        }
    }
}

/// Collation algorithm for `text-match` comparisons (RFC 4791 §8.4 / RFC 6352 §7.3).
///
/// Determines how string comparisons are performed in calendar-query and
/// addressbook-query filters. See RFC 4790 for the collation registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Collation {
    /// `i;unicode-casemap` — case-insensitive, Unicode-aware (default).
    #[default]
    UnicodeCasemap,
    /// `i;ascii-casemap` — case-insensitive, ASCII only.
    AsciiCasemap,
}

impl Collation {
    /// Returns the collation identifier string used in XML attributes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnicodeCasemap => "i;unicode-casemap",
            Self::AsciiCasemap => "i;ascii-casemap",
        }
    }
}

/// Match type for `text-match` comparisons (RFC 4791 §8.4 / RFC 6352 §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MatchType {
    /// Exact match (default).
    #[default]
    Equals,
    /// Substring match.
    Contains,
    /// Prefix match.
    StartsWith,
    /// Suffix match.
    EndsWith,
}

impl MatchType {
    /// Returns the match-type identifier string used in XML attributes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::Contains => "contains",
            Self::StartsWith => "starts-with",
            Self::EndsWith => "ends-with",
        }
    }
}

/// Text-match condition for `prop-filter` and `param-filter`
/// (RFC 4791 §8.4 / RFC 6352 §7.3).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TextMatch {
    /// The value to match against.
    pub value: String,
    /// Collation algorithm for comparison.
    pub collation: Collation,
    /// Match type for comparison.
    pub match_type: MatchType,
    /// If `true`, the match condition is negated (`negate-condition="yes"`).
    pub negate: bool,
}

impl TextMatch {
    /// Create a new `TextMatch` with the given value and default collation/match-type.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            collation: Collation::default(),
            match_type: MatchType::default(),
            negate: false,
        }
    }

    /// Set the collation algorithm.
    pub fn with_collation(mut self, collation: Collation) -> Self {
        self.collation = collation;
        self
    }

    /// Set the match type.
    pub fn with_match_type(mut self, match_type: MatchType) -> Self {
        self.match_type = match_type;
        self
    }

    /// Negate the match condition.
    pub fn with_negate(mut self, negate: bool) -> Self {
        self.negate = negate;
        self
    }

    /// Render this text-match as the `<C:text-match>` element.
    pub fn to_xml(&self) -> String {
        xml::text_match_xml(
            &self.value,
            self.collation.as_str(),
            self.match_type.as_str(),
            self.negate,
        )
    }
}

/// Nested parameter filter inside a `prop-filter` (RFC 4791 §8.3 / RFC 6352 §7.2).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ParamFilter {
    /// The property parameter name to filter on (e.g. `PARTSTAT`, `TYPE`).
    pub name: String,
    /// Optional text-match condition. If `None` and `is_not_defined` is
    /// `false`, the filter tests for the parameter's existence.
    pub text_match: Option<TextMatch>,
    /// If `true`, matches resources where this parameter is **absent**.
    pub is_not_defined: bool,
}

impl ParamFilter {
    /// Create a `ParamFilter` that matches the parameter with a text-match.
    pub fn new(name: impl Into<String>, text_match: TextMatch) -> Self {
        Self {
            name: name.into(),
            text_match: Some(text_match),
            is_not_defined: false,
        }
    }

    /// Create a `ParamFilter` that matches resources where the parameter is **absent**.
    pub fn not_defined(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text_match: None,
            is_not_defined: true,
        }
    }

    /// Render this param-filter as the `<C:param-filter>` element.
    pub fn to_xml(&self) -> String {
        let inner = if self.is_not_defined {
            xml::IS_NOT_DEFINED_XML.to_string()
        } else if let Some(tm) = &self.text_match {
            tm.to_xml()
        } else {
            String::new()
        };
        xml::param_filter_xml(&self.name, &inner)
    }
}

/// WebDAV Depth
#[derive(Copy, Clone)]
#[non_exhaustive]
pub enum Depth {
    Zero,
    One,
    Infinity,
}
impl Depth {
    pub fn as_str(self) -> &'static str {
        match self {
            Depth::Zero => "0",
            Depth::One => "1",
            Depth::Infinity => "infinity",
        }
    }
}

/// Annotated result of a batch operation
#[non_exhaustive]
pub struct BatchItem<T> {
    pub pub_path: String,
    pub result: Result<T>,
}

/// Extract the numeric HTTP status code from a WebDAV `<D:status>` value.
///
/// Splits on ASCII whitespace and returns the first token that parses as a
/// `u16` within the valid HTTP status range (`100..=599`). This handles both
/// full status lines (`"HTTP/1.1 404 Not Found"`) and bare codes (`"404"`),
/// while rejecting look-alikes such as `"HTTP/1.1 4040 Custom"`.
pub(crate) fn http_status_code(status_line: &str) -> Option<u16> {
    status_line.split_ascii_whitespace().find_map(|token| {
        token
            .parse::<u16>()
            .ok()
            .filter(|code| (100..=599).contains(code))
    })
}

/// DAV compliance classes and extensions advertised by a server via the `DAV`
/// response header (RFC 4918 §10.1).
///
/// The `DAV` header is a comma-separated list of compliance class tokens
/// (`1`, `2`, `3`) and optional extension tokens (e.g. `calendar-access`,
/// `addressbook`). See [`parse_dav_header`] for parsing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DavCapabilities {
    /// Class 1 compliance (RFC 4918 baseline WebDAV).
    pub class1: bool,
    /// Class 2 compliance (locking).
    pub class2: bool,
    /// Class 3 compliance (extended WebDAV features).
    pub class3: bool,
    /// Extension tokens advertised by the server (e.g. `calendar-access`,
    /// `addressbook`, `version-control`).
    pub extensions: Vec<String>,
}

/// A single `<D:propstat>` group within a `<D:response>` (RFC 4918 §13.1).
///
/// A response may contain multiple propstat groups, each carrying its own
/// status — e.g. one group of properties returned with `200 OK` and another
/// with `404 Not Found`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PropStat {
    /// The status line for this group (e.g. `"HTTP/1.1 200 OK"`).
    pub status: Option<String>,
    /// Local names of the properties in this group (e.g. `displayname`,
    /// `getetag`).
    pub prop_names: Vec<String>,
}

/// Precondition/postcondition error body parsed from a `<D:error>` element
/// (RFC 4918 §14.12).
///
/// Server error responses (4xx/5xx) may include a `<D:error>` body whose
/// child element identifies the precondition or postcondition that failed
/// (e.g. `<C:no-uid-conflict/>`).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct WebDavError {
    /// The local name of the `<D:error>` child element identifying the
    /// failed precondition/postcondition (e.g. `no-uid-conflict`), without
    /// the XML namespace prefix. `None` when the `<D:error>` body has no
    /// child element or no error body was present.
    pub precondition_code: Option<String>,
}

/// Parse a `DAV` response header value (RFC 4918 §10.1) into
/// [`DavCapabilities`].
///
/// The header is a comma-separated list of compliance class tokens (`1`,
/// `2`, `3`) and optional extension tokens (e.g. `calendar-access`).
/// Whitespace around tokens is tolerated.
///
/// ```
/// use fast_dav_rs::webdav::{DavCapabilities, parse_dav_header};
///
/// let caps = parse_dav_header("1, 2, calendar-access").unwrap();
/// assert!(caps.class1 && caps.class2);
/// assert_eq!(caps.extensions, vec!["calendar-access".to_string()]);
/// ```
pub fn parse_dav_header(value: &str) -> Result<DavCapabilities> {
    let mut caps = DavCapabilities::default();
    for raw in value.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        match token {
            "1" => caps.class1 = true,
            "2" => caps.class2 = true,
            "3" => caps.class3 = true,
            other => caps.extensions.push(other.to_string()),
        }
    }
    Ok(caps)
}

/// A media type advertised by a calendar collection via `supported-calendar-data`
/// (RFC 4791 §5.2.6).
///
/// Parsed from the `content-type` and optional `version` attributes of the
/// `<C:calendar-data-type>` elements inside the property, e.g.
/// `<C:calendar-data-type content-type="text/calendar" version="2.0"/>`.
///
/// ```
/// use fast_dav_rs::caldav::MediaType;
///
/// let media = MediaType::new("text/calendar", Some("2.0"));
/// assert_eq!(media.content_type, "text/calendar");
/// assert_eq!(media.version.as_deref(), Some("2.0"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MediaType {
    /// The MIME content type (e.g. `text/calendar`).
    pub content_type: String,
    /// The optional version parameter (e.g. `Some("2.0")` for iCalendar 2.0).
    pub version: Option<String>,
}

impl MediaType {
    /// Create a `MediaType` with the given content type and optional version.
    pub fn new(content_type: impl Into<String>, version: Option<impl Into<String>>) -> Self {
        Self {
            content_type: content_type.into(),
            version: version.map(Into::into),
        }
    }
}

/// Item extracted from a WebDAV `207 Multi-Status` response.
///
/// Superset of the CalDAV and CardDAV item fields: only the properties the
/// server actually returned are populated (`is_calendar`/`calendar_data` for
/// CalDAV, `is_addressbook`/`address_data` for CardDAV).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DavItem {
    pub href: String,
    pub status: Option<String>,
    pub displayname: Option<String>,
    pub etag: Option<String>,
    pub is_collection: bool,
    /// `<D:resourcetype><D:collection/><C:calendar/></D:resourcetype>` seen (CalDAV).
    pub is_calendar: bool,
    /// `<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>` seen (CardDAV).
    pub is_addressbook: bool,
    pub supported_components: Vec<String>,
    pub supported_address_data: Vec<String>,
    /// `max-resource-size` in octets (RFC 4791 §5.2.3); `None` when absent or unparseable.
    pub max_resource_size: Option<u64>,
    /// Media types from `supported-calendar-data` (RFC 4791 §5.2.6).
    pub supported_calendar_data: Vec<MediaType>,
    /// `max-attendees-per-instance` (RFC 4791 §5.2.4); `None` when absent or unparseable.
    pub max_attendees_per_instance: Option<u32>,
    pub calendar_data: Option<String>,
    pub address_data: Option<String>,
    pub calendar_home_set: Vec<String>,
    pub addressbook_home_set: Vec<String>,
    pub current_user_principal: Vec<String>,
    pub owner: Option<String>,
    pub calendar_description: Option<String>,
    pub calendar_timezone: Option<String>,
    pub calendar_color: Option<String>,
    pub addressbook_description: Option<String>,
    pub addressbook_color: Option<String>,
    pub sync_token: Option<String>,
    pub content_type: Option<String>,
    pub last_modified: Option<String>,
    pub propstats: Vec<PropStat>,
    pub response_status: Option<String>,
}

impl Default for DavItem {
    fn default() -> Self {
        Self::new()
    }
}

impl DavItem {
    pub fn new() -> Self {
        Self {
            href: String::new(),
            status: None,
            displayname: None,
            etag: None,
            is_collection: false,
            is_calendar: false,
            is_addressbook: false,
            supported_components: Vec::new(),
            supported_address_data: Vec::new(),
            max_resource_size: None,
            supported_calendar_data: Vec::new(),
            max_attendees_per_instance: None,
            calendar_data: None,
            address_data: None,
            calendar_home_set: Vec::new(),
            addressbook_home_set: Vec::new(),
            current_user_principal: Vec::new(),
            owner: None,
            calendar_description: None,
            calendar_timezone: None,
            calendar_color: None,
            addressbook_description: None,
            addressbook_color: None,
            sync_token: None,
            content_type: None,
            last_modified: None,
            propstats: Vec::new(),
            response_status: None,
        }
    }

    pub(crate) fn apply_common(&mut self, common: DavItemCommon) {
        crate::apply_common_fields!(self, common);
    }
}

/// Common fields extracted from a WebDAV response.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DavItemCommon {
    pub href: String,
    pub status: Option<String>,
    pub displayname: Option<String>,
    pub etag: Option<String>,
    pub is_collection: bool,
    pub sync_token: Option<String>,
    pub current_user_principal: Vec<String>,
    pub owner: Option<String>,
    pub content_type: Option<String>,
    pub last_modified: Option<String>,
    /// Multiple `<D:propstat>` groups for this response (RFC 4918 §13.1).
    pub propstats: Vec<PropStat>,
    /// Response-level `<D:status>` if present (distinct from propstat
    /// status).
    pub response_status: Option<String>,
}

#[macro_export]
macro_rules! apply_common_fields {
    ($self:expr, $common:expr) => {
        $self.href = $common.href;
        $self.status = $common.status;
        $self.displayname = $common.displayname;
        $self.etag = $common.etag;
        $self.is_collection = $common.is_collection;
        $self.sync_token = $common.sync_token;
        $self.current_user_principal = $common.current_user_principal;
        $self.owner = $common.owner;
        $self.content_type = $common.content_type;
        $self.last_modified = $common.last_modified;
        $self.propstats = $common.propstats;
        $self.response_status = $common.response_status;
    };
}

use hyper::HeaderMap;

/// Flattened `sync-collection` item shared by the CalDAV/CardDAV mappers.
pub(crate) struct SyncRow {
    pub href: String,
    pub etag: Option<String>,
    pub data: Option<String>,
    pub status: Option<String>,
    pub is_deleted: bool,
}

/// Shared `sync-collection` mapping logic (RFC 6578): resolve the sync token
/// (top-level, then `Sync-Token` header, then first per-item token), skip
/// collection entries, and flag 404/410 items as deleted.
pub(crate) fn map_sync_rows(
    headers: &HeaderMap,
    items: Vec<DavItem>,
    top_level_sync_token: Option<String>,
    data_of: impl FnMut(&mut DavItem) -> Option<String>,
) -> (Option<String>, Vec<SyncRow>) {
    let mut data_of = data_of;
    let mut sync_token = top_level_sync_token.or_else(|| {
        headers
            .get("Sync-Token")
            .and_then(|v| v.to_str().ok())
            .map(crate::webdav::client::normalize_sync_token)
    });
    let mut out = Vec::new();

    for mut item in items {
        // Per-item tokens are already normalized by the streaming parser.
        if item.sync_token.is_some() && sync_token.is_none() {
            sync_token = item.sync_token.clone();
        }

        let is_collection = item.is_collection
            || (item.sync_token.is_some() && item.etag.is_none() && data_of(&mut item).is_none());
        if is_collection {
            continue;
        }
        let status = item.status.clone();
        let code = status.as_deref().and_then(http_status_code);
        let is_deleted = matches!(code, Some(404) | Some(410));

        let data = data_of(&mut item);
        out.push(SyncRow {
            href: item.href,
            etag: item.etag,
            data,
            status,
            is_deleted,
        });
    }

    (sync_token, out)
}
