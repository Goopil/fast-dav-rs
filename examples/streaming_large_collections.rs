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

#[path = "common/mod.rs"]
mod common;

use bytes::Bytes;
use fast_dav_rs::caldav::parse_multistatus_stream_visit;
use fast_dav_rs::{Depth, detect_encoding};

use common::radicale_client;

const COLLECTION: &str = "test/example-streaming/";
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
