//! Builder for [`CalDavClient`] — a thin wrapper over
//! [`WebDavClientBuilder`] with the same option set, plus the CalDAV-only
//! iCalendar validation level.
//!
//! See [`CalDavClient::builder`](crate::caldav::client::CalDavClient::builder)
//! for usage.

use crate::caldav::client::CalDavClient;
use crate::caldav::validation::ValidationLevel;
use crate::impl_dav_builder;

impl_dav_builder! {
    /// Builder for [`CalDavClient`].
    ///
    /// Created with [`CalDavClient::builder`]. Delegates every option to
    /// [`WebDavClientBuilder`]; only the base URL is required.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::CalDavClient;
    /// use fast_dav_rs::webdav::RequestCompressionMode;
    /// use std::time::Duration;
    ///
    /// let client = CalDavClient::builder("https://cal.example.com/dav/user01/")
    ///     .basic_auth("user01", "secret")
    ///     .timeout(Duration::from_secs(10))
    ///     .pool_max_idle_per_host(8)
    ///     .request_compression(RequestCompressionMode::Auto)
    ///     .build()?;
    /// # Ok::<(), fast_dav_rs::Error>(())
    /// ```
    pub struct CalDavClientBuilder;
    client = CalDavClient;
    extra = validation_level: ValidationLevel = ValidationLevel::Structural;
}

impl CalDavClientBuilder {
    /// Set how strictly iCalendar `PUT` bodies are validated client-side
    /// before they are sent. Default: [`ValidationLevel::Structural`].
    ///
    /// [`ValidationLevel::Structural`] rejects bodies that are not valid
    /// UTF-8, lack the `BEGIN:VCALENDAR`/`END:VCALENDAR` envelope, a
    /// `VERSION:2.0` or a `PRODID` property, or have unbalanced
    /// `BEGIN`/`END` pairs — **before any network I/O** — with
    /// [`Error::InvalidICalendar`](crate::Error::InvalidICalendar).
    /// [`ValidationLevel::Strict`] additionally requires a `UID` in every
    /// `VEVENT`/`VTODO`; [`ValidationLevel::None`] restores the
    /// pre-validation behavior. On a body that declares a `VERSION`, the
    /// wire `Content-Type` gains a matching `version` parameter.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::caldav::ValidationLevel;
    /// use fast_dav_rs::CalDavClient;
    ///
    /// let client = CalDavClient::builder("https://cal.example.com/dav/")
    ///     .validation_level(ValidationLevel::Strict)
    ///     .build()?;
    /// # Ok::<(), fast_dav_rs::Error>(())
    /// ```
    pub fn validation_level(mut self, level: ValidationLevel) -> Self {
        self.validation_level = level;
        self
    }
}
