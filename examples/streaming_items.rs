//! Item-by-item streaming: `calendar_query_stream` yields each parsed
//! [`CalendarObject`] as soon as its `<D:response>` completes, so memory stays
//! bounded by the current item regardless of collection size. Dropping the
//! stream aborts the download.
//!
//! Target fixture: **Radicale** (`radicale-test/`, Basic auth `test`/`test`).
//!
//! ```sh
//! ./radicale-test/setup.sh        # start + seed the fixture on http://localhost:8081
//! cargo run --example streaming_items
//! ```

#[path = "common/mod.rs"]
mod common;

use bytes::Bytes;
use fast_dav_rs::CalendarObject;
use futures::StreamExt;

use common::radicale_client;

const COLLECTION: &str = "test/example-streaming-items/";
const EVENT_COUNT: usize = 40;

fn event_ics(uid: &str) -> String {
    common::event_ics(uid, &format!("event {uid}"))
}

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = radicale_client()?;

    // Fixture data: a calendar with EVENT_COUNT events.
    let mk = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop><D:displayname>streaming items example</D:displayname></D:prop></D:set>
</C:mkcalendar>"#;
    let _ = client.mkcalendar(COLLECTION, mk).await?;
    for i in 0..EVENT_COUNT {
        let path = format!("{COLLECTION}stream-{i}.ics");
        let status = client
            .put(
                &path,
                Bytes::from(event_ics(&format!("stream-items-{i}@example.com"))),
            )
            .await?
            .status();
        assert!(status.is_success(), "seed PUT failed with {status}");
    }
    println!("seeded {EVENT_COUNT} events");

    // Each object arrives as soon as its <D:response> is parsed; the body is
    // decompressed on the fly and never aggregated.
    let mut stream = client
        .calendar_query_stream(COLLECTION, "VEVENT", None, None, true, None)
        .await?;
    let mut seen = 0usize;
    while let Some(object) = stream.next().await {
        let object: CalendarObject = object?;
        seen += 1;
        let bytes = object.calendar_data.as_deref().map_or(0, str::len);
        println!(
            "{:>3}. {} ({} bytes, etag {:?})",
            seen, object.href, bytes, object.etag
        );
    }

    println!("streamed {seen} items with memory bounded by the current item");
    client.delete(COLLECTION).await?;
    println!("done");
    Ok(())
}
