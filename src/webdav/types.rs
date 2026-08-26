use crate::Result;

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
