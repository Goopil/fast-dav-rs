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

#[path = "common/mod.rs"]
mod common;

use bytes::Bytes;

use common::radicale_client;

const COLLECTION: &str = "test/example-multiget/";
const EVENT_COUNT: usize = 25;
const BATCH_SIZE: usize = 10;
const CONCURRENCY: usize = 3;

fn event_ics(uid: &str) -> String {
    common::event_ics(uid, &format!("event {uid}"))
}

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = radicale_client()?;

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
