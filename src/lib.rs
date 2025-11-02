//! Fast CalDAV client library for Rust.
//!
//! This library provides a high-performance, asynchronous CalDAV client built on modern
//! Rust ecosystem components including hyper 1.x, rustls, and tokio.
//!
//! # Features
//!
//! - HTTP/2 multiplexing and connection pooling
//! - Automatic response decompression (br/zstd/gzip)
//! - Streaming-friendly APIs for large WebDAV responses
//! - Batch operations with bounded concurrency
//! - ETag helpers for safe conditional writes/deletes
//! - Streaming XML parsing with minimal memory footprint
//! - Fluent Query Builder for easy calendar queries *(optional feature)*
//!
//! # Examples
//!
//! ## Basic Setup and Calendar Discovery
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth};
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!
//!     // Discover the current user's principal
//!     let principal = client.discover_current_user_principal().await?
//!         .ok_or_else(|| anyhow::anyhow!("No principal found"))?;
//!     
//!     // Find calendar home sets
//!     let homes = client.discover_calendar_home_set(&principal).await?;
//!     let home = homes.first().ok_or_else(|| anyhow::anyhow!("No calendar home found"))?;
//!     
//!     // List all calendars
//!     let calendars = client.list_calendars(home).await?;
//!     for calendar in &calendars {
//!         println!("Found calendar: {:?}", calendar.displayname);
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Calendar Operations
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth};
//! use bytes::Bytes;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let home = ""; // Placeholder for calendar home path
//!     // Create a new calendar
//!     let calendar_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
//!     <C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//!       <D:set>
//!         <D:prop>
//!           <D:displayname>My New Calendar</D:displayname>
//!           <C:calendar-description>Calendar created with fast-dav-rs</C:calendar-description>
//!         </D:prop>
//!       </D:set>
//!     </C:mkcalendar>"#;
//!     
//!     let response = client.mkcalendar("my-new-calendar/", calendar_xml).await?;
//!     println!("Created calendar with status: {}", response.status());
//!     
//!     // Delete a calendar
//!     let delete_response = client.delete("my-new-calendar/").await?;
//!     println!("Deleted calendar with status: {}", delete_response.status());
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Event Operations
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth};
//! use bytes::Bytes;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let calendar_path = ""; // Placeholder for calendar path
//!     // Create a new event
//!     let event_ics = Bytes::from(r#"BEGIN:VCALENDAR
//! VERSION:2.0
//! PRODID:-//fast-dav-rs//EN
//! BEGIN:VEVENT
//! UID:event-123@example.com
//! DTSTAMP:20230101T000000Z
//! DTSTART:20231225T100000Z
//! DTEND:20231225T110000Z
//! SUMMARY:Christmas Day Event
//! DESCRIPTION:Celebrate Christmas with family
//! END:VEVENT
//! END:VCALENDAR
//! "#);
//!     
//!     // Safe creation with If-None-Match to prevent overwriting
//!     let response = client.put_if_none_match("my-calendar/christmas-event.ics", event_ics).await?;
//!     println!("Created event with status: {}", response.status());
//!     
//!     // Query events in a date range
//!     let events = client.calendar_query_timerange(
//!         "my-calendar/",
//!         "VEVENT",
//!         Some("20231201T000000Z"),  // Start date
//!         Some("20231231T235959Z"),  // End date
//!         true  // Include event data
//!     ).await?;
//!     
//!     for event in &events {
//!         println!("Event: {:?}, ETag: {:?}", event.href, event.etag);
//!     }
//!     
//!     // Update an existing event (if we found one)
//!     if let Some(first_event) = events.first() {
//!         if let Some(etag) = &first_event.etag {
//!             let updated_ics = Bytes::from(format!(r#"BEGIN:VCALENDAR
//! VERSION:2.0
//! PRODID:-//fast-dav-rs//EN
//! BEGIN:VEVENT
//! UID:event-123@example.com
//! DTSTAMP:20230102T000000Z
//! DTSTART:20231225T100000Z
//! DTEND:20231225T120000Z  // Extended end time
//! SUMMARY:Christmas Day Event (Extended)
//! DESCRIPTION:Celebrate Christmas with extended family
//! END:VEVENT
//! END:VCALENDAR
//! "#));
//!             
//!             // Safe update with If-Match
//!             let update_response = client.put_if_match(
//!                 &first_event.href,
//!                 updated_ics,
//!                 etag
//!             ).await?;
//!             println!("Updated event with status: {}", update_response.status());
//!         }
//!     }
//!     
//!     // Delete an event (using conditional delete for safety)
//!     if let Some(first_event) = events.first() {
//!         if let Some(etag) = &first_event.etag {
//!             let delete_response = client.delete_if_match(&first_event.href, etag).await?;
//!             println!("Deleted event with status: {}", delete_response.status());
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Fluent Query Builder
//!
//! For easier construction of calendar queries, use the fluent Query Builder API:
//!
//! *Note: This feature requires enabling the `query-builder` feature.*
//!
//! ```no_run
//! use fast_dav_rs::CalDavClient;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let calendar_path = ""; // Placeholder for calendar path
//!     // Query events with a fluent API
//!     let events = client
//!         .query("my-calendar/")
//!         .component("VEVENT")
//!         .timerange("20231201T000000Z", "20231231T235959Z")
//!         .include_data()
//!         .limit(100)
//!         .execute()
//!         .await?;
//!     
//!     for event in &events {
//!         println!("Event: {:?}, ETag: {:?}", event.href, event.etag);
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Common Query Builder Use Cases
//!
//! ### 1. Get today's events
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
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
//! ### 2. Get upcoming tasks
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
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
//! ### 3. Get all events in a month (without data for efficiency)
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
//! # async fn example(client: CalDavClient) -> Result<()> {
//! let monthly_events = client
//!     .query("my-calendar/")
//!     .component("VEVENT")
//!     .timerange("20240101T000000Z", "20240131T235959Z")
//!     // Note: not calling include_data() to only get metadata
//!     .execute()
//!     .await?;
//!     
//! // Later fetch specific events with full data when needed
//! for event in monthly_events {
//!     if event.href.contains("important-meeting") {
//!         let full_event = client.get(&event.href).await?;
//!         // Process the full iCalendar data
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### 4. Get events with pagination
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
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
//!
//! ### 5. Get all calendar entries without time filtering
//! ```no_run
//! # use fast_dav_rs::CalDavClient;
//! # use anyhow::Result;
//! # async fn example(client: CalDavClient) -> Result<()> {
//! let all_events = client
//!     .query("my-calendar/")
//!     .component("VEVENT")
//!     // No timerange specified - gets all events
//!     .include_data()
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Working with ETags for Safe Operations
//!
//! ```no_run
//! use fast_dav_rs::CalDavClient;
//! use bytes::Bytes;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let event_path = ""; // Placeholder
//!     // Get current ETag before modifying a resource
//!     let head_response = client.head("my-calendar/some-event.ics").await?;
//!     if let Some(etag) = CalDavClient::etag_from_headers(head_response.headers()) {
//!         // Now we can safely update with If-Match
//!         let updated_ics = Bytes::from(r#"BEGIN:VCALENDAR
//! VERSION:2.0
//! PRODID:-//fast-dav-rs//EN
//! BEGIN:VEVENT
//! UID:some-event@example.com
//! DTSTAMP:20230101T000000Z
//! DTSTART:20231225T100000Z
//! DTEND:20231225T110000Z
//! SUMMARY:Updated Event Title
//! END:VEVENT
//! END:VCALENDAR
//! "#);
//!         
//!         let response = client.put_if_match("my-calendar/some-event.ics", updated_ics, &etag).await?;
//!         if response.status().is_success() {
//!             println!("Successfully updated event");
//!         } else {
//!             println!("Failed to update event: {}", response.status());
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Streaming Large Responses
//!
//! For processing large collections without loading everything into memory:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth, parse_multistatus_stream, detect_encoding};
//! use bytes::Bytes;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let calendar_path = ""; // Placeholder
//!     // Stream a large PROPFIND response
//!     let propfind_xml = r#"
//!     <D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//!       <D:prop>
//!         <D:displayname/>
//!         <D:getetag/>
//!         <C:calendar-data/>
//!       </D:prop>
//!     </D:propfind>"#;
//!     
//!     let response = client.propfind_stream("large-calendar/", Depth::One, propfind_xml).await?;
//!     let encodings = detect_encoding(response.headers());
//!     let items = parse_multistatus_stream(response.into_body(), &encodings).await?;
//!     
//!     // Process items one by one without loading everything into memory
//!     for item in items {
//!         println!("Found item: {} with etag: {:?}",
//!                  item.displayname.unwrap_or_default(),
//!                  item.etag);
//!         
//!         // Process calendar data if present
//!         if let Some(data) = item.calendar_data {
//!             // Handle large iCalendar data efficiently
//!             println!("Processing calendar data of length: {}", data.len());
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Concurrent Batch Operations
//!
//! Execute multiple operations concurrently with controlled parallelism:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth};
//! use bytes::Bytes;
//! use std::sync::Arc;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let calendar_paths = vec!["".to_string()]; // Placeholder
//!     // Prepare a common PROPFIND request for multiple calendars
//!     let propfind_body = Arc::new(Bytes::from(r#"
//!     <D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//!       <D:prop>
//!         <D:displayname/>
//!         <D:getetag/>
//!         <C:supported-calendar-component-set/>
//!       </D:prop>
//!     </D:propfind>"#));
//!     
//!     // Execute PROPFIND on multiple calendars concurrently (max 5 in parallel)
//!     let results = client.propfind_many(
//!         calendar_paths,  // Vector of calendar paths
//!         Depth::Zero,
//!         propfind_body,
//!         5  // Maximum concurrency
//!     ).await;
//!     
//!     // Process results in the same order as input
//!     for result in results {
//!         match result.result {
//!             Ok(response) => {
//!                 if response.status().is_success() {
//!                     println!("Successfully queried: {}", result.pub_path);
//!                     // Parse response body as needed
//!                 } else {
//!                     println!("Query failed for {}: {}", result.pub_path, response.status());
//!                 }
//!             }
//!             Err(e) => {
//!                 println!("Error querying {}: {}", result.pub_path, e);
//!             }
//!         }
//!     }
//!     
//!     // Batch event updates
//!     let event_updates = vec![
//!         ("event1.ics", "BEGIN:VCALENDAR...END:VCALENDAR"),
//!         ("event2.ics", "BEGIN:VCALENDAR...END:VCALENDAR"),
//!     ];
//!     
//!     # let calendar_path = ""; // Placeholder
//!     // Upload multiple events concurrently
//!     let mut upload_tasks = Vec::new();
//!     for (filename, ical_data) in event_updates {
//!         let client_clone = client.clone();
//!         let path = format!("{}/{}", calendar_path, filename);
//!         let data = Bytes::from(ical_data);
//!         
//!         upload_tasks.push(tokio::spawn(async move {
//!             client_clone.put(&path, data).await
//!         }));
//!     }
//!     
//!     // Wait for all uploads to complete
//!     let upload_results = futures::future::join_all(upload_tasks).await;
//!     for (i, result) in upload_results.into_iter().enumerate() {
//!         match result {
//!             Ok(Ok(response)) => {
//!                 println!("Upload {} completed with status: {}", i, response.status());
//!             }
//!             Ok(Err(e)) => {
//!                 println!("Upload {} failed with error: {}", i, e);
//!             }
//!             Err(e) => {
//!                 println!("Upload {} panicked: {}", i, e);
//!             }
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Bootstrap and Capability Detection
//!
//! Discover server capabilities and choose appropriate synchronization methods:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth};
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     // Bootstrap: Discover server capabilities
//!     println!("Detecting server capabilities...");
//!     
//!     // Check if server supports WebDAV-Sync (RFC 6578)
//!     let has_sync_support = client.supports_webdav_sync().await?;
//!     println!("WebDAV-Sync support: {}", has_sync_support);
//!     
//!     // Discover user principal
//!     let principal = client.discover_current_user_principal().await?
//!         .ok_or_else(|| anyhow::anyhow!("No principal found"))?;
//!     println!("User principal: {}", principal);
//!     
//!     // Discover calendar homes
//!     let homes = client.discover_calendar_home_set(&principal).await?;
//!     let home = homes.first().ok_or_else(|| anyhow::anyhow!("No calendar home found"))?;
//!     println!("Calendar home: {}", home);
//!     
//!     // List calendars with detailed info
//!     let calendars = client.list_calendars(home).await?;
//!     for calendar in &calendars {
//!         println!("Calendar: {} (sync token: {:?})",
//!                  calendar.displayname.as_deref().unwrap_or("unnamed"),
//!                  calendar.sync_token.as_ref().map(|s| &s[..20]));
//!     }
//!     
//!     // Choose synchronization strategy based on capabilities
//!     if has_sync_support && !calendars.is_empty() {
//!         println!("Using efficient WebDAV-Sync for synchronization");
//!         sync_with_webdav_sync(&client, &calendars[0]).await?;
//!     } else {
//!         println!("Using traditional polling for synchronization");
//!         sync_with_polling(&client, home).await?;
//!     }
//!     
//!     Ok(())
//! }
//!
//! /// Efficient synchronization using WebDAV-Sync
//! async fn sync_with_webdav_sync(client: &CalDavClient, calendar: &fast_dav_rs::CalendarInfo) -> Result<()> {
//!     let mut sync_token = calendar.sync_token.clone();
//!     
//!     loop {
//!         println!("Syncing with token: {:?}", sync_token.as_ref().map(|s| &s[..20]));
//!         
//!         // Perform incremental sync
//!         let sync_response = client.sync_collection(
//!             &calendar.href,
//!             sync_token.as_deref(),
//!             Some(100), // Limit results
//!             true // Include data
//!         ).await?;
//!         
//!         println!("Received {} updates", sync_response.items.len());
//!         
//!         // Process changes
//!         for item in &sync_response.items {
//!             if item.is_deleted {
//!                 println!("Deleted: {}", item.href);
//!             } else if let Some(data) = &item.calendar_data {
//!                 println!("Updated: {} ({} chars)", item.href, data.len());
//!             } else {
//!                 println!("Changed: {} (no data)", item.href);
//!             }
//!         }
//!         
//!         // Update sync token for next iteration
//!         sync_token = sync_response.sync_token;
//!         
//!         // Break if no more changes or implement your own exit condition
//!         if sync_response.items.is_empty() {
//!             break;
//!         }
//!         
//!         // In a real application, you'd probably want to sleep between syncs
//!         // tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
//!     }
//!     
//!     Ok(())
//! }
//!
//! /// Traditional synchronization using polling
//! async fn sync_with_polling(client: &CalDavClient, calendar_home: &str) -> Result<()> {
//!     // Get all calendars
//!     let calendars = client.list_calendars(calendar_home).await?;
//!     
//!     for calendar in calendars {
//!         println!("Polling calendar: {:?}", calendar.displayname);
//!         
//!         // Query recent events (example with fixed dates)
//!         let start = "20240101T000000Z";
//!         let end = "20240201T000000Z";
//!         
//!         let events = client.calendar_query_timerange(
//!             &calendar.href,
//!             "VEVENT",
//!             Some(&start),
//!             Some(&end),
//!             true // Include data
//!         ).await?;
//!         
//!         println!("Found {} events in {}", events.len(), calendar.displayname.unwrap_or_default());
//!         
//!         // Process events (in a real app, you'd compare with local cache)
//!         for event in events {
//!             if let Some(data) = event.calendar_data {
//!                 println!("Event: {} ({})", event.href, data.lines().next().unwrap_or(""));
//!             }
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ```no_run
//! use fast_dav_rs::CalDavClient;
//! use bytes::Bytes;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let event_path = ""; // Placeholder
//!     // Get current ETag before modifying a resource
//!     let head_response = client.head("my-calendar/some-event.ics").await?;
//!     if let Some(etag) = CalDavClient::etag_from_headers(head_response.headers()) {
//!         // Now we can safely update with If-Match
//!         let updated_ics = Bytes::from(r#"BEGIN:VCALENDAR
//! VERSION:2.0
//! PRODID:-//fast-dav-rs//EN
//! BEGIN:VEVENT
//! UID:some-event@example.com
//! DTSTAMP:20230101T000000Z
//! DTSTART:20231225T100000Z
//! DTEND:20231225T110000Z
//! SUMMARY:Updated Event Title
//! END:VEVENT
//! END:VCALENDAR
//! "#);
//!         
//!         let response = client.put_if_match("my-calendar/some-event.ics", updated_ics, &etag).await?;
//!         if response.status().is_success() {
//!             println!("Successfully updated event");
//!         } else {
//!             println!("Failed to update event: {}", response.status());
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Streaming Large Responses
//!
//! For processing large collections without loading everything into memory:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth, parse_multistatus_stream, detect_encoding};
//! use bytes::Bytes;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let calendar_path = ""; // Placeholder
//!     // Stream a large PROPFIND response
//!     let propfind_xml = r#"
//!     <D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//!       <D:prop>
//!         <D:displayname/>
//!         <D:getetag/>
//!         <C:calendar-data/>
//!       </D:prop>
//!     </D:propfind>"#;
//!     
//!     let response = client.propfind_stream("large-calendar/", Depth::One, propfind_xml).await?;
//!     let encodings = detect_encodings(response.headers());
//!     let items = parse_multistatus_stream(response.into_body(), &encodings).await?;
//!     
//!     // Process items one by one without loading everything into memory
//!     for item in items {
//!         println!("Found item: {} with etag: {:?}",
//!                  item.displayname.unwrap_or_default(),
//!                  item.etag);
//!         
//!         // Process calendar data if present
//!         if let Some(data) = item.calendar_data {
//!             // Handle large iCalendar data efficiently
//!             println!("Processing calendar data of length: {}", data.len());
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Concurrent Batch Operations
//!
//! Execute multiple operations concurrently with controlled parallelism:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth};
//! use bytes::Bytes;
//! use std::sync::Arc;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let calendar_paths = vec!["".to_string()]; // Placeholder
//!     // Prepare a common PROPFIND request for multiple calendars
//!     let propfind_body = Arc::new(Bytes::from(r#"
//!     <D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//!       <D:prop>
//!         <D:displayname/>
//!         <D:getetag/>
//!         <C:supported-calendar-component-set/>
//!       </D:prop>
//!     </D:propfind>"#));
//!     
//!     // Execute PROPFIND on multiple calendars concurrently (max 5 in parallel)
//!     let results = client.propfind_many(
//!         calendar_paths,  // Vector of calendar paths
//!         Depth::Zero,
//!         propfind_body,
//!         5  // Maximum concurrency
//!     ).await;
//!     
//!     // Process results in the same order as input
//!     for result in results {
//!         match result.result {
//!             Ok(response) => {
//!                 if response.status().is_success() {
//!                     println!("Successfully queried: {}", result.pub_path);
//!                     // Parse response body as needed
//!                 } else {
//!                     println!("Query failed for {}: {}", result.pub_path, response.status());
//!                 }
//!             }
//!             Err(e) => {
//!                 println!("Error querying {}: {}", result.pub_path, e);
//!             }
//!         }
//!     }
//!     
//!     // Batch event updates
//!     let event_updates = vec![
//!         ("event1.ics", "BEGIN:VCALENDAR...END:VCALENDAR"),
//!         ("event2.ics", "BEGIN:VCALENDAR...END:VCALENDAR"),
//!     ];
//!     
//!     # let calendar_path = ""; // Placeholder
//!     // Upload multiple events concurrently
//!     let mut upload_tasks = Vec::new();
//!     for (filename, ical_data) in event_updates {
//!         let client_clone = client.clone();
//!         let path = format!("{}/{}", calendar_path, filename);
//!         let data = Bytes::from(ical_data);
//!         
//!         upload_tasks.push(tokio::spawn(async move {
//!             client_clone.put(&path, data).await
//!         }));
//!     }
//!     
//!     // Wait for all uploads to complete
//!     let upload_results = futures::future::join_all(upload_tasks).await;
//!     for (i, result) in upload_results.into_iter().enumerate() {
//!         match result {
//!             Ok(Ok(response)) => {
//!                 println!("Upload {} completed with status: {}", i, response.status());
//!             }
//!             Ok(Err(e)) => {
//!                 println!("Upload {} failed with error: {}", i, e);
//!             }
//!             Err(e) => {
//!                 println!("Upload {} panicked: {}", i, e);
//!             }
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Bootstrap and Capability Detection
//!
//! Discover server capabilities and choose appropriate synchronization methods:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, Depth};
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     // Bootstrap: Discover server capabilities
//!     println!("Detecting server capabilities...");
//!     
//!     // Check if server supports WebDAV-Sync (RFC 6578)
//!     let has_sync_support = client.supports_webdav_sync().await?;
//!     println!("WebDAV-Sync support: {}", has_sync_support);
//!     
//!     // Discover user principal
//!     let principal = client.discover_current_user_principal().await?
//!         .ok_or_else(|| anyhow::anyhow!("No principal found"))?;
//!     println!("User principal: {}", principal);
//!     
//!     // Discover calendar homes
//!     let homes = client.discover_calendar_home_set(&principal).await?;
//!     let home = homes.first().ok_or_else(|| anyhow::anyhow!("No calendar home found"))?;
//!     println!("Calendar home: {}", home);
//!     
//!     // List calendars with detailed info
//!     let calendars = client.list_calendars(home).await?;
//!     for calendar in &calendars {
//!         println!("Calendar: {} (sync token: {:?})",
//!                  calendar.displayname.as_deref().unwrap_or("unnamed"),
//!                  calendar.sync_token.as_ref().map(|s| &s[..20]));
//!     }
//!     
//!     // Choose synchronization strategy based on capabilities
//!     if has_sync_support && !calendars.is_empty() {
//!         println!("Using efficient WebDAV-Sync for synchronization");
//!         sync_with_webdav_sync(&client, &calendars[0]).await?;
//!     } else {
//!         println!("Using traditional polling for synchronization");
//!         sync_with_polling(&client, home).await?;
//!     }
//!     
//!     Ok(())
//! }
//!
//! /// Efficient synchronization using WebDAV-Sync
//! async fn sync_with_webdav_sync(client: &CalDavClient, calendar: &fast_dav_rs::CalendarInfo) -> Result<()> {
//!     let mut sync_token = calendar.sync_token.clone();
//!     
//!     loop {
//!         println!("Syncing with token: {:?}", sync_token.as_ref().map(|s| &s[..20]));
//!         
//!         // Perform incremental sync
//!         let sync_response = client.sync_collection(
//!             &calendar.href,
//!             sync_token.as_deref(),
//!             Some(100), // Limit results
//!             true // Include data
//!         ).await?;
//!         
//!         println!("Received {} updates", sync_response.items.len());
//!         
//!         // Process changes
//!         for item in &sync_response.items {
//!             if item.is_deleted {
//!                 println!("Deleted: {}", item.href);
//!             } else if let Some(data) = &item.calendar_data {
//!                 println!("Updated: {} ({} chars)", item.href, data.len());
//!             } else {
//!                 println!("Changed: {} (no data)", item.href);
//!             }
//!         }
//!         
//!         // Update sync token for next iteration
//!         sync_token = sync_response.sync_token;
//!         
//!         // Break if no more changes or implement your own exit condition
//!         if sync_response.items.is_empty() {
//!             break;
//!         }
//!         
//!         // In a real application, you'd probably want to sleep between syncs
//!         // tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
//!     }
//!     
//!     Ok(())
//! }
//!
//! /// Traditional synchronization using polling
//! async fn sync_with_polling(client: &CalDavClient, calendar_home: &str) -> Result<()> {
//!     // Get all calendars
//!     let calendars = client.list_calendars(calendar_home).await?;
//!     
//!     for calendar in calendars {
//!         println!("Polling calendar: {:?}", calendar.displayname);
//!         
//!         // Query recent events (example with fixed dates)
//!         let start = "20240101T000000Z";
//!         let end = "20240201T000000Z";
//!         
//!         let events = client.calendar_query_timerange(
//!             &calendar.href,
//!             "VEVENT",
//!             Some(&start),
//!             Some(&end),
//!             true // Include data
//!         ).await?;
//!         
//!         println!("Found {} events in {}", events.len(), calendar.displayname.unwrap_or_default());
//!         
//!         // Process events (in a real app, you'd compare with local cache)
//!         for event in events {
//!             if let Some(data) = event.calendar_data {
//!                 println!("Event: {} ({})", event.href, data.lines().next().unwrap_or(""));
//!             }
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Synchronization Strategies
//!
//! Efficient synchronization is crucial for CalDAV clients. This library supports multiple
//! synchronization approaches depending on server capabilities.
//!
//! ### Initial Full Synchronization
//!
//! For the first sync or when starting fresh, retrieve all calendar data:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, CalendarObject};
//! use anyhow::Result;
//! use std::collections::HashMap;
//! use std::sync::Arc;
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     // Discover calendar home (see Bootstrap section for complete discovery)
//!     # let calendar_home = "";
//!     
//!     // Get all calendars
//!     let calendars = client.list_calendars(calendar_home).await?;
//!     
//!     // Local cache to store events
//!     let mut local_cache: HashMap<String, (String, Option<String>)> = HashMap::new();
//!     
//!     // Process all calendars in parallel for better performance
//!     for calendar in calendars {
//!         println!("Syncing calendar: {}", calendar.displayname.as_deref().unwrap_or("unnamed"));
//!         
//!         // Prepare calendar-query request to get all events with data
//!         let query_body = Arc::new(Bytes::from(r#"
//!         <C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//!           <D:prop>
//!             <D:getetag/>
//!             <C:calendar-data/>
//!           </D:prop>
//!           <C:filter>
//!             <C:comp-filter name="VCALENDAR">
//!               <C:comp-filter name="VEVENT"/>
//!             </C:comp-filter>
//!           </C:filter>
//!         </C:calendar-query>"#));
//!         
//!         // Get all events in the calendar using parallel requests (max 5 concurrent)
//!         let calendar_paths = vec![calendar.href.clone()];
//!         let event_results = client.report_many(
//!             calendar_paths,
//!             fast_dav_rs::Depth::One,
//!             query_body,
//!             5  // Max concurrency
//!         ).await;
//!         
//!         // Process results
//!         let mut total_events = 0;
//!         for batch_result in event_results {
//!             match batch_result.result {
//!                 Ok(response) if response.status().is_success() => {
//!                     let body = response.into_body();
//!                     if let Ok(items) = fast_dav_rs::parse_multistatus_bytes(&body) {
//!                         total_events += items.len();
//!                         
//!                         // Store events in local cache
//!                         for item in items {
//!                             if let Some(data) = item.calendar_data {
//!                                 local_cache.insert(item.href.clone(), (data, item.etag.clone()));
//!                                 println!("Cached event: {} (ETag: {:?})", item.href, item.etag);
//!                             }
//!                         }
//!                     }
//!                 }
//!                 Ok(response) => {
//!                     eprintln!("Calendar query failed with status: {}", response.status());
//!                 }
//!                 Err(e) => {
//!                     eprintln!("Calendar query error: {}", e);
//!                 }
//!             }
//!         }
//!         
//!         println!("Found {} events in calendar", total_events);
//!     }
//!     
//!     println!("Initial sync completed. Cached {} events.", local_cache.len());
//!     Ok(())
//! }
//! ```
//!
//! ### Incremental Synchronization with ETags
//!
//! After initial sync, use ETags for efficient updates. This approach checks each event
//! individually to see if it has changed:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, CalendarObject};
//! use anyhow::Result;
//! use std::collections::HashMap;
//! use std::sync::Arc;
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     # let calendar_home = "";
//!     # let mut local_cache: HashMap<String, (String, Option<String>)> = HashMap::new();
//!     
//!     // Get all calendars
//!     let calendars = client.list_calendars(calendar_home).await?;
//!     
//!     // Process all calendars in parallel for better performance
//!     for calendar in calendars {
//!         println!("Incremental sync for calendar: {}", calendar.displayname.as_deref().unwrap_or("unnamed"));
//!         
//!         // Prepare PROPFIND request to get metadata for all events
//!         let propfind_body = Arc::new(Bytes::from(r#"
//!         <D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//!           <D:prop>
//!             <D:getetag/>
//!             <D:getlastmodified/>
//!           </D:prop>
//!         </D:propfind>"#));
//!         
//!         // Get metadata for all events in parallel (max 10 concurrent requests)
//!         let calendar_paths = vec![calendar.href.clone()];
//!         let meta_results = client.propfind_many(
//!             calendar_paths,
//!             fast_dav_rs::Depth::One,
//!             propfind_body,
//!             10  // Max concurrency
//!         ).await;
//!         
//!         // Process results
//!         for batch_result in meta_results {
//!             match batch_result.result {
//!                 Ok(response) if response.status().is_success() => {
//!                     let body = response.into_body();
//!                     if let Ok(items) = fast_dav_rs::parse_multistatus_bytes(&body) {
//!                         println!("Checking {} events for changes", items.len());
//!                         
//!                         // Check each event for changes using ETags
//!                         for item in items {
//!                             let local_entry = local_cache.get(&item.href);
//!                             
//!                             match (&local_entry, &item.etag) {
//!                                 // New event (not in cache)
//!                                 (None, _) => {
//!                                     println!("New event detected: {}", item.href);
//!                                     // Fetch full event data
//!                                     if let Ok(response) = client.get(&item.href).await {
//!                                         if response.status().is_success() {
//!                                             let data = String::from_utf8(response.into_body().to_vec())?;
//!                                             let etag = CalDavClient::etag_from_headers(response.headers());
//!                                             local_cache.insert(item.href.clone(), (data, etag));
//!                                             println!("Downloaded new event: {}", item.href);
//!                                         }
//!                                     }
//!                                 },
//!                                 
//!                                 // Existing event - check if ETag changed
//!                                 (Some((_, local_etag)), Some(server_etag)) => {
//!                                     if local_etag.as_ref() != Some(server_etag) {
//!                                         println!("Event changed: {}", item.href);
//!                                         // ETag changed, fetch updated data
//!                                         if let Ok(response) = client.get(&item.href).await {
//!                                             if response.status().is_success() {
//!                                                 let data = String::from_utf8(response.into_body().to_vec())?;
//!                                                 local_cache.insert(item.href.clone(), (data, Some(server_etag.clone())));
//!                                                 println!("Updated cached event: {}", item.href);
//!                                             }
//!                                         }
//!                                     } else {
//!                                         println!("Event unchanged: {}", item.href);
//!                                     }
//!                                 },
//!                                 
//!                                 // Unexpected state
//!                                 _ => {
//!                                     println!("Unexpected state for event: {}", item.href);
//!                                 }
//!                             }
//!                         }
//!                         
//!                         // For simplicity, we're not checking for deletions in this example
//!                         // A full implementation would compare server items with local cache
//!                     }
//!                 }
//!                 Ok(response) => {
//!                     eprintln!("PROPFIND failed with status: {}", response.status());
//!                 }
//!                 Err(e) => {
//!                     eprintln!("PROPFIND error: {}", e);
//!                 }
//!             }
//!         }
//!     }
//!     
//!     println!("Incremental sync completed. Cache now contains {} events.", local_cache.len());
//!     Ok(())
//! }
//! ```
//!
//! ### Efficient WebDAV-Sync (RFC 6578)
//!
//! When supported by the server, WebDAV-Sync provides the most efficient synchronization
//! by only returning changed items:
//!
//! ```no_run
//! use fast_dav_rs::{CalDavClient, CalendarObject};
//! use anyhow::Result;
//! use std::collections::HashMap;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = CalDavClient::new(
//!         "https://caldav.example.com/user/",
//!         Some("username"),
//!         Some("password"),
//!     )?;
//!     
//!     // Check if server supports WebDAV-Sync
//!     let supports_sync = client.supports_webdav_sync().await?;
//!     if !supports_sync {
//!         println!("Server does not support WebDAV-Sync, falling back to ETag sync");
//!         return Ok(());
//!     }
//!     
//!     # let mut local_cache: HashMap<String, (String, Option<String>)> = HashMap::new();
//!     
//!     // Discover calendars
//!     # let calendar_home = "";
//!     let calendars = client.list_calendars(calendar_home).await?;
//!     
//!     for calendar in calendars {
//!         println!("WebDAV-Sync for calendar: {}", calendar.displayname.as_deref().unwrap_or("unnamed"));
//!         
//!         // Start with the calendar's sync token (None for initial sync)
//!         let mut sync_token = calendar.sync_token.clone();
//!         
//!         loop {
//!             // Perform incremental sync
//!             let sync_response = client.sync_collection(
//!                 &calendar.href,
//!                 sync_token.as_deref(), // Previous sync token
//!                 Some(100),             // Limit results for pagination
//!                 true                   // Include calendar data
//!             ).await?;
//!             
//!             println!("Received {} updates", sync_response.items.len());
//!             
//!             // Process changes
//!             for item in &sync_response.items {
//!                 if item.is_deleted {
//!                     // Remove deleted event from cache
//!                     local_cache.remove(&item.href);
//!                     println!("Deleted: {}", item.href);
//!                 } else if let Some(data) = &item.calendar_data {
//!                     // Update or add changed event
//!                     local_cache.insert(item.href.clone(), (data.clone(), item.etag.clone()));
//!                     println!("Updated: {} ({} chars)", item.href, data.len());
//!                 } else {
//!                     // Changed item without data - might need to fetch
//!                     println!("Changed (no data): {}", item.href);
//!                 }
//!             }
//!             
//!             // Update sync token for next iteration
//!             sync_token = sync_response.sync_token.clone();
//!             
//!             // If we got fewer items than our limit, we're done
//!             // In a real app, you might want to store the sync_token persistently
//!             if sync_response.items.len() < 100 {
//!                 break;
//!             }
//!             
//!             // For demonstration, we'll break after one iteration
//!             // In practice, you'd continue until no more changes
//!             break;
//!         }
//!         
//!         // Store the final sync token for next sync session
//!         if let Some(token) = &sync_token {
//!             println!("Next sync token: {}...", &token[..std::cmp::min(20, token.len())]);
//!             // In a real app: save_token_to_storage(&calendar.href, token);
//!         }
//!     }
//!     
//!     println!("WebDAV-Sync completed. Cache now contains {} events.", local_cache.len());
//!     Ok(())
//! }
//! ```
//!
//! ## Choosing the Right Synchronization Strategy
//!
//! 1. **Initial Sync**: Always start with a full sync to establish your local cache
//! 2. **Ongoing Sync**: Choose based on server capabilities:
//!    - **WebDAV-Sync**: Most efficient when supported (check with `supports_webdav_sync()`)
//!    - **ETag Sync**: Good fallback when WebDAV-Sync is not available
//!    - **Polling**: Least efficient, only for servers with minimal CalDAV support
//!
//! ### Performance Tips
//!
//! - Use **parallel requests** (`propfind_many`, `report_many`) when processing multiple
//!   calendars or large collections to improve performance
//! - Set appropriate **concurrency limits** to avoid overwhelming the server
//! - **Cache ETags** locally to enable efficient incremental sync
//! - Use **WebDAV-Sync** when available for the most efficient synchronization
//! - Consider **pagination** for very large collections to manage memory usage
//!
//! The examples above show complete workflows for each approach, demonstrating
//! how to handle new events, updates, and deletions efficiently.
pub mod caldav;
pub mod common;

// Backwards-compatible re-exports
pub use caldav::streaming::{
    parse_multistatus_bytes, parse_multistatus_bytes_visit, parse_multistatus_stream,
    parse_multistatus_stream_visit,
};
pub use caldav::{
    BatchItem, CalDavClient, CalendarInfo, CalendarObject, DavItem, Depth, SyncItem, SyncResponse,
    build_calendar_multiget_body, build_calendar_query_body, build_sync_collection_body,
    map_calendar_list, map_calendar_objects, map_sync_response,
};
pub use common::compression::{
    ContentEncoding, add_accept_encoding, add_content_encoding, compress_payload, detect_encoding,
    detect_encodings, detect_request_compression_preference,
};

// Legacy module paths kept for compatibility with existing imports.
pub mod client {
    pub use crate::caldav::client::*;
}

pub mod streaming {
    pub use crate::caldav::streaming::*;
}

pub mod types {
    pub use crate::caldav::types::*;
}

pub mod compression {
    pub use crate::common::compression::*;
}
