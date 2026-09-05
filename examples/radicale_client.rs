//! Working with a "no-LOCK" provider, using Radicale as the fixture: a
//! server that advertises WebDAV class 2 but answers `LOCK` with `405`, plus
//! the `SyncSession` behavior differences on such servers.
//!
//! Target fixture: **Radicale** (`radicale-test/`, Basic auth `test`/`test`).
//!
//! ```sh
//! ./radicale-test/setup.sh        # start + seed the fixture on http://localhost:8081
//! cargo run --example radicale_client
//! ```
//!
//! Two lessons this server teaches:
//!
//! 1. **Compliance classes can lie a little.** Radicale advertises
//!    `DAV: 1, 2, 3` on `OPTIONS /`, yet a `LOCK` request is rejected with
//!    `405 Method Not Allowed`. Probe capabilities with `capabilities()`, but
//!    treat `LOCK` failures as a graceful "no locking here" signal and fall
//!    back to etag-only conditional writes (`put_if_match`).
//! 2. **Sync without (or with volatile) server tokens.** When the server has
//!    no `sync-collection` support the session transparently falls back to a
//!    `PROPFIND Depth: 1` etag diff and `sync_token` stays `None` — every
//!    sync is a full list. Radicale *does* support `sync-collection`, but its
//!    tokens are volatile (a data wipe invalidates them with
//!    `403 + valid-sync-token`), so the stale-token reset path is exercised
//!    below as well.

#[path = "common/mod.rs"]
mod common;

use fast_dav_rs::{Error, LockScope, Operation};

use common::{radicale_client, radicale_webdav_client};

const COLLECTION: &str = "test/example-radicale-client/";

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = radicale_client()?;
    // Capability probes live on the underlying WebDAV client type.
    let probe = radicale_webdav_client()?;

    // Fixture data (idempotent re-runs).
    let mk = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop><D:displayname>radicale-client example</D:displayname></D:prop></D:set>
</C:mkcalendar>"#;
    let _ = client.mkcalendar(COLLECTION, mk).await?;

    // 1. What does the server advertise?
    let caps = probe.capabilities("").await?;
    println!("DAV compliance: {:?}", caps.compliance());

    // 2. RFC 6578 probe for this collection.
    let sync = probe.supports_webdav_sync_on(COLLECTION).await?;
    println!("sync-collection support: {sync:?}");

    // 3. SyncSession: full snapshot, then a stale-token incremental that the
    //    session resets transparently (Radicale's 403 + valid-sync-token).
    let session = client.sync_session(COLLECTION);
    let snapshot = session.initial().await?;
    println!(
        "initial: {} items, token = {:?} (None would mean full-list fallback on every sync)",
        snapshot.items.len(),
        snapshot.sync_token
    );

    let stale = client
        .sync_session(COLLECTION)
        .with_sync_token(Some("token-issued-before-a-cache-wipe"));
    let delta = stale.incremental().await?;
    if delta.resynced {
        println!(
            "stale token: transparent full resync, {} items — rebuild caches",
            delta.added.len()
        );
    }

    // 4. The no-LOCK part: a class-2-advertising server that still says 405.
    match client
        .lock(
            COLLECTION,
            LockScope::Exclusive,
            "<D:href>demo</D:href>",
            Some(60),
        )
        .await
    {
        Ok(lock) => {
            println!("locking available: token {}", lock.token);
            client.unlock(COLLECTION, &lock.token).await?;
        }
        // Radicale answers 405 Method Not Allowed (no LOCK implementation).
        Err(Error::UnexpectedStatus {
            operation: Operation::Lock,
            status,
            ..
        }) => {
            println!(
                "LOCK rejected with {status}: this server does not implement locking \
                 despite its compliance header — use etag-conditional writes \
                 (put_if_match) instead of locks"
            );
        }
        // Servers with a <D:error> body (e.g. 423 + no-conflicting-lock).
        Err(Error::UnexpectedStatusWithDav {
            operation: Operation::Lock,
            status,
            dav,
            ..
        }) => {
            println!(
                "LOCK rejected with {status}: {}",
                dav.precondition_code
                    .as_deref()
                    .unwrap_or("(no precondition)")
            );
        }
        Err(other) => return Err(other),
    }

    // Cleanup.
    client.delete(COLLECTION).await?;
    println!("done");
    Ok(())
}
