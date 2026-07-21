use anyhow::Result;

/// WebDAV Depth
#[derive(Copy, Clone)]
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

/// Common fields extracted from a WebDAV response.
#[derive(Debug, Clone, Default)]
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
}
