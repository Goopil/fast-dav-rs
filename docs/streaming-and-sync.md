# Streaming & Sync

- Use `caldav::parse_multistatus_stream` for CalDAV responses and `carddav::parse_multistatus_stream`
  for CardDAV responses.
- `supports_webdav_sync` and `sync_collection` work for both calendars and addressbooks.
- `sync_collection_with_level` (all clients) sends a configurable `sync-level` (RFC 6578 §3.3):
  `SyncLevel::One` restricts the sync to the collection members, `SyncLevel::Infinite` includes
  all descendants.
- `sync_collection_resilient` (all clients) recovers automatically from a stale sync token —
  `410 Gone` (RFC 6578 §3.11) or `403 Forbidden` + `valid-sync-token` (§3.2) — by re-issuing the
  report as an initial sync and returning the full result set with the new token; any other error
  propagates unchanged. The response is flagged: the `WebDavClient` variant returns a 4-tuple whose
  last element is the `resynced` flag, and `caldav::SyncResponse`/`carddav::SyncResponse` expose
  `resynced == true`. Per RFC 6578 §3.4 an initial sync MUST NOT report deletions that predate the
  stale token, so rebuild your caches from `items` instead of applying them incrementally when the
  flag is set.
- **Result truncation (RFC 6578 §3.6):** when the server truncates a sync result set it reports
  `507 Insufficient Storage` inside the 207 multistatus (normally on the request-URI).
  `caldav::SyncResponse`/`carddav::SyncResponse` expose this as `truncated == true`; the 507
  element still appears in `items` with its per-item status, and the returned `sync_token`
  stays valid for fetching the next page. At the `WebDavClient` level, inspect `items` for a
  `HTTP/1.1 507 …` status.
- The sync types and helpers (`SyncItem`, `SyncResponse`, `build_sync_collection_body`,
  `map_sync_response`) are **module-qualified**: CalDAV and CardDAV define distinct same-named
  items, so they are only available as `fast_dav_rs::caldav::{SyncItem, SyncResponse, …}` and
  `fast_dav_rs::carddav::{SyncItem, SyncResponse, …}` — never at the crate root.

### SyncSession (stateful sync with transparent fallback)

`SyncSession` (new in this release, issue #160) packages the sync algorithm
above into a per-collection, in-memory state machine — the DAVx⁵ approach:

1. it probes `supported-report-set` **once** and caches the answer;
2. while the server supports RFC 6578 `sync-collection`, `initial()` returns
   the full state snapshot and `incremental()` returns a typed delta
   (`added` / `modified` / `deleted`) carrying the token to persist; 507
   result-set truncation is continued transparently;
3. on an unsupported server (or one that rejects the report with `403`/`405`)
   it falls back transparently to a `PROPFIND Depth: 1` etag diff, fetching
   content for changed members via batched `calendar-multiget` /
   `addressbook-multiget` REPORTs (CalDAV/CardDAV sessions);
4. a stale token — `410 Gone`, or `403` + `valid-sync-token` as observed on
   Radicale — resets the session transparently to a full initial sync,
   flagged `resynced == true` (rebuild caches; per RFC 6578 §3.4 the delta
   then reports no deletions);
5. conflicts: the server wins.

The session is in-memory only: **you** persist `sync_token` between runs
(store it next to your application data) and restore it with
`with_sync_token`. Clones share the token and the probe cache, like client
clones share the connection pool.

```rust
use fast_dav_rs::{CalDavClient, Result, SyncSession};

async fn sync_loop(client: &CalDavClient, saved_token: Option<&str>) -> Result<()> {
    // Restore the persisted token from your own storage when resuming.
    let session = client
        .sync_session("calendars/alice/work/")
        .with_sync_token(saved_token);

    let delta = session.incremental().await?;
    if delta.resynced {
        // Stale token: this is a full snapshot — rebuild your cache from
        // `delta.added` instead of applying it incrementally.
        println!("resync: {} live items", delta.added.len());
    }
    for entry in delta.added.iter().chain(&delta.modified) {
        println!("upsert {} (etag {:?})", entry.href, entry.etag);
    }
    for href in &delta.deleted {
        println!("remove {href}");
    }
    println!("persist this token: {:?}", delta.sync_token);
    Ok(())
}
```

A plain `WebDavClient::sync_session(collection)` requests `getetag` only;
the `CalDavClient`/`CardDavClient` constructors also fetch
`calendar-data`/`address-data` for every entry (and via multiget on the
fallback path).

A runnable end-to-end version — initial + incremental + stale-token resync
against the Radicale fixture, with `calendar-data` parsed by the `icalendar`
crate and a file-based token store — lives in
[`examples/sync_loop.rs`](../examples/sync_loop.rs).

### WebDAV locking (class 2)

All clients (`WebDavClient`, `CalDavClient`, `CardDavClient`) support WebDAV locking (RFC 4918
class 2). `lock` sends `LOCK` with an explicit `Depth: 0` header (RFC 4918 §9.10.4), a
`Timeout: Second-N` header (clamped to `u32::MAX` seconds, RFC 4918 §10.7) and a `<D:lockinfo>`
body and returns the parsed `<D:activelock>` (`LockInfo`: token, timeout, scope, owner, lockroot,
depth); `refresh_lock` re-issues the `LOCK` with the token in an `If` header (RFC 4918 §9.10.7)
and falls back to the request token when the server omits `<D:locktoken>` in the response;
`unlock` sends `UNLOCK` with the token in a `Lock-Token` header. Tokens are validated
(RFC 4918 §10.5 Coded-URL grammar) before being embedded in a header. Non-success statuses
surface as `Error::UnexpectedStatus` with `Operation::Lock`/`Operation::Unlock` — or as
`Error::UnexpectedStatusWithDav` when the error body carries a `<D:error>` precondition (e.g.
`423 Locked` + `no-conflicting-lock`, RFC 4918 §16). A successful `LOCK` response without a lock
token fails with `Error::InvalidInput` (RFC 4918 §9.10.9).

The client keeps **no implicit lock state**: callers keep the token and pass it to
`refresh_lock`/`unlock`, or send it in an `If` header via the low-level `send` on conditional
writes. Check `capabilities()` (`class2`) to confirm the server supports locking. `PROPFIND`
responses containing the `lockdiscovery` property can be parsed with
`webdav::parse_lock_discovery_bytes`.

```rust
use fast_dav_rs::webdav::LockScope;
use fast_dav_rs::{CalDavClient, Result};

async fn edit_shared_doc(client: &CalDavClient) -> Result<()> {
    let lock = client
        .lock(
            "docs/plan.txt",
            LockScope::Exclusive,
            "<D:href>https://example.com/alice</D:href>",
            Some(300),
        )
        .await?;

    // Write while holding the lock: the token goes in an If header.
    let mut headers = hyper::HeaderMap::new();
    headers.insert("If", format!("(<{}>)", lock.token).parse().expect("valid coded-URL token"));
    client
        .send(
            hyper::Method::PUT,
            "docs/plan.txt",
            headers,
            Some(bytes::Bytes::from_static(b"updated content")),
            None,
        )
        .await?;

    client.refresh_lock("docs/plan.txt", &lock.token, Some(300)).await?;
    client.unlock("docs/plan.txt", &lock.token).await?;
    Ok(())
}
```

### Resilient sync example

```rust
use fast_dav_rs::{CalDavClient, Result, SyncLevel};

async fn sync(client: &CalDavClient) -> Result<()> {
    // Incremental sync; on 410 Gone the report is re-issued as an initial sync
    // and the full result set with the new token is returned.
    let sync = client
        .sync_collection_resilient("calendars/alice/work/", Some("stale-token"), None, true)
        .await?;
    println!("new token: {:?}", sync.sync_token);

    // Custom sync-level (RFC 6578 §3.3).
    let full = client
        .sync_collection_with_level("calendars/alice/work/", None, None, false, SyncLevel::Infinite)
        .await?;
    println!("items: {}", full.items.len());

    Ok(())
}
```

### CalDAV streaming example

```rust,no_run
use fast_dav_rs::{CalDavClient, Depth, Result, detect_encoding};
use fast_dav_rs::caldav::parse_multistatus_stream;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;
    let propfind_xml = r#"<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop></D:propfind>"#;

    let response = client.propfind_stream("calendars/alice/work/", Depth::One, propfind_xml).await?;
    let encoding = detect_encoding(response.headers());
    let parsed = parse_multistatus_stream(response.into_body(), &[encoding]).await?;

    for item in parsed.items {
        if let Some(data) = item.calendar_data {
            println!("{} -> {} bytes", item.href, data.len());
        }
    }

    Ok(())
}
```

### CardDAV streaming example

```rust,no_run
use fast_dav_rs::{CardDavClient, Depth, Result, detect_encoding};
use fast_dav_rs::carddav::parse_multistatus_stream;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CardDavClient::new("https://carddav.example.com/users/alice/", None, None)?;
    let report_xml = r#"<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav"><D:prop><D:getetag/><C:address-data/></D:prop></C:addressbook-query>"#;

    let response = client.report_stream("addressbooks/alice/team/", Depth::One, report_xml).await?;
    let encoding = detect_encoding(response.headers());
    let parsed = parse_multistatus_stream(response.into_body(), &[encoding]).await?;

    for item in parsed.items {
        if let Some(data) = item.address_data {
            println!("{} -> {} bytes", item.href, data.len());
        }
    }

    Ok(())
}
```
