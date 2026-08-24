//! Builder for [`CardDavClient`] — a thin wrapper over
//! [`WebDavClientBuilder`] with the same option set.
//!
//! See [`CardDavClient::builder`](crate::carddav::client::CardDavClient::builder)
//! for usage.

use crate::carddav::client::CardDavClient;
use crate::impl_dav_builder;

impl_dav_builder! {
    /// Builder for [`CardDavClient`].
    ///
    /// Created with [`CardDavClient::builder`]. Delegates every option to
    /// [`WebDavClientBuilder`]; only the base URL is required.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::CardDavClient;
    /// use fast_dav_rs::webdav::RequestCompressionMode;
    /// use std::time::Duration;
    ///
    /// let client = CardDavClient::builder("https://card.example.com/dav/user01/")
    ///     .basic_auth("user01", "secret")
    ///     .timeout(Duration::from_secs(10))
    ///     .pool_max_idle_per_host(8)
    ///     .request_compression(RequestCompressionMode::Auto)
    ///     .build()?;
    /// # Ok::<(), fast_dav_rs::Error>(())
    /// ```
    pub struct CardDavClientBuilder;
    client = CardDavClient;
}
