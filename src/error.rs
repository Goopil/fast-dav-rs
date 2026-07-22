use hyper::StatusCode;
use std::fmt;
use std::time::Duration;

/// Error returned by `fast-dav-rs` operations.
#[derive(Debug, thiserror::Error)]
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
    #[error("operation timed out after {limit:?}")]
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

    /// XML text or an attribute value could not be decoded.
    #[error("XML decoding error: {0}")]
    XmlDecode(String),

    /// Parsing an XML attribute failed.
    #[error("XML attribute error: {0}")]
    XmlAttribute(#[from] quick_xml::events::attributes::AttrError),

    /// An I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Decoding UTF-8 text failed.
    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// An error returned by user-provided callback code.
    #[error("{0}")]
    Other(#[source] Box<dyn std::error::Error + Send + Sync>),
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

    pub(crate) fn from_client(source: hyper_util::client::legacy::Error) -> Self {
        if source.is_connect() {
            Self::Connection(source)
        } else {
            Self::Transport(source)
        }
    }

    /// Wrap an error message originating outside the DAV protocol stack.
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(Box::new(MessageError(message.into())))
    }
}

#[derive(Debug)]
struct MessageError(String);

impl fmt::Display for MessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MessageError {}

/// Result type used by all fallible `fast-dav-rs` APIs.
pub type Result<T> = std::result::Result<T, Error>;
