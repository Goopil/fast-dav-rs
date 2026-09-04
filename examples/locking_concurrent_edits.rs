//! WebDAV locking lifecycle (RFC 4918 class 2): `lock` → edit under the lock
//! token → `refresh_lock` → `unlock`, plus a token-less write correctly
//! rejected with `423`.
//!
//! Target fixture: **SabreDAV** (`sabredav-test/`, Basic auth `test`/`test`),
//! a real class-2 server with the PDO locks backend.
//!
//! ```sh
//! ./sabredav-test/setup.sh        # http://localhost:8080
//! cargo run --example locking_concurrent_edits
//! ```
//!
//! Portability note: not every DAV server implements `LOCK`. Radicale, for
//! example, advertises class 2 but answers `LOCK` with `405` — the example
//! fails gracefully in that case (see the match at the bottom of `run`) and
//! you should fall back to etag-conditional writes (`put_if_match`).

use bytes::Bytes;
use fast_dav_rs::{CalDavClient, Error, LockScope, Operation};

const USER: &str = "test";
const PASS: &str = "test";

fn sabredav_url() -> String {
    let mut url = std::env::var("SABREDAV_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

fn event_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//locking example//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260913T100000Z\r\n\
         DTEND:20260913T110000Z\r\n\
         SUMMARY:{summary}\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR"
    )
}

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = CalDavClient::new(&sabredav_url(), Some(USER), Some(PASS))?;

    // Lock-capability check first: a cheap OPTIONS probe.
    let probe = fast_dav_rs::WebDavClient::new(&sabredav_url(), Some(USER), Some(PASS))?;
    let caps = probe.capabilities("").await?;
    if !caps.class2 {
        println!("this server does not advertise locking (class 2) — nothing to demo");
        return Ok(());
    }

    // The example works on any class-2 server regardless of collection
    // layout: discover the calendar home set and create the calendar there.
    let principal = client
        .discover_current_user_principal()
        .await?
        .expect("server advertises a current-user-principal");
    let home = client
        .discover_calendar_home_set(&principal)
        .await?
        .into_iter()
        .next()
        .expect("server advertises a calendar home set");
    let calendar_path = format!("{}example-locking/", home.trim_start_matches('/'));

    let mk = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop><D:displayname>locking example</D:displayname></D:prop></D:set>
</C:mkcalendar>"#;
    let _ = client.mkcalendar(&calendar_path, mk).await?;
    let event_path = format!("{calendar_path}contended.ics");
    client
        .put(
            &event_path,
            Bytes::from(event_ics("lock-demo@example.com", "original")),
        )
        .await?;
    // Read the etag back: PUT responses do not always carry an ETag header.
    let etag = fast_dav_rs::webdav::etag_from_headers(client.get(&event_path).await?.headers())
        .expect("GET returns an ETag");

    match run(&client, &event_path, &etag).await {
        Ok(()) => {}
        // Graceful no-LOCK handling (e.g. Radicale answers 405 despite
        // advertising class 2): fall back to etag-only conditional writes.
        Err(Error::UnexpectedStatus {
            operation: Operation::Lock,
            status,
            ..
        }) => {
            println!(
                "LOCK rejected with {status}: no locking on this server; use put_if_match instead"
            );
        }
        Err(other) => {
            client.delete(&calendar_path).await?;
            return Err(other);
        }
    }

    client.delete(&calendar_path).await?;
    println!("done");
    Ok(())
}

async fn run(client: &CalDavClient, event_path: &str, etag: &str) -> fast_dav_rs::Result<()> {
    // 1. Acquire an exclusive write lock on the event resource.
    let lock = client
        .lock(
            event_path,
            LockScope::Exclusive,
            "<D:href>fast-dav-rs example</D:href>",
            Some(60),
        )
        .await?;
    println!(
        "locked: token {} for {}s",
        lock.token,
        lock.timeout_secs.unwrap_or(0)
    );

    // 2. A peer's write without the lock token is rejected (423 Locked):
    //    the client keeps no implicit lock state, so the token must be sent.
    let peer = client
        .put_if_match(
            event_path,
            Bytes::from(event_ics("lock-demo@example.com", "peer edit")),
            etag,
        )
        .await?;
    println!(
        "peer write without the token: {} (rejected while locked)",
        peer.status()
    );

    // 3. Our edit under the lock: the token goes in an If header alongside
    //    the If-Match etag (low-level send — put_if_match cannot add headers).
    //    Entity-tags must be quoted (RFC 9110 §8.8.3): the library's
    //    put_if_match does that; done by hand here.
    let mut headers = hyper::HeaderMap::new();
    headers.insert(
        "If",
        format!("(<{}>)", lock.token).parse().expect("valid token"),
    );
    headers.insert(
        "If-Match",
        format!("\"{etag}\"").parse().expect("valid etag"),
    );
    let ours = client
        .send(
            hyper::Method::PUT,
            event_path,
            headers,
            Some(Bytes::from(event_ics(
                "lock-demo@example.com",
                "edited under lock",
            ))),
            None,
        )
        .await?;
    println!("edit under the lock: {}", ours.status());

    // 4. Long-running work? Refresh before the timeout lapses (may rotate
    //    the token and always re-grants the timeout).
    let refreshed = client
        .refresh_lock(event_path, &lock.token, Some(60))
        .await?;
    println!(
        "refreshed: {}s remaining",
        refreshed.timeout_secs.unwrap_or(0)
    );

    // 5. Release.
    client.unlock(event_path, &lock.token).await?;
    println!("unlocked");
    Ok(())
}
