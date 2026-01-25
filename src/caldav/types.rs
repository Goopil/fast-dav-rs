use crate::webdav::types::DavItemCommon;
pub use crate::webdav::types::{BatchItem, Depth};

/// Item extracted from a WebDAV response
#[derive(Debug, Clone)]
pub struct DavItem {
    pub href: String,
    pub status: Option<String>,
    pub displayname: Option<String>,
    pub etag: Option<String>,
    pub is_collection: bool,
    pub is_calendar: bool,
    pub supported_components: Vec<String>,
    pub calendar_data: Option<String>,
    pub calendar_home_set: Vec<String>,
    pub current_user_principal: Vec<String>,
    pub owner: Option<String>,
    pub calendar_description: Option<String>,
    pub calendar_timezone: Option<String>,
    pub calendar_color: Option<String>,
    pub sync_token: Option<String>,
    pub content_type: Option<String>,
    pub last_modified: Option<String>,
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
            supported_components: Vec::new(),
            calendar_data: None,
            calendar_home_set: Vec::new(),
            current_user_principal: Vec::new(),
            owner: None,
            calendar_description: None,
            calendar_timezone: None,
            calendar_color: None,
            sync_token: None,
            content_type: None,
            last_modified: None,
        }
    }

    pub(crate) fn apply_common(&mut self, common: DavItemCommon) {
        self.href = common.href;
        self.status = common.status;
        self.displayname = common.displayname;
        self.etag = common.etag;
        self.is_collection = common.is_collection;
        self.sync_token = common.sync_token;
        self.current_user_principal = common.current_user_principal;
        self.owner = common.owner;
        self.content_type = common.content_type;
        self.last_modified = common.last_modified;
    }
}

/// Summary of a calendar (collection) returned by a `PROPFIND` depth=1.
#[derive(Debug, Clone)]
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
pub struct CalendarObject {
    pub href: String,
    pub etag: Option<String>,
    pub calendar_data: Option<String>,
    pub status: Option<String>,
}

/// Detail of an item returned by `sync-collection`.
#[derive(Debug, Clone)]
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
