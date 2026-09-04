//! Streaming a large collection PROPFIND with constant memory: `propfind_stream`
//! hands you the raw (possibly compressed) body; `parse_multistatus_stream_visit`
//! invokes your callback per item as bytes arrive, so an arbitrarily large
//! collection never needs to fit in memory.
//!
//! Target fixture: **Radicale** (`radicale-test/`, Basic auth `test`/`test`).
//!
//! ```sh
//! ./radicale-test/setup.sh        # start + seed the fixture on http://localhost:8081
//! cargo run --example streaming_large_collections
//! ```

use bytes::Bytes;
use fast_dav_rs::caldav::parse_multistatus_stream_visit;
use fast_dav_rs::{CalDavClient, Depth, detect_encoding};

const USER: &str = "test";
const PASS: &str = "test";
const COLLECTION: &str = "test/example-streaming/";
const EVENT_COUNT: usize = 40;

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
         PRODID:-//fast-dav-rs//streaming example//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260912T100000Z\r\n\
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
  <D:set><D:prop><D:displayname>streaming example</D:displayname></D:prop></D:set>
</C:mkcalendar>"#;
    let _ = client.mkcalendar(COLLECTION, mk).await?;
    for i in 0..EVENT_COUNT {
        let path = format!("{COLLECTION}stream-{i}.ics");
        let status = client
            .put(
                &path,
                Bytes::from(event_ics(&format!("stream-{i}@example.com"))),
            )
            .await?
            .status();
        assert!(status.is_success(), "seed PUT failed with {status}");
    }
    println!("seeded {EVENT_COUNT} events");

    // Stream a Depth:1 PROPFIND. The response body is not aggregated: the
    // parser consumes it incrementally and hands you one item at a time.
    let propfind = r#"<D:propfind xmlns:D="DAV:"><D:prop><D:getetag/></D:prop></D:propfind>"#;
    let response = client
        .propfind_stream(COLLECTION, Depth::One, propfind)
        .await?;

    let encoding = detect_encoding(response.headers());
    let mut seen = 0usize;
    parse_multistatus_stream_visit(response.into_body(), &[encoding], |item| {
        if item.is_collection {
            return Ok(()); // the Depth:1 answer starts with the collection itself
        }
        seen += 1;
        println!("{:>3}. {} (etag {:?})", seen, item.href, item.etag);
        Ok(())
    })
    .await?;

    println!("streamed {seen} items without buffering the whole response");
    client.delete(COLLECTION).await?;
    println!("done");
    Ok(())
}
