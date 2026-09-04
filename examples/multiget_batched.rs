//! Batched content fetching: `calendar_multiget_many` splits a large href
//! list into chunked `calendar-multiget` REPORTs with bounded concurrency,
//! and reports per-chunk results so one failed chunk does not lose the rest.
//!
//! Target fixture: **Radicale** (`radicale-test/`, Basic auth `test`/`test`).
//!
//! ```sh
//! ./radicale-test/setup.sh        # start + seed the fixture on http://localhost:8081
//! cargo run --example multiget_batched
//! ```

use bytes::Bytes;
use fast_dav_rs::CalDavClient;

const USER: &str = "test";
const PASS: &str = "test";
const COLLECTION: &str = "test/example-multiget/";
const EVENT_COUNT: usize = 25;
const BATCH_SIZE: usize = 10;
const CONCURRENCY: usize = 3;

fn radicale_url() -> String {
    let mut url = std::env::var("RADICALE_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

fn event_ics(uid: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//multiget example//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260914T100000Z\r\n\
         SUMMARY:event {uid}\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR"
    )
}

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = CalDavClient::new(&radicale_url(), Some(USER), Some(PASS))?;

    // Fixture data: a calendar with EVENT_COUNT events.
    let mk = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop><D:displayname>multiget example</D:displayname></D:prop></D:set>
</C:mkcalendar>"#;
    let _ = client.mkcalendar(COLLECTION, mk).await?;
    let mut hrefs = Vec::with_capacity(EVENT_COUNT);
    for i in 0..EVENT_COUNT {
        let path = format!("{COLLECTION}multi-{i}.ics");
        let status = client
            .put(
                &path,
                Bytes::from(event_ics(&format!("multi-{i}@example.com"))),
            )
            .await?
            .status();
        assert!(status.is_success(), "seed PUT failed with {status}");
        hrefs.push(path);
    }
    println!("seeded {EVENT_COUNT} events");

    // One REPORT per chunk of BATCH_SIZE hrefs, at most CONCURRENCY in flight.
    let batches = client
        .calendar_multiget_many(COLLECTION, &hrefs, true, None, BATCH_SIZE, CONCURRENCY)
        .await?;
    println!(
        "{EVENT_COUNT} hrefs -> {} REPORTs (batch size {BATCH_SIZE}, max {CONCURRENCY} in flight)",
        batches.len()
    );

    for batch in &batches {
        match &batch.result {
            // One CalendarObject per item of a successful chunk.
            Ok(object) => println!(
                "  {} (etag {:?}, {} bytes)",
                object.href,
                object.etag,
                object.calendar_data.as_ref().map_or(0, |d| d.len())
            ),
            // Partial failure: `hrefs` is exactly the chunk to re-fetch.
            Err(err) => {
                println!(
                    "chunk failed ({err}); re-fetch these hrefs later: {:?}",
                    batch.hrefs
                );
            }
        }
    }
    let ok = batches.iter().filter(|b| b.result.is_ok()).count();
    println!("{ok}/{} chunks succeeded — done", batches.len());

    client.delete(COLLECTION).await?;
    Ok(())
}
