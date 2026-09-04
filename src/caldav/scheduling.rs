//! CalDAV scheduling types (RFC 6638).
//!
//! Scheduling discovery surfaces the calendar user's scheduling
//! collections ([`ScheduleEndpoints`]); the outbox accepts iTIP messages
//! ([`SchedulingResponse`](crate::caldav::SchedulingResponse)) and the
//! inbox delivers them as [`InboxItem`](crate::caldav::InboxItem)s.

use bytes::Bytes;
use hyper::{HeaderMap, Method, Response, StatusCode, header};

use crate::Result;
use crate::caldav::client::ICAL_CONTENT_TYPE;
use crate::caldav::streaming::parse_multistatus_bytes;
use crate::caldav::types::DavItem;
use crate::webdav::types::map_sync_rows;
use crate::{CalDavClient, Error, Operation};

/// Scheduling endpoints of a principal (RFC 6638 §2).
///
/// Populated by
/// [`CalDavClient::discover_schedule_endpoints`](crate::CalDavClient::discover_schedule_endpoints):
/// `inbox`/`outbox` are `None` when the server omits the corresponding
/// `schedule-inbox-URL`/`schedule-outbox-URL` property (the calendar user
/// is not enabled for receiving/sending scheduling messages on the server).
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct ScheduleEndpoints {
    /// `schedule-inbox-URL` href (RFC 6638 §2.2.1).
    pub inbox: Option<String>,
    /// `schedule-outbox-URL` href (RFC 6638 §2.1.1).
    pub outbox: Option<String>,
    /// `calendar-user-address-set` hrefs (RFC 6638 §2.4.1), e.g.
    /// `mailto:bernard@example.com`; sorted and de-duplicated.
    pub user_addresses: Vec<String>,
}

/// Raw response to a scheduling `POST` against a scheduling outbox
/// collection (RFC 6638 §5).
///
/// The body is returned verbatim — a `text/calendar` body with a
/// `REQUEST-STATUS` property for free-busy requests, or a
/// `CALDAV:schedule-response` XML body — no iTIP parsing is performed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SchedulingResponse {
    /// HTTP status of the outbox `POST` (success, i.e. `2xx`).
    pub status: StatusCode,
    /// Raw response body.
    pub body: Bytes,
}

/// One scheduling message delivered to a schedule inbox (RFC 6638 §2.2),
/// as returned by
/// [`CalDavClient::list_inbox`](crate::CalDavClient::list_inbox).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InboxItem {
    /// Href of the scheduling message resource.
    pub href: String,
    /// `getetag` of the resource, if the server returned one.
    pub etag: Option<String>,
    /// Inline `calendar-data` payload (iCalendar text), if present.
    pub data: Option<String>,
}

/// Build the `If-Schedule-Tag-Match` header value (RFC 6638 §8.3).
///
/// Surrounding quotes are stripped and re-added, so both the raw schedule-tag
/// and the quoted form returned in the `Schedule-Tag` response header are
/// accepted. Empty (or whitespace-only) tags are rejected before any I/O.
fn schedule_tag_header_value(schedule_tag: &str) -> Result<header::HeaderValue> {
    let binding = crate::normalize_etag(schedule_tag);
    let tag = binding.trim();
    if tag.is_empty() {
        return Err(Error::InvalidInput(
            "schedule-tag must not be empty".to_owned(),
        ));
    }
    let value = format!("\"{tag}\"");
    header::HeaderValue::from_str(&value).map_err(|err| {
        Error::InvalidInput(format!(
            "schedule-tag cannot form a valid header value: {err}"
        ))
    })
}

impl CalDavClient {
    /// Discover the scheduling endpoints of a principal (RFC 6638 §2).
    ///
    /// Sends a `PROPFIND` (`Depth: 0`) for `schedule-inbox-URL`,
    /// `schedule-outbox-URL`, and `calendar-user-address-set` against the
    /// principal at `principal_path` and aggregates the properties across
    /// all response elements.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, the server responds with a
    /// non-success status, or the multistatus body cannot be parsed.
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::{CalDavClient, Result};
    ///
    /// # async fn example() -> Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// let endpoints = client.discover_schedule_endpoints("principals/user01/").await?;
    /// if let Some(inbox) = &endpoints.inbox {
    ///     println!("schedule inbox at {inbox}");
    /// }
    /// println!("addresses: {:?}", endpoints.user_addresses);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover_schedule_endpoints(
        &self,
        principal_path: &str,
    ) -> Result<ScheduleEndpoints> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <C:schedule-inbox-URL/>
    <C:schedule-outbox-URL/>
    <C:calendar-user-address-set/>
  </D:prop>
</D:propfind>
"#;
        let resp = self
            .propfind(principal_path, crate::Depth::Zero, body)
            .await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindScheduleEndpoints,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        let mut endpoints = ScheduleEndpoints::default();
        let mut addresses = Vec::new();
        for mut item in parse_multistatus_bytes(&body)?.items {
            if endpoints.inbox.is_none() {
                endpoints.inbox = item.schedule_inbox.take();
            }
            if endpoints.outbox.is_none() {
                endpoints.outbox = item.schedule_outbox.take();
            }
            addresses.append(&mut item.calendar_user_addresses);
        }
        addresses.sort();
        addresses.dedup();
        endpoints.user_addresses = addresses;
        Ok(endpoints)
    }

    /// `POST` an iTIP message to a scheduling outbox collection
    /// (RFC 6638 §5 — e.g. a `VFREEBUSY` free-busy request).
    ///
    /// The body is sent verbatim with `Content-Type: text/calendar`;
    /// neither iTIP parsing nor iCalendar validation is applied. The
    /// raw server response is returned on any success (`2xx`) status —
    /// typically a `text/calendar` body carrying a `REQUEST-STATUS`
    /// property, or a `CALDAV:schedule-response` XML body.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`Error::UnexpectedStatus`](crate::Error::UnexpectedStatus) with
    /// [`Operation::PostSchedule`](crate::Operation::PostSchedule) when the
    /// server responds with a non-success status (e.g. `403` when the
    /// scheduling feature is disabled), and an error when the transport
    /// itself fails.
    ///
    /// # Example
    /// ```no_run
    /// use bytes::Bytes;
    /// use fast_dav_rs::{CalDavClient, Result};
    ///
    /// # async fn example() -> Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// let endpoints = client.discover_schedule_endpoints("principals/user01/").await?;
    /// let outbox = endpoints.outbox.expect("scheduling outbox present");
    ///
    /// // Hand-built iTIP REQUEST (organizer -> attendee).
    /// let itip = "BEGIN:VCALENDAR\r\n\
    ///             VERSION:2.0\r\n\
    ///             PRODID:-//example//EN\r\n\
    ///             METHOD:REQUEST\r\n\
    ///             BEGIN:VEVENT\r\n\
    ///             UID:20010712T182145Z-123401@example.com\r\n\
    ///             DTSTAMP:20060102T000000Z\r\n\
    ///             DTSTART:20060104T140000Z\r\n\
    ///             DTEND:20060104T150000Z\r\n\
    ///             SUMMARY:Design review\r\n\
    ///             ORGANIZER:mailto:user01@example.com\r\n\
    ///             ATTENDEE:mailto:user02@example.com\r\n\
    ///             END:VEVENT\r\n\
    ///             END:VCALENDAR\r\n";
    ///
    /// let response = client
    ///     .post_schedule(&outbox, Bytes::from_static(itip.as_bytes()))
    ///     .await?;
    /// println!("REQUEST-STATUS: {}", response.status);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn post_schedule(
        &self,
        outbox_path: &str,
        ical_body: Bytes,
    ) -> Result<SchedulingResponse> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static(ICAL_CONTENT_TYPE),
        );
        let resp = self
            .send(Method::POST, outbox_path, h, Some(ical_body), None)
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PostSchedule,
                status,
            });
        }
        Ok(SchedulingResponse {
            status,
            body: resp.into_body(),
        })
    }

    /// List the scheduling messages in a schedule inbox (`PROPFIND`
    /// `Depth: 1` with inline `getetag` + `calendar-data`, RFC 6638 §2.2).
    ///
    /// Response elements without any etag or calendar data (e.g. the inbox
    /// collection entry itself) and deleted (`404`/`410`) entries are
    /// skipped; the returned hrefs are relative paths suitable for a
    /// follow-up `GET` or `calendar-multiget`.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`Error::UnexpectedStatus`](crate::Error::UnexpectedStatus) with
    /// [`Operation::ScheduleInbox`](crate::Operation::ScheduleInbox) when the
    /// server responds with a non-success status, and an error when the
    /// request fails or the multistatus body cannot be parsed.
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::{CalDavClient, Result};
    ///
    /// # async fn example() -> Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// let endpoints = client.discover_schedule_endpoints("principals/user01/").await?;
    /// if let Some(inbox) = &endpoints.inbox {
    ///     for item in client.list_inbox(inbox).await? {
    ///         println!("{} -> {:?}", item.href, item.etag);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_inbox(&self, inbox_path: &str) -> Result<Vec<InboxItem>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag/>
    <C:calendar-data/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(inbox_path, crate::Depth::One, body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::ScheduleInbox,
                status: resp.status(),
            });
        }
        let headers = resp.headers().clone();
        let body = resp.into_body();
        let (_, rows, _) = map_sync_rows(
            &headers,
            parse_multistatus_bytes(&body)?.items,
            None,
            |item: &mut DavItem| item.calendar_data.take(),
        );
        Ok(rows
            .into_iter()
            .filter(|row| !row.is_deleted && (row.etag.is_some() || row.data.is_some()))
            .map(|row| InboxItem {
                href: row.href,
                etag: row.etag,
                data: row.data,
            })
            .collect())
    }

    /// Conditional `PUT` on a scheduling object resource guarded by
    /// `If-Schedule-Tag-Match` (RFC 6638 §8.3, §3.2.10.1).
    ///
    /// Use the opaque schedule-tag returned in the `Schedule-Tag` response
    /// header instead of an ETag when an "Attendee" may have processed the
    /// invitation in between: the schedule-tag changes on attendee-driven
    /// changes even when the event data (and therefore the ETag) of the
    /// "Organizer" copy did not. The server answers `412 Precondition
    /// Failed` when the tag no longer matches; the response (any status)
    /// is returned to the caller.
    ///
    /// The body is sent verbatim with `Content-Type: text/calendar` —
    /// unlike [`put`](crate::CalDavClient::put) no client-side iCalendar
    /// validation is applied.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) **before
    /// any network I/O** when `schedule_tag` is empty or cannot form a
    /// valid HTTP header value, and an error when the transport itself
    /// fails.
    ///
    /// # Example
    /// ```no_run
    /// use bytes::Bytes;
    /// use fast_dav_rs::{CalDavClient, Result};
    ///
    /// # async fn example(schedule_tag: &str) -> Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// let ics = Bytes::from_static(b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n");
    /// let resp = client
    ///     .put_if_schedule_tag("calendars/work/meeting.ics", ics, schedule_tag)
    ///     .await?;
    /// if resp.status() == 412 {
    ///     println!("schedule-tag mismatch — reload the event");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn put_if_schedule_tag(
        &self,
        path: &str,
        body: Bytes,
        schedule_tag: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static(ICAL_CONTENT_TYPE),
        );
        h.insert(
            header::HeaderName::from_static("if-schedule-tag-match"),
            schedule_tag_header_value(schedule_tag)?,
        );
        self.send(Method::PUT, path, h, Some(body), None).await
    }

    /// Conditional `DELETE` on a scheduling object resource guarded by
    /// `If-Schedule-Tag-Match` (RFC 6638 §8.3, §3.2.10.2).
    ///
    /// See [`put_if_schedule_tag`](Self::put_if_schedule_tag) for when to
    /// prefer a schedule-tag over an ETag. The server response (any status,
    /// e.g. `204` on success or `412` on tag mismatch) is returned to the
    /// caller.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) **before
    /// any network I/O** when `schedule_tag` is empty or cannot form a
    /// valid HTTP header value, and an error when the transport itself
    /// fails.
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::{CalDavClient, Result};
    ///
    /// # async fn example(schedule_tag: &str) -> Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// let resp = client
    ///     .delete_if_schedule_tag("calendars/work/meeting.ics", schedule_tag)
    ///     .await?;
    /// println!("DELETE returned {}", resp.status());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_if_schedule_tag(
        &self,
        path: &str,
        schedule_tag: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::HeaderName::from_static("if-schedule-tag-match"),
            schedule_tag_header_value(schedule_tag)?,
        );
        self.send(Method::DELETE, path, h, None, None).await
    }
}
