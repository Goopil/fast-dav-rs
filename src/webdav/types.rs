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
