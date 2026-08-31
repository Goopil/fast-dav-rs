use crate::webdav::xml;
use crate::Result;

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
