//! Builder for [`CalDavClient`] — a thin wrapper over
//! [`WebDavClientBuilder`] with the same option set.
//!
//! See [`CalDavClient::builder`](crate::caldav::client::CalDavClient::builder)
//! for usage.

use crate::caldav::client::CalDavClient;
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
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub struct CalDavClientBuilder;
    client = CalDavClient;
}
