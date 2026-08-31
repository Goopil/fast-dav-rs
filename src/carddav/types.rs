use crate::carddav::client::escape_xml;

pub use crate::webdav::types::{BatchItem, DavItem, Depth};
/// Collation algorithm for `text-match` comparisons (RFC 6352 §7.3).
///
/// Determines how string comparisons are performed in addressbook-query
/// filters. See RFC 4790 for the collation registry.
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

/// Match type for `text-match` comparisons (RFC 6352 §7.3).
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

/// Text-match condition for `prop-filter` and `param-filter` (RFC 6352 §7.3).
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

    /// Render this text-match as CardDAV XML (the `<C:text-match>` element).
    pub fn to_xml(&self) -> String {
        let mut attrs = String::new();
        attrs.push_str(&format!(" collation=\"{}\"", self.collation.as_str()));
        attrs.push_str(&format!(" match-type=\"{}\"", self.match_type.as_str()));
        if self.negate {
            attrs.push_str(" negate-condition=\"yes\"");
        }
        format!(
            "<C:text-match{attrs}>{}</C:text-match>",
            escape_xml(&self.value)
        )
    }
}

/// Nested parameter filter inside a `prop-filter` (RFC 6352 §7.2).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ParamFilter {
    /// The vCard parameter name to filter on (e.g. `TYPE`, `PREF`).
    pub name: String,
    /// Optional text-match condition. If `None` and `is_not_defined` is
    /// `false`, the filter tests for the parameter's existence.
    pub text_match: Option<TextMatch>,
    /// If `true`, matches cards where this parameter is **absent**.
    pub is_not_defined: bool,
}

impl ParamFilter {
    /// Create a `ParamFilter` that matches the parameter's existence with a text-match.
    pub fn new(name: impl Into<String>, text_match: TextMatch) -> Self {
        Self {
            name: name.into(),
            text_match: Some(text_match),
            is_not_defined: false,
        }
    }

    /// Create a `ParamFilter` that matches cards where the parameter is **absent**.
    pub fn not_defined(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text_match: None,
            is_not_defined: true,
        }
    }

    /// Render this param-filter as CardDAV XML (the `<C:param-filter>` element).
    pub fn to_xml(&self) -> String {
        let mut inner = String::new();
        if self.is_not_defined {
            inner.push_str("<C:is-not-defined/>");
        } else if let Some(tm) = &self.text_match {
            inner.push_str(&tm.to_xml());
        }
        format!(
            "<C:param-filter name=\"{}\">{inner}</C:param-filter>",
            escape_xml(&self.name)
        )
    }
}

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
    /// Nested `param-filter` elements.
    pub param_filters: Vec<ParamFilter>,
    /// If `true`, matches cards where the property is **absent**.
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
        }
        for pf in &self.param_filters {
            prop_inner.push_str(&pf.to_xml());
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
#[derive(Debug, Clone)]
pub struct SyncResponse {
    pub sync_token: Option<String>,
    pub items: Vec<SyncItem>,
}
