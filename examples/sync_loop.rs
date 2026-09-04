//! A full `SyncSession` loop: initial snapshot, persisted sync token,
//! incremental deltas, stale-token resync — with `calendar-data` parsed by
//! the `icalendar` crate (parsing stays caller-side).
//!
//! Target fixture: **Radicale** (`radicale-test/`, Basic auth `test`/`test`).
//!
//! ```sh
//! ./radicale-test/setup.sh        # start + seed the fixture on http://localhost:8081
//! cargo run --example sync_loop
//! ```
//!
//! Token persistence pattern: the session is in-memory only; the caller
//! persists `sync_token` between runs. The toy store below keeps one token in
//! a file (an in-memory `HashMap<String, String>` keyed by collection is the
//! in-process equivalent for many collections).

use std::path::PathBuf;

use bytes::Bytes;
use fast_dav_rs::CalDavClient;
use icalendar::Component;

const USER: &str = "test";
const PASS: &str = "test";
const COLLECTION: &str = "test/example-sync-loop/";

fn radicale_url() -> String {
    let mut url = std::env::var("RADICALE_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// Toy persistence: one line, one token. Swap for sqlite/your KV store.
struct TokenStore(PathBuf);

impl TokenStore {
    fn load(&self) -> Option<String> {
        std::fs::read_to_string(&self.0)
            .ok()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    }

    fn save(&self, token: Option<&str>) {
        if let Some(token) = token {
            let _ = std::fs::write(&self.0, token);
        }
    }
}

fn event_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//sync-loop example//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260911T090000Z\r\n\
         SUMMARY:{summary}\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR"
    )
}

/// Print parsed `calendar-data` through the `icalendar` crate.
fn print_entry(href: &str, data: Option<&str>) {
    match data.and_then(|d| d.parse::<icalendar::Calendar>().ok()) {
        Some(cal) => {
            for event in cal.events() {
                println!(
                    "  {href} -> {:?} ({})",
                    event.get_summary().unwrap_or("(no summary)"),
                    event.get_uid().unwrap_or("(no uid)"),
                );
            }
        }
        None => println!("  {href} -> (no data or unparsable)"),
    }
}

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = CalDavClient::new(&radicale_url(), Some(USER), Some(PASS))?;
    let store = TokenStore(std::env::temp_dir().join("fast-dav-rs-sync-loop-demo.token"));

    // Fixture data: a calendar with two events (idempotent re-runs).
    let mk = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop><D:displayname>sync-loop example</D:displayname></D:prop></D:set>
</C:mkcalendar>"#;
    let _ = client.mkcalendar(COLLECTION, mk).await?;
    client
        .put(
            &format!("{COLLECTION}seed-1.ics"),
            Bytes::from(event_ics("seed-1@example.com", "seed one")),
        )
        .await?;
    client
        .put(
            &format!("{COLLECTION}seed-2.ics"),
            Bytes::from(event_ics("seed-2@example.com", "seed two")),
        )
        .await?;

    // 1. Initial sync: full snapshot of the collection.
    let session = client
        .sync_session(COLLECTION)
        .with_sync_token(store.load().as_deref());
    let snapshot = session.initial().await?;
    println!(
        "initial: {} items, token = {:?}",
        snapshot.items.len(),
        snapshot.sync_token
    );
    for entry in &snapshot.items {
        print_entry(&entry.href, entry.data.as_deref());
    }
    store.save(snapshot.sync_token.as_deref());

    // 2. Change the server state, then run an incremental sync.
    let seed_etag = snapshot
        .items
        .iter()
        .find(|e| e.href.ends_with("seed-1.ics"))
        .and_then(|e| e.etag.clone())
        .expect("seed etag");
    client
        .put(
            &format!("{COLLECTION}added.ics"),
            Bytes::from(event_ics("added@example.com", "added later")),
        )
        .await?;
    let edited = client
        .put_if_match(
            &format!("{COLLECTION}seed-1.ics"),
            Bytes::from(event_ics("seed-1@example.com", "seed one (edited)")),
            &seed_etag,
        )
        .await?;
    println!(
        "edit seed-1 (If-Match {}): {}",
        &seed_etag[..12.min(seed_etag.len())],
        edited.status()
    );

    let delta = session.incremental().await?;
    println!(
        "incremental: +{} ~{} -{} (resynced = {})",
        delta.added.len(),
        delta.modified.len(),
        delta.deleted.len(),
        delta.resynced
    );
    for entry in delta.added.iter().chain(&delta.modified) {
        print_entry(&entry.href, entry.data.as_deref());
    }
    for href in &delta.deleted {
        println!("  deleted: {href}");
    }
    store.save(delta.sync_token.as_deref());

    // 3. Stale token (e.g. the server dropped its sync cache): the session
    //    resets to a full initial sync, flagged `resynced`. Rebuild caches
    //    from `delta.added`; per RFC 6578 §3.4 the delta carries no deletions.
    let stale = client
        .sync_session(COLLECTION)
        .with_sync_token(Some("token-the-server-never-issued"));
    let delta = stale.incremental().await?;
    println!(
        "stale-token sync: resynced = {}, {} items in `added`",
        delta.resynced,
        delta.added.len()
    );

    // Cleanup so re-runs start from a known state.
    client.delete(COLLECTION).await?;
    let _ = std::fs::remove_file(&store.0);
    println!("done");
    Ok(())
}
