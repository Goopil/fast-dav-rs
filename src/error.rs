use crate::webdav::WebDavError;
use hyper::StatusCode;
use std::time::Duration;

/// Why an ETag was rejected by validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EtagReason {
    /// The ETag string was empty or only whitespace.
    Empty,
    /// The ETag has an invalid entity-tag format (e.g. unbalanced quotes).
    InvalidFormat,
    /// The ETag contains characters not allowed in entity tags.
    InvalidCharacters,
    /// The ETag cannot be used as an HTTP header value.
    InvalidHeaderValue,
    /// A weak entity-tag (`W/"…"`) was used where RFC 9110 requires strong
    /// comparison (`If-Match`): weak validators never match, so the
    /// conditional operation could never succeed.
    Weak,
}

impl std::fmt::Display for EtagReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Empty => "ETag cannot be empty",
            Self::InvalidFormat => "invalid entity-tag format",
            Self::InvalidCharacters => "contains invalid entity-tag characters",
            Self::InvalidHeaderValue => "cannot be used as an If-Match header value",
            Self::Weak => {
                "weak entity-tag not allowed for If-Match (RFC 9110 strong comparison: \
                 weak validators never match)"
            }
        };
        f.write_str(s)
    }
}

/// Why an iCalendar body was rejected by structural validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ICalendarViolation {
    /// The body is not valid UTF-8.
    NotUtf8,
    /// The body does not start with a `BEGIN:VCALENDAR` line.
    MissingBegin,
    /// The body does not end with an `END:VCALENDAR` line.
    MissingEnd,
    /// No `VERSION` property is present.
    MissingVersion,
    /// A `VERSION` property is present but its value is not `2.0`.
    UnsupportedVersion,
    /// No `PRODID` property is present (RFC 5545 §3.6 requires one).
    MissingProdId,
    /// A `BEGIN:x`/`END:x` pair is unbalanced or the names do not match.
    UnbalancedComponents,
    /// A `VEVENT` or `VTODO` component has no `UID` property
    /// (reported at [`ValidationLevel::Strict`](crate::caldav::ValidationLevel::Strict) only).
    MissingUid,
}

impl std::fmt::Display for ICalendarViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::NotUtf8 => "body is not valid UTF-8",
            Self::MissingBegin => "missing BEGIN:VCALENDAR at start",
            Self::MissingEnd => "missing END:VCALENDAR at end",
            Self::MissingVersion => "missing VERSION property",
            Self::UnsupportedVersion => "unsupported VERSION value (only 2.0 is supported)",
            Self::MissingProdId => "missing PRODID property",
            Self::UnbalancedComponents => "unbalanced BEGIN/END component pairs",
            Self::MissingUid => "VEVENT/VTODO component without a UID property",
        };
        f.write_str(s)
    }
}

/// Why an OAuth2 token refresh (RFC 6749 §6) failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenRefreshReason {
    /// The token endpoint answered with a non-success HTTP status.
    Rejected,
    /// The response body was not a parsable RFC 6749 §5.1 token response
    /// (invalid JSON, missing/empty `access_token`), or exceeded the 1 MiB
    /// body limit.
    MalformedResponse,
    /// The request to the token endpoint failed at the transport level or
    /// timed out.
    Transport,
}

impl std::fmt::Display for TokenRefreshReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Rejected => "token endpoint rejected the grant",
            Self::MalformedResponse => "token endpoint returned a malformed token response",
            Self::Transport => "token endpoint request failed",
        };
        f.write_str(s)
    }
}

/// Error returned by `fast-dav-rs` operations.
///
/// The enum is `#[non_exhaustive]` so that new variants can be added
/// without breaking downstream `match` expressions. Always include a
/// wildcard arm (`_ => …`) when matching.
///
/// [non_exhaustive]: https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A base URL or resolved request URI is invalid.
    #[error("invalid URL `{url}`: {source}")]
    #[non_exhaustive]
    InvalidUrl {
        /// The URL value that failed validation.
        url: String,
        /// The URI parser error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A caller-provided value failed validation.
    ///
    /// This is a **catch-all** variant for caller-side validation errors that
    /// don't fit a more specific variant. The library itself uses
    /// [`InvalidEtag`](Self::InvalidEtag), [`InvalidComponentName`](Self::InvalidComponentName),
    /// [`InvalidDateTime`](Self::InvalidDateTime), and [`InvalidConfig`](Self::InvalidConfig)
    /// for known validation cases. This variant is kept for external code
    /// that needs to return a validation error without a dedicated variant.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// An ETag value failed validation (empty, malformed, or contains
    /// invalid characters for use in an `If-Match` / `If-None-Match` header).
    #[error("invalid ETag: {reason}")]
    #[non_exhaustive]
    InvalidEtag {
        /// Why the ETag was rejected.
        reason: EtagReason,
        /// The underlying header parsing error, if applicable.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// A calendar or addressbook component name failed validation.
    #[error("{context}: invalid component name `{name}`: {reason}")]
    #[non_exhaustive]
    InvalidComponentName {
        /// Where the invalid component name was encountered (e.g. "invalid calendar-query component").
        context: String,
        /// The component name that was rejected.
        name: String,
        /// Why it was rejected.
        reason: &'static str,
        /// The invalid character that caused the rejection, if applicable.
        bad_char: Option<char>,
    },

    /// A date-time value did not match the expected iCalendar UTC format.
    #[error("{context}: invalid UTC date-time `{value}`: {reason}")]
    #[non_exhaustive]
    InvalidDateTime {
        /// Where the invalid date-time was encountered (e.g. "calendar-query start").
        context: String,
        /// The value that failed validation.
        value: String,
        /// Why it was rejected.
        reason: &'static str,
    },

    /// An iCalendar body failed structural validation (CalDAV `PUT`).
    ///
    /// Returned by `put`, `put_if_match`, and `put_if_none_match` on
    /// `CalDavClient` **before any network I/O** when the configured
    /// [`ValidationLevel`](crate::caldav::ValidationLevel) rejects the body.
    #[error("invalid iCalendar: {violation}")]
    #[non_exhaustive]
    InvalidICalendar {
        /// Which structural check failed.
        violation: ICalendarViolation,
    },

    /// An OAuth2 token refresh (RFC 6749 §6) failed. Tokens and client
    /// secrets never appear in this error.
    #[error("token refresh failed: {reason}")]
    #[non_exhaustive]
    TokenRefresh {
        /// Why the refresh failed.
        reason: TokenRefreshReason,
        /// HTTP status from the token endpoint when the failure was an HTTP
        /// error status. The response body is never included (it may echo
        /// tokens, RFC 6749 §10.4).
        status: Option<StatusCode>,
        /// The underlying transport error, when the failure happened before
        /// an HTTP response was received.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// A builder configuration value is invalid (timeout, pool size, auth, etc.).
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// An HTTP header value is invalid.
    #[error("invalid HTTP header value: {0}")]
    InvalidHeader(#[from] hyper::http::header::InvalidHeaderValue),

    /// An HTTP method is invalid.
    #[error("invalid HTTP method: {0}")]
    InvalidMethod(#[from] hyper::http::method::InvalidMethod),

    /// Building an HTTP request failed.
    #[error("HTTP request error: {0}")]
    Http(#[from] hyper::http::Error),

    /// A low-level Hyper connection or body operation failed.
    #[error("Hyper error: {0}")]
    Hyper(#[from] hyper::Error),

    /// Establishing the connection failed.
    #[error("connection error: {0}")]
    Connection(#[source] hyper_util::client::legacy::Error),

    /// Sending a request or receiving its body failed.
    #[error("transport error: {0}")]
    Transport(#[source] hyper_util::client::legacy::Error),

    /// A DAV operation returned an unexpected HTTP status.
    #[error("{operation} failed with {status}")]
    #[non_exhaustive]
    UnexpectedStatus {
        /// The operation that failed.
        operation: Operation,
        /// The status returned by the server.
        status: StatusCode,
    },

    /// A DAV operation returned an unexpected HTTP status with a `<D:error>`
    /// body identifying the failed precondition/postcondition
    /// (RFC 4918 §16, §14.12).
    ///
    /// Returned by the locking API (`LOCK`/`UNLOCK`) when the server reports
    /// e.g. `423 Locked` with `<D:no-conflicting-lock/>`. Bodies without a
    /// `<D:error>` element keep surfacing as [`Error::UnexpectedStatus`].
    #[error("{operation} failed with {status}: {dav}")]
    #[non_exhaustive]
    UnexpectedStatusWithDav {
        /// The operation that failed.
        operation: Operation,
        /// The status returned by the server.
        status: StatusCode,
        /// The parsed `<D:error>` body (RFC 4918 §14.12); inspect
        /// `precondition_code` for the failed precondition (e.g.
        /// `no-conflicting-lock`).
        dav: WebDavError,
    },

    /// Principal discovery returned `404 Not Found` even though
    /// authentication succeeded.
    ///
    /// This is a failure mode of its own, distinct from
    /// [`UnexpectedStatus`](Self::UnexpectedStatus): the server accepted the
    /// credentials (it never answered `401`) but has no current-user
    /// principal for this account. On some providers this is the signature
    /// of a **wrong username form** — e.g. authenticating with an email
    /// address where the provider expects an internal short account ID: the
    /// auth layer succeeds anyway and the `current-user-principal` PROPFIND
    /// then answers `404`. If you hit this error, retry discovery with the
    /// provider's canonical account identifier before assuming a server
    /// fault.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::{CalDavClient, Error};
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://dav.example.com/",
    ///     Some("me@example.com"),
    ///     Some("app-password"),
    /// )?;
    /// match client.discover_current_user_principal().await {
    ///     Err(Error::PrincipalNotFound { url, .. }) => {
    ///         eprintln!(
    ///             "auth OK but no principal at {url}: \
    ///              the username form is likely wrong for this provider"
    ///         );
    ///     }
    ///     other => {
    ///         other?;
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[error(
        "current-user-principal discovery returned 404 for `{url}`: authentication \
             succeeded but no principal exists at this URL — on some providers this means \
             the username form is wrong (e.g. email instead of the account ID)"
    )]
    #[non_exhaustive]
    PrincipalNotFound {
        /// The (credential-redacted) URL that answered `404`.
        url: String,
    },

    /// A request phase exceeded its configured time limit.
    ///
    /// Covers receiving response headers and reading/decompressing an aggregated
    /// body, each bounded by the limit. Stream *parsing* enforces its own 30 s
    /// idle timeout by default; raw `send_stream` body reads are the caller's
    /// responsibility.
    #[error("operation timed out after {}s", limit.as_secs())]
    #[non_exhaustive]
    Timeout {
        /// The configured time limit.
        limit: Duration,
    },

    /// A decompressed response body exceeded the configured size limit.
    #[error("decompressed body exceeds the {} MiB limit", limit / (1024 * 1024))]
    #[non_exhaustive]
    BodyTooLarge {
        /// The configured maximum size in bytes.
        limit: usize,
    },

    /// The HTTP redirect limit was exhausted while following redirects.
    ///
    /// Returned when the server kept redirecting (301/302/303/307/308) beyond
    /// the limit configured with `max_redirects` on the client builder.
    #[error("exceeded the maximum of {limit} redirects")]
    #[non_exhaustive]
    TooManyRedirects {
        /// The configured maximum number of redirects to follow.
        limit: u8,
    },

    /// Parsing or decoding XML failed.
    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),

    /// The XML element hierarchy is malformed or incomplete.
    #[error("XML structure error: {0}")]
    XmlStructure(String),

    /// Unescaping XML entity references failed.
    #[error("XML escape error: {0}")]
    XmlEscape(#[from] quick_xml::escape::EscapeError),

    /// Parsing an XML attribute failed.
    #[error("XML attribute error: {0}")]
    XmlAttribute(#[from] quick_xml::events::attributes::AttrError),

    /// An I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Decoding UTF-8 text failed.
    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// A rustls TLS operation failed.
    ///
    /// This variant is used when a `rustls::Error` is propagated via `?`
    /// (automatic `#[from]` conversion). For manually-wrapped TLS errors
    /// that carry additional context (e.g. PEM parsing failures), see
    /// [`Tls`](Self::Tls). Consumers checking for TLS errors should match
    /// both `TlsRustls(_)` and `Tls { .. }`.
    #[error("rustls error: {0}")]
    TlsRustls(#[from] rustls::Error),

    /// A TLS, certificate, or PKI operation failed.
    ///
    /// Covers PEM parsing errors, rustls configuration failures, and
    /// native certificate store errors. The `context` string describes
    /// where or why the error occurred; the underlying cause is
    /// accessible via `source()`.
    ///
    /// `source` is `Some` for most TLS errors — it wraps the underlying
    /// error from rustls, `rustls_pki_types`, or `rustls_native_certs`.
    /// `source` is `None` when the error has no underlying cause (e.g.
    /// a configuration error that is purely descriptive). The [`tls`](Self::tls)
    /// constructor always sets `source: Some`; `source: None` is only
    /// reachable via internal construction for edge cases.
    #[error("TLS error: {context}")]
    #[non_exhaustive]
    Tls {
        /// Human-readable context describing the TLS failure.
        context: String,
        /// The underlying error, if any. `Some` for most errors;
        /// `None` only when there is no deeper cause to chain.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// An error returned by user-provided callback code or when wrapping an
    /// error that does not fit any other variant.
    ///
    /// # When to use this variant
    ///
    /// This is an **escape-hatch** for cases that do not fit a specific
    /// variant — primarily errors from user-provided callbacks. If a new
    /// specific failure mode becomes common, prefer adding a dedicated
    /// variant over relying on `Other`.
    ///
    /// The `context` string is used for `Display`; the underlying `source` is
    /// accessible only via [`std::error::Error::source`]. This intentionally
    /// avoids leaking the cause into the `Display` output, but consumers that
    /// print errors should walk the source chain to avoid losing information.
    #[error("{context}")]
    #[non_exhaustive]
    Other {
        /// Human-readable context describing where or why the error occurred.
        context: String,
        /// The underlying error, if any.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

/// Identifies which DAV operation produced an [`Error::UnexpectedStatus`].
///
/// Using an enum instead of `String` avoids allocation in the error path
/// and lets callers match on the operation without string comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Operation {
    /// `PROPFIND` to discover the current-user-principal.
    PropfindCurrentUserPrincipal,
    /// `PROPFIND` to discover the calendar-home-set.
    PropfindCalendarHomeSet,
    /// `PROPFIND` to discover the addressbook-home-set.
    PropfindAddressbookHomeSet,
    /// `PROPFIND` to list calendars or addressbooks.
    PropfindCollections,
    /// `PROPFIND` to read a calendar's `calendar-timezone` (RFC 4791 §5.2.2).
    PropfindCalendarTimezone,
    /// `REPORT` calendar-query.
    ReportCalendarQuery,
    /// `REPORT` calendar-multiget.
    ReportCalendarMultiget,
    /// `REPORT` free-busy-query.
    ReportFreeBusyQuery,
    /// `REPORT` addressbook-query.
    ReportAddressbookQuery,
    /// `REPORT` addressbook-multiget.
    ReportAddressbookMultiget,
    /// `REPORT` sync-collection.
    ReportSyncCollection,
    /// `LOCK` to acquire or refresh a WebDAV lock (RFC 4918 §9.10).
    Lock,
    /// `UNLOCK` to remove a WebDAV lock (RFC 4918 §9.11).
    Unlock,
    /// `PROPFIND` against `/.well-known/caldav` (RFC 6764 §5 service discovery).
    DiscoverWellKnownCaldav,
    /// `PROPFIND` against `/.well-known/carddav` (RFC 6764 §5 service discovery).
    DiscoverWellKnownCarddav,
    /// `PROPFIND` to discover `schedule-inbox-URL`/`schedule-outbox-URL`/
    /// `calendar-user-address-set` (RFC 6638 §2.1.1, §2.2.1, §2.4.1).
    PropfindScheduleEndpoints,
    /// `POST` of an iTIP message (e.g. a free-busy request) to a scheduling
    /// outbox collection (RFC 6638 §5).
    PostSchedule,
    /// `PROPFIND` to list the contents of a schedule inbox (RFC 6638 §2.2).
    ScheduleInbox,
    /// `POST` of an attachment body to a calendar collection with
    /// `?action=attachment-add` (managed attachments; CalendarServer wire
    /// form of RFC 8607).
    PostManagedAttachment,
    /// `PROPFIND` to read `current-user-privilege-set` (RFC 3744 §5.4).
    PropfindCurrentUserPrivilegeSet,
    /// `PROPPATCH` to set/remove a calendar's `calendar-timezone`
    /// (RFC 4791 §5.2.2).
    ProppatchCalendarTimezone,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::PropfindCurrentUserPrincipal => "PROPFIND current-user-principal",
            Self::PropfindCalendarHomeSet => "PROPFIND calendar-home-set",
            Self::PropfindAddressbookHomeSet => "PROPFIND addressbook-home-set",
            Self::PropfindCollections => "PROPFIND collections",
            Self::PropfindCalendarTimezone => "PROPFIND calendar-timezone",
            Self::ReportCalendarQuery => "REPORT calendar-query",
            Self::ReportCalendarMultiget => "REPORT calendar-multiget",
            Self::ReportFreeBusyQuery => "REPORT free-busy-query",
            Self::ReportAddressbookQuery => "REPORT addressbook-query",
            Self::ReportAddressbookMultiget => "REPORT addressbook-multiget",
            Self::ReportSyncCollection => "REPORT sync-collection",
            Self::Lock => "LOCK",
            Self::Unlock => "UNLOCK",
            Self::DiscoverWellKnownCaldav => "PROPFIND .well-known/caldav",
            Self::DiscoverWellKnownCarddav => "PROPFIND .well-known/carddav",
            Self::PropfindScheduleEndpoints => "PROPFIND schedule endpoints",
            Self::PostSchedule => "POST scheduling outbox",
            Self::ScheduleInbox => "PROPFIND schedule inbox",
            Self::PostManagedAttachment => "POST managed attachment",
            Self::PropfindCurrentUserPrivilegeSet => "PROPFIND current-user-privilege-set",
            Self::ProppatchCalendarTimezone => "PROPPATCH calendar-timezone",
        };
        f.write_str(s)
    }
}

impl Error {
    pub(crate) fn invalid_url(
        url: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        // Redact userinfo before storing: the URL may come from a caller or a
        // remote server and must never be echoed with embedded credentials.
        Self::InvalidUrl {
            url: crate::common::redact_userinfo(url.into()),
            source: Box::new(source),
        }
    }

    /// Create an [`UnexpectedStatus`](Self::UnexpectedStatus) error.
    ///
    /// This is the public constructor for the `UnexpectedStatus` variant,
    /// which is `#[non_exhaustive]` and therefore cannot be constructed with
    /// a struct expression outside this crate.
    pub fn unexpected_status(operation: Operation, status: StatusCode) -> Self {
        Self::UnexpectedStatus { operation, status }
    }

    /// Create a [`Timeout`](Self::Timeout) error.
    ///
    /// This is the public constructor for the `Timeout` variant, which is
    /// `#[non_exhaustive]` and therefore cannot be constructed with a struct
    /// expression outside this crate.
    pub fn timeout(limit: Duration) -> Self {
        Self::Timeout { limit }
    }

    /// Create an [`InvalidEtag`](Self::InvalidEtag) error.
    ///
    /// This is the public constructor for the `InvalidEtag` variant, which is
    /// `#[non_exhaustive]` and therefore cannot be constructed with a struct
    /// expression outside this crate.
    pub fn invalid_etag(reason: EtagReason) -> Self {
        Self::InvalidEtag {
            reason,
            source: None,
        }
    }

    /// Create an [`InvalidEtag`](Self::InvalidEtag) error with an underlying source.
    ///
    /// This is the public constructor for the `InvalidEtag` variant, which is
    /// `#[non_exhaustive]` and therefore cannot be constructed with a struct
    /// expression outside this crate. Use this overload when the ETag
    /// validation failure wraps an underlying parsing error (e.g. an
    /// `InvalidHeaderValue`).
    pub fn invalid_etag_with_source(
        reason: EtagReason,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::InvalidEtag {
            reason,
            source: Some(Box::new(source)),
        }
    }

    /// Create an [`InvalidComponentName`](Self::InvalidComponentName) error.
    ///
    /// This is the public constructor for the `InvalidComponentName` variant,
    /// which is `#[non_exhaustive]` and therefore cannot be constructed with a
    /// struct expression outside this crate.
    pub fn invalid_component_name(
        context: impl Into<String>,
        name: impl Into<String>,
        reason: &'static str,
    ) -> Self {
        Self::InvalidComponentName {
            context: context.into(),
            name: name.into(),
            reason,
            bad_char: None,
        }
    }

    /// Create an [`InvalidComponentName`](Self::InvalidComponentName) error
    /// with a specific invalid character.
    ///
    /// This is the public constructor for the `InvalidComponentName` variant,
    /// which is `#[non_exhaustive]` and therefore cannot be constructed with a
    /// struct expression outside this crate. Use this overload when the
    /// rejection is due to a specific invalid character.
    pub fn invalid_component_name_with_char(
        context: impl Into<String>,
        name: impl Into<String>,
        reason: &'static str,
        bad_char: char,
    ) -> Self {
        Self::InvalidComponentName {
            context: context.into(),
            name: name.into(),
            reason,
            bad_char: Some(bad_char),
        }
    }

    /// Create an [`InvalidDateTime`](Self::InvalidDateTime) error.
    ///
    /// This is the public constructor for the `InvalidDateTime` variant, which
    /// is `#[non_exhaustive]` and therefore cannot be constructed with a struct
    /// expression outside this crate.
    pub fn invalid_datetime(
        context: impl Into<String>,
        value: impl Into<String>,
        reason: &'static str,
    ) -> Self {
        Self::InvalidDateTime {
            context: context.into(),
            value: value.into(),
            reason,
        }
    }

    /// Create a [`PrincipalNotFound`](Self::PrincipalNotFound) error.
    ///
    /// This is the public constructor for the `PrincipalNotFound` variant,
    /// which is `#[non_exhaustive]` and therefore cannot be constructed with
    /// a struct expression outside this crate. The URL is credential-redacted
    /// before being stored, like every URL carried by an [`Error`] variant.
    pub fn principal_not_found(url: impl std::fmt::Display) -> Self {
        Self::PrincipalNotFound {
            url: crate::common::redact_userinfo(url.to_string()),
        }
    }

    /// Create a [`Tls`](Self::Tls) error with the given context and source.
    ///
    /// This is the public constructor for the `Tls` variant, which is
    /// `#[non_exhaustive]` and therefore cannot be constructed with a struct
    /// expression outside this crate.
    pub fn tls(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Tls {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Classify a `hyper_util` client error as [`Connection`](Self::Connection)
    /// or [`Transport`](Self::Transport).
    ///
    /// Only `ErrorKind::Connect` is mapped to `Connection`. All other kinds —
    /// including `SendRequest`, which *may* indicate the request never reached
    /// the server — are mapped to `Transport`. Consumers that need to
    /// distinguish "request possibly not sent" from "response stream broken"
    /// should inspect the `hyper_util` error via `source()` rather than
    /// relying solely on the variant.
    pub(crate) fn from_client(source: hyper_util::client::legacy::Error) -> Self {
        if source.is_connect() {
            Self::Connection(source)
        } else {
            Self::Transport(source)
        }
    }

    /// Wrap an error message originating outside the DAV protocol stack.
    ///
    /// This is an **escape-hatch** for errors that do not fit a specific
    /// [`Error`] variant. Prefer a dedicated variant when one exists.
    ///
    /// Use [`Error::with_source`] when you have an underlying error to chain.
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            context: message.into(),
            source: None,
        }
    }

    /// Wrap an error with a context message and an underlying source.
    ///
    /// This is an **escape-hatch** for errors that do not fit a specific
    /// [`Error`] variant. Prefer a dedicated variant when one exists.
    ///
    /// The context is used for `Display`; the source is returned by
    /// [`std::error::Error::source`] so the full error chain is preserved.
    pub fn with_source(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Other {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Convert a `quick_xml::Error` into the most specific `Error` variant.
    ///
    /// `Syntax` and `IllFormed` errors are mapped to [`XmlStructure`](Self::XmlStructure)
    /// because they indicate a structurally invalid XML document (mismatched tags,
    /// unclosed elements, …). The `quick_xml` message is stringified because
    /// `Syntax` and `IllFormed` carry `SyntaxError`/`IllFormedError` (which
    /// implement `Display` but have no `source()` chain) — there is no deeper
    /// source chain to preserve. All other variants (`Io`, `Encoding`,
    /// `Escape`, `InvalidAttr`, `Namespace`) are mapped to [`Xml`](Self::Xml) via the
    /// blanket `#[from]` conversion, which preserves the full error chain.
    pub(crate) fn from_quick_xml(error: quick_xml::Error) -> Self {
        match error {
            quick_xml::Error::Syntax(s) => Self::XmlStructure(s.to_string()),
            quick_xml::Error::IllFormed(s) => Self::XmlStructure(s.to_string()),
            other => Self::Xml(other),
        }
    }
}

/// Result type used by all fallible `fast-dav-rs` APIs.
pub type Result<T> = std::result::Result<T, Error>;
