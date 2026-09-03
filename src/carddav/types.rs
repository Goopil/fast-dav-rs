use crate::carddav::client::escape_xml;

pub use crate::webdav::types::{
    BatchItem, Collation, DavItem, Depth, MatchType, ParamFilter, TextMatch,
};
/// A CardDAV addressbook-query filter (RFC 6352 §7).
///
/// Combines a `prop-filter` with optional nested `param-filter` elements
/// and an `is-not-defined` test. Use [`CardDavFilter::to_filter_xml`] to
/// produce the `<C:filter>` XML fragment for an `addressbook-query` REPORT.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CardDavFilter {
    /// The vCard property name to filter on (e.g. `EMAIL`, `FN`, `UID`).
    pub prop: String,
    /// The value to match (used for the `text-match` child).
    pub value: String,
    /// Collation algorithm for the `text-match` child.
    pub collation: Collation,
    /// Match type for the `text-match` child.
    pub match_type: MatchType,
    /// If `true`, the `text-match` condition is negated.
    pub negate: bool,
    /// Nested `param-filter` elements. Ignored (not serialized) when
    /// `is_not_defined` is set.
    pub param_filters: Vec<ParamFilter>,
    /// If `true`, matches cards where the property is **absent**. Excludes
    /// the `text-match` child and any `param-filter` children
    /// (RFC 6352 §10.5.1).
    pub is_not_defined: bool,
}

impl CardDavFilter {
    /// Create a new `CardDavFilter` for the given property and value with
    /// default collation (`i;unicode-casemap`) and match-type (`equals`).
    pub fn new(prop: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            prop: prop.into(),
            value: value.into(),
            collation: Collation::default(),
            match_type: MatchType::default(),
            negate: false,
            param_filters: vec![],
            is_not_defined: false,
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

    /// Negate the text-match condition.
    pub fn with_negate(mut self, negate: bool) -> Self {
        self.negate = negate;
        self
    }

    /// Add nested `param-filter` elements.
    pub fn with_param_filters(mut self, param_filters: Vec<ParamFilter>) -> Self {
        self.param_filters = param_filters;
        self
    }

    /// Set the `is-not-defined` flag (property absence test).
    pub fn with_is_not_defined(mut self, is_not_defined: bool) -> Self {
        self.is_not_defined = is_not_defined;
        self
    }

    /// Build the `<C:filter>` XML fragment for an `addressbook-query` REPORT.
    ///
    /// Per the RFC 6352 §10.5.1 DTD (`is-not-defined | (text-match?,
    /// param-filter*)`), when `is_not_defined` is set only
    /// `<C:is-not-defined/>` is emitted — the `text-match` child and the
    /// `param-filter` children are dropped.
    pub fn to_filter_xml(&self) -> String {
        let mut prop_inner = String::new();
        if self.is_not_defined {
            prop_inner.push_str("<C:is-not-defined/>");
        } else {
            let tm = TextMatch {
                value: self.value.clone(),
                collation: self.collation,
                match_type: self.match_type,
                negate: self.negate,
            };
            prop_inner.push_str(&tm.to_xml());
            for pf in &self.param_filters {
                prop_inner.push_str(&pf.to_xml());
            }
        }
        format!(
            "<C:filter><C:prop-filter name=\"{}\">{prop_inner}</C:prop-filter></C:filter>",
            escape_xml(&self.prop)
        )
    }
}

/// Summary of an addressbook (collection) returned by a `PROPFIND` depth=1.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AddressBookInfo {
    pub href: String,
    pub displayname: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub etag: Option<String>,
    pub sync_token: Option<String>,
    pub supported_address_data: Vec<String>,
}

/// Address object (vCard) returned by a `REPORT`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AddressObject {
    pub href: String,
    pub etag: Option<String>,
    pub address_data: Option<String>,
    pub status: Option<String>,
}

/// Detail of an item returned by `sync-collection`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SyncItem {
    pub href: String,
    pub etag: Option<String>,
    pub address_data: Option<String>,
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
/// [`map_sync_response`](crate::carddav::map_sync_response): response elements
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
