# Streaming & Sync

- Stream CalDAV/CardDAV responses item by item with the `*_items_stream` methods
  (`propfind_items_stream`, `report_items_stream`, `calendar_query_stream`,
  `addressbook_query_stream`). The low-level `parse_multistatus_stream*` parsers are deprecated
  since 0.18.0.
- `supports_webdav_sync` and `sync_collection` work for both calendars and addressbooks.
- `sync_collection_with_level` (all clients) sends a configurable `sync-level` (RFC 6578 §3.3):
  `SyncLevel::One` restricts the sync to the collection members, `SyncLevel::Infinite` includes
  all descendants.
- `SyncSession` (all clients) is the resilient sync engine: it recovers automatically from a stale
  sync token — `410 Gone` (RFC 6578 §3.11) or `403 Forbidden` + `valid-sync-token` (§3.2) — by
  re-issuing the report as an initial sync, and the delta it yields carries `resynced == true` on
  the rebuild. Per RFC 6578 §3.4 an initial sync MUST NOT report deletions that predate the stale
  token, so rebuild your caches from the delta instead of applying it incrementally when the flag
  is set. The raw `sync_collection_resilient` methods are deprecated since 0.18.0 in favour of
  `SyncSession`.
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

### Item-by-item streams (no aggregation)

The `*_items_stream` methods parse the multistatus **incrementally** over the response
body — each complete `<D:response>` is yielded as soon as it parses, so memory stays
bounded by the current item regardless of collection size (the XML is also decompressed
on the fly, br/gzip/zstd). At the `WebDavClient` level, `propfind_items_stream` and
`report_items_stream` (plus `*_with_timeout` variants) yield `DavStreamEvent` items and
the `<D:sync-token>` in document order; a non-success status is rejected eagerly by the
call itself as `Error::UnexpectedStatus`. **Dropping the stream aborts the download**
and frees (rather than re-pools) the connection.

```rust,no_run
use fast_dav_rs::{CalDavClient, Result};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;

    // Each CalendarObject arrives as soon as its <D:response> is parsed.
    let mut stream = client
        .calendar_query_stream("calendars/alice/work/", "VEVENT", None, None, true, None)
        .await?;
    while let Some(object) = stream.next().await {
        let object = object?;
        if let Some(data) = &object.calendar_data {
            println!("{} -> {} bytes", object.href, data.len());
        }
    }

    Ok(())
}
```

`CardDavClient::addressbook_query_stream` works the same way for vCards, and the raw
event engine behind them is available as
`fast_dav_rs::webdav::streaming::multistatus_events` (an idle-read timeout variant,
`multistatus_events_with_timeout`, is there too).

### SyncSession (stateful sync with transparent fallback)

`SyncSession` (new in this release, issue #160) packages the sync algorithm
above into a per-collection, in-memory state machine — the DAVx⁵ approach:

1. it probes `supported-report-set` **once** and caches the answer;
2. while the server supports RFC 6578 `sync-collection`, `initial()` returns
   the full state snapshot and `incremental()` returns a typed delta
   (`added` / `modified` / `deleted`) carrying the token to persist; 507
   result-set truncation is continued with the page token, and a truncation
   that cannot be continued (no new token, or a repeated token) fails with
   `Error::SyncIncomplete` instead of surfacing a partial delta;
3. on an unsupported server (or one that rejects the report with `405`) it
   falls back transparently to a `PROPFIND Depth: 1` etag diff, fetching
   content for changed members via batched `calendar-multiget` /
   `addressbook-multiget` REPORTs (CalDAV/CardDAV sessions); a bare `403`
   propagates as an error instead of downgrading (a transient ACL flap must
   not silently pin the session to the slow path — the next call re-attempts
   `sync-collection`);
4. a stale token — `410 Gone`, or `403` + `valid-sync-token` as observed on
   Radicale — resets the session transparently to a full initial sync,
   flagged `resynced == true` (rebuild caches; per RFC 6578 §3.4 the delta
   then reports no deletions);
5. conflicts: the server wins.

The session is in-memory only: **you** persist `sync_token` between runs
(store it next to your application data) and restore it with
`with_sync_token`. Clones share the token and the probe cache, like client
clones share the connection pool, and concurrent `initial()`/`incremental()`
calls on clones are serialized (single-flight): one probe and one report at
a time, with the session state transitions kept consistent.

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

`sync_collection_resilient` is deprecated since 0.18.0 — prefer `SyncSession`
(described above). The raw method remains available:

```rust
use fast_dav_rs::{CalDavClient, Result, SyncLevel};

// `sync_collection_resilient` is deprecated; `SyncSession` is the supported path.
#[allow(deprecated)]
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
use fast_dav_rs::{CalDavClient, Result};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CalDavClient::new("https://caldav.example.com/users/alice/", None, None)?;

    // Each parsed object arrives as soon as its <D:response> completes;
    // dropping the stream aborts the download.
    let mut stream = client
        .calendar_query_stream("calendars/alice/work/", "VEVENT", None, None, true, None)
        .await?;

    while let Some(object) = stream.next().await {
        let object = object?;
        if let Some(data) = object.calendar_data {
            println!("{} -> {} bytes", object.href, data.len());
        }
    }

    Ok(())
}
```

### CardDAV streaming example

```rust,no_run
use fast_dav_rs::{CardDavClient, Result};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    let client = CardDavClient::new("https://carddav.example.com/users/alice/", None, None)?;
    let filter_xml = r#"<C:filter><C:prop-filter name="FN"><C:text-match collation="i;unicode-casemap">Ada</C:text-match></C:prop-filter></C:filter>"#;

    let mut stream = client
        .addressbook_query_stream("addressbooks/alice/team/", filter_xml, true)
        .await?;

    while let Some(object) = stream.next().await {
        let object = object?;
        if let Some(data) = object.address_data {
            println!("{} -> {} bytes", object.href, data.len());
        }
    }

    Ok(())
}
```
