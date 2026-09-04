//! Discovery + basic CRUD + conditional (ETag-protected) writes.
//!
//! Target fixture: **Radicale** (`radicale-test/`, Basic auth `test`/`test`).
//!
//! ```sh
//! ./radicale-test/setup.sh        # start + seed the fixture on http://localhost:8081
//! cargo run --example getting_started
//! ```

use bytes::Bytes;
use fast_dav_rs::webdav::etag_from_headers;
use fast_dav_rs::{CalDavClient, Depth};

const USER: &str = "test";
const PASS: &str = "test";

fn radicale_url() -> String {
    let mut url = std::env::var("RADICALE_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

fn event_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//getting-started example//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260910T100000Z\r\n\
         DTEND:20260910T110000Z\r\n\
         SUMMARY:{summary}\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR"
    )
}

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = CalDavClient::new(&radicale_url(), Some(USER), Some(PASS))?;

    // 1. Discovery: principal -> calendar home -> calendars.
    //    On Radicale all three resolve to `/{user}/`.
    let principal = client
        .discover_current_user_principal()
        .await?
        .expect("Radicale advertises a current-user-principal");
    println!("principal: {principal}");

    let home = client
        .discover_calendar_home_set(&principal)
        .await?
        .into_iter()
        .next()
        .expect("Radicale advertises a calendar home set");
    println!("calendar home: {home}");

    // 2. Create a working calendar (405 = already there from a previous run).
    let calendar_path = format!("{USER}/example-getting-started/");
    let mk = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop><D:displayname>getting-started example</D:displayname></D:prop></D:set>
</C:mkcalendar>"#;
    let resp = client.mkcalendar(&calendar_path, mk).await?;
    println!("mkcalendar: {}", resp.status());

    // 3. Create an event; If-None-Match makes the write fail (412) instead of
    //    silently overwriting an existing resource.
    let event_path = format!("{calendar_path}getting-started-1.ics");
    let created = client
        .put_if_none_match(
            &event_path,
            Bytes::from(event_ics("gs-1@example.com", "first")),
        )
        .await?;
    println!("create (If-None-Match): {}", created.status());
    let etag = match etag_from_headers(created.headers()) {
        Some(etag) => etag,
        None => etag_from_headers(client.get(&event_path).await?.headers())
            .expect("server returns an ETag on GET"),
    };
    println!("etag: {etag}");

    // 4. Conditional update: only succeeds while the etag is current. A peer
    //    editing in between invalidates our copy and the server answers 412.
    let updated = client
        .put_if_match(
            &event_path,
            Bytes::from(event_ics("gs-1@example.com", "edited")),
            &etag,
        )
        .await?;
    println!("update (If-Match): {}", updated.status());

    let stale = client
        .put_if_match(
            &event_path,
            Bytes::from(event_ics("gs-1@example.com", "lost race")),
            &etag,
        )
        .await?;
    println!(
        "stale update (If-Match): {} — the stale etag lost the race",
        stale.status()
    );

    // 5. Conditional delete, then clean up the calendar.
    let current = etag_from_headers(client.get(&event_path).await?.headers()).unwrap();
    client.delete_if_match(&event_path, &current).await?;
    println!("deleted event");

    let _ = client.list_calendars(&home).await?; // harmless: shows collection listing
    let items = client
        .propfind(
            &calendar_path,
            Depth::One,
            r#"<D:propfind xmlns:D="DAV:"><D:prop><D:getetag/></D:prop></D:propfind>"#,
        )
        .await?;
    println!("calendar listing status: {}", items.status());

    client.delete(&calendar_path).await?;
    println!("deleted calendar — done");
    Ok(())
}
