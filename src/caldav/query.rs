//! Calendar Query Builder for constructing CalDAV queries fluently.
//!
//! This module provides a fluent API for building and executing CalDAV calendar queries
//! without having to manually construct XML bodies.

//! Calendar Query Builder for constructing CalDAV queries fluently.
//!
//! This module provides a fluent API for building and executing CalDAV calendar queries
//! without having to manually construct XML bodies.
//!
//! *Note: This module is only available when the `query-builder` feature is enabled.*
//!
//! # Common Use Cases
//!
//! ## Get today's events
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
//! # #[cfg(feature = "query-builder")]
//! # async fn example(client: CalDavClient) -> Result<()> {
//! // Assuming today is 2024-01-15
//! let todays_events = client
//!     .query("my-calendar/")
//!     .component("VEVENT")
//!     .timerange("20240115T000000Z", "20240115T235959Z")
//!     .include_data()
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Get upcoming tasks
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
//! # #[cfg(feature = "query-builder")]
//! # async fn example(client: CalDavClient) -> Result<()> {
//! let upcoming_tasks = client
//!     .query("my-tasks/")
//!     .component("VTODO")
//!     .timerange("20240115T000000Z", "20240131T235959Z")
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Get all events in a month (without data for efficiency)
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
//! # #[cfg(feature = "query-builder")]
//! # async fn example(client: CalDavClient) -> Result<()> {
//! let monthly_events = client
//!     .query("my-calendar/")
//!     .component("VEVENT")
//!     .timerange("20240101T000000Z", "20240131T235959Z")
//!     // Note: not calling include_data() to only get metadata
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Get events with pagination
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
//! # #[cfg(feature = "query-builder")]
//! # async fn example(client: CalDavClient) -> Result<()> {
//! let paginated_events = client
//!     .query("my-calendar/")
//!     .component("VEVENT")
//!     .timerange("20240101T000000Z", "20241231T235959Z")
//!     .limit(50)  // Only get first 50 events
//!     .include_data()
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```

use crate::caldav::{CalDavClient, CalendarObject};
use crate::caldav::client::escape_xml;
use anyhow::Result;

/// A builder for constructing CalDAV calendar queries fluently.
///
/// # Examples
///
/// ```no_run
/// # use fast_dav_rs::{CalDavClient, Depth};
/// # use anyhow::Result;
/// #
/// # async fn example(client: CalDavClient) -> Result<()> {
/// let events = client
///     .query("/calendars/user/main/")
///     .component("VEVENT")
///     .timerange("20240101T000000Z", "20240201T000000Z")
///     .include_data()
///     .limit(100)
///     .execute()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct CalendarQueryBuilder {
    client: CalDavClient,
    calendar_path: String,
    component: String,
    start_time: Option<String>,
    end_time: Option<String>,
    include_data: bool,
    limit: Option<u32>,
}

impl CalendarQueryBuilder {
    /// Create a new CalendarQueryBuilder instance.
    ///
    /// # Arguments
    ///
    /// * `client` - The CalDavClient to use for executing the query
    /// * `calendar_path` - The path to the calendar collection to query
    pub fn new(client: CalDavClient, calendar_path: &str) -> Self {
        Self {
            client,
            calendar_path: calendar_path.to_string(),
            component: "VEVENT".to_string(),
            start_time: None,
            end_time: None,
            include_data: false,
            limit: None,
        }
    }

    /// Set the component type to query for.
    ///
    /// # Arguments
    ///
    /// * `component` - The component type (e.g., "VEVENT", "VTODO")
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use fast_dav_rs::CalDavClient;
    /// # let client = CalDavClient::new("https://example.com/", None, None).unwrap();
    /// let query = client.query("/calendars/user/tasks/").component("VTODO");
    /// ```
    pub fn component(mut self, component: &str) -> Self {
        self.component = component.to_string();
        self
    }

    /// Set a time range filter for the query.
    ///
    /// # Arguments
    ///
    /// * `start` - Start time in ISO format (e.g., "20240101T000000Z")
    /// * `end` - End time in ISO format (e.g., "20240201T000000Z")
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use fast_dav_rs::CalDavClient;
    /// # let client = CalDavClient::new("https://example.com/", None, None).unwrap();
    /// let query = client
    ///     .query("/calendars/user/main/")
    ///     .timerange("20240101T000000Z", "20240201T000000Z");
    /// ```
    pub fn timerange(mut self, start: &str, end: &str) -> Self {
        self.start_time = Some(start.to_string());
        self.end_time = Some(end.to_string());
        self
    }

    /// Include calendar data in the response.
    ///
    /// By default, only metadata like ETags are returned. Call this method
    /// to also retrieve the actual iCalendar data for each item.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use fast_dav_rs::CalDavClient;
    /// # let client = CalDavClient::new("https://example.com/", None, None).unwrap();
    /// let query = client.query("/calendars/user/main/").include_data();
    /// ```
    pub fn include_data(mut self) -> Self {
        self.include_data = true;
        self
    }

    /// Limit the number of results returned.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of results to return
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use fast_dav_rs::CalDavClient;
    /// # let client = CalDavClient::new("https://example.com/", None, None).unwrap();
    /// let query = client.query("/calendars/user/main/").limit(50);
    /// ```
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Execute the query and return the results.
    ///
    /// # Returns
    ///
    /// A vector of CalendarObject instances matching the query criteria.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails to execute or if the server returns an error response.
    pub async fn execute(self) -> Result<Vec<CalendarObject>> {
        use anyhow::anyhow;
        
        // Build the calendar query XML body
        let xml_body = self.build_calendar_query_body();
        
        // Execute the query using the existing report method with our custom XML
        let response = self.client.report(
            &self.calendar_path,
            crate::caldav::types::Depth::One,
            &xml_body,
        ).await?;
        
        if !response.status().is_success() {
            return Err(anyhow!(
                "Calendar query failed with status: {}",
                response.status()
            ));
        }
        
        let body = response.into_body();
        Ok(crate::caldav::client::map_calendar_objects(
            crate::caldav::streaming::parse_multistatus_bytes(&body)?
        ))
    }

    /// Build the XML body for a calendar-query REPORT request.
    fn build_calendar_query_body(&self) -> String {
        let mut prop = String::from("<D:prop><D:getetag/>");
        if self.include_data {
            prop.push_str("<C:calendar-data/>");
        }
        prop.push_str("</D:prop>");

        let mut filter = format!(
            "<C:filter>\
               <C:comp-filter name=\"VCALENDAR\">\
                 <C:comp-filter name=\"{}\">",
            escape_xml(&self.component)
        );
        
        if self.start_time.is_some() || self.end_time.is_some() {
            filter.push_str("<C:time-range");
            if let Some(start) = &self.start_time {
                filter.push_str(&format!(" start=\"{}\"", escape_xml(start)));
            }
            if let Some(end) = &self.end_time {
                filter.push_str(&format!(" end=\"{}\"", escape_xml(end)));
            }
            filter.push_str("/>");
        }
        filter.push_str("</C:comp-filter></C:comp-filter></C:filter>");

        format!(
            r#"<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">{prop}{filter}</C:calendar-query>"#
        )
    }
}