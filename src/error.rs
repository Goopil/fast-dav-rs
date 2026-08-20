use hyper::StatusCode;
use std::time::Duration;

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
    InvalidUrl {
        /// The URL value that failed validation.
        url: String,
        /// The URI parser error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A caller-provided value failed validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),

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
    UnexpectedStatus {
        /// The operation that failed.
        operation: String,
        /// The status returned by the server.
        status: StatusCode,
    },

    /// An operation exceeded its configured time limit.
    #[error("operation timed out after {}s", limit.as_secs())]
    Timeout {
        /// The configured time limit.
        limit: Duration,
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

    /// A TLS, certificate, or PKI operation failed.
    ///
    /// Covers PEM parsing errors, rustls configuration failures, and
    /// native certificate store errors. The `context` string describes
    /// where or why the error occurred; the underlying cause is
    /// accessible via `source()`.
    #[error("TLS error: {context}")]
    Tls {
        /// Human-readable context describing the TLS failure.
        context: String,
        /// The underlying error, if any.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// An error returned by user-provided callback code or when wrapping an
    /// error that does not fit any other variant.
    ///
    /// The `context` string is used for `Display`; the underlying `source` is
    /// accessible only via [`std::error::Error::source`]. This intentionally
    /// avoids leaking the cause into the `Display` output, but consumers that
    /// print errors should walk the source chain to avoid losing information.
    #[error("{context}")]
    Other {
        /// Human-readable context describing where or why the error occurred.
        context: String,
        /// The underlying error, if any.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl Error {
    pub(crate) fn invalid_url(
        url: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::InvalidUrl {
            url: url.into(),
            source: Box::new(source),
        }
    }

    pub(crate) fn tls(
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
    /// Use [`Error::with_source`] when you have an underlying error to chain.
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            context: message.into(),
            source: None,
        }
    }

    /// Wrap an error with a context message and an underlying source.
    ///
    /// The context is used for `Display`; the source is returned by
    /// [`Error::source`] so the full error chain is preserved.
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
