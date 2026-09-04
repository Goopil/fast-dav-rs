//! Stateful sync sessions (RFC 6578) with transparent full-list fallback.
//!
//! [`SyncSession`] wraps a [`WebDavClient`] plus one collection path and
//! implements the DAVx⁵ sync algorithm:
//!
//! 1. probe `supported-report-set` once (cached for the session's lifetime);
//! 2. when `sync-collection` is supported: `initial()` is a sync-collection
//!    with an empty token, `incremental()` carries the stored token;
//! 3. when it is not (probe-negative, or the server rejects the report with
//!    `403`/`405`): transparent fallback to a `PROPFIND Depth: 1` etag list
//!    diffed against the cached previous state, with content fetched via
//!    batched multiget REPORTs;
//! 4. a stale token (`410 Gone`, or `403` + the `valid-sync-token`
//!    precondition) triggers a transparent reset to a full initial sync,
//!    flagged via [`SyncDelta::resynced`];
//! 5. conflicts: the server wins (no client-side merge logic).
//!
//! The session is in-memory only: the **caller** persists
//! [`SyncSnapshot::sync_token`] / [`SyncDelta::sync_token`] between runs and
//! hands it back via [`SyncSession::with_sync_token`].

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use hyper::StatusCode;
use parking_lot::Mutex;

use crate::webdav::client::WebDavClient;
use crate::webdav::streaming::parse_multistatus_bytes;
use crate::webdav::types::SyncRow;
use crate::webdav::types::{DavItem, Depth, SyncCapability, http_status_code, map_sync_rows};
use crate::webdav::xml::build_multiget_body;
use crate::{Error, Operation, Result};

/// Hrefs per multiget REPORT in the fallback content fetch (matches the
/// documented `calendar_multiget_many` batch-size example).
const MULTIGET_BATCH_SIZE: usize = 100;
/// Concurrent multiget REPORTs in the fallback content fetch.
const MULTIGET_CONCURRENCY: usize = 4;

/// One entry of a sync snapshot or delta: the member href, its entity-tag,
/// and — when the session fetches data (CalDAV/CardDAV sessions, or the
/// fallback multiget) — the resource body.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SyncEntry {
    /// Member href as returned by the server (path or absolute URL).
    pub href: String,
    /// Normalized entity-tag; `None` when the server did not return one.
    pub etag: Option<String>,
    /// `calendar-data` / `address-data` payload when requested and returned.
    pub data: Option<String>,
}

/// Full state snapshot returned by [`SyncSession::initial`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SyncSnapshot {
    /// Every live member of the collection (href + etag + optional data).
    pub items: Vec<SyncEntry>,
    /// The sync token to persist; `None` when the session runs on the
    /// full-list fallback (no server-side token).
    pub sync_token: Option<String>,
}

/// Typed delta returned by [`SyncSession::incremental`].
///
/// `added` and `modified` carry the current href + etag (and data when the
/// session fetches it); `deleted` lists the hrefs removed since the last
/// sync. With `resynced == true` the delta is a full rebuild (the stored
/// token was stale): per RFC 6578 §3.4 it MUST NOT report deletions that
/// predate the stale token, so `deleted` is empty and `added` holds the
/// complete current state — rebuild caches instead of applying incrementally.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SyncDelta {
    /// Members created since the last sync (from the session's point of
    /// view: hrefs absent from the session's previous state).
    pub added: Vec<SyncEntry>,
    /// Members whose etag changed since the last sync.
    pub modified: Vec<SyncEntry>,
    /// Hrefs removed since the last sync (empty when `resynced`).
    pub deleted: Vec<String>,
    /// The sync token to persist after applying this delta.
    pub sync_token: Option<String>,
    /// `true` when a stale token forced a transparent full resync
    /// (`410 Gone` or `403` + `valid-sync-token`).
    pub resynced: bool,
}

/// Per-protocol constants for the data-fetching parts of a session
/// (sync-collection data property + multiget REPORT shape).
#[derive(Debug, Clone, Copy)]
pub(crate) struct SyncDataSpec {
    /// XML namespace of the data property (CalDAV/CardDAV).
    pub namespace: &'static str,
    /// Data property requested alongside `getetag` (e.g. `calendar-data`).
    pub data_element: &'static str,
    /// Multiget REPORT root element (e.g. `calendar-multiget`).
    pub multiget_root: &'static str,
    /// Operation reported in multiget [`Error::UnexpectedStatus`] errors.
    pub multiget_operation: Operation,
}

pub(crate) const CALENDAR_DATA_SPEC: SyncDataSpec = SyncDataSpec {
    namespace: "urn:ietf:params:xml:ns:caldav",
    data_element: "calendar-data",
    multiget_root: "calendar-multiget",
    multiget_operation: Operation::ReportCalendarMultiget,
};

pub(crate) const ADDRESS_DATA_SPEC: SyncDataSpec = SyncDataSpec {
    namespace: "urn:ietf:params:xml:ns:carddav",
    data_element: "address-data",
    multiget_root: "addressbook-multiget",
    multiget_operation: Operation::ReportAddressbookMultiget,
};

/// Session state shared by all clones of a [`SyncSession`] (like the client's
/// compression cache, clones see each other's last token).
#[derive(Debug, Default)]
struct SessionState {
    /// Cached capability-probe outcome; `None` until first probed.
    capability: Option<SyncCapability>,
    /// Last sync token handed out by the server (or restored by the caller).
    token: Option<String>,
    /// Previous href → etag state, the diff reference for the fallback path
    /// and the added/modified classifier for sync-collection deltas.
    prev: Option<HashMap<String, Option<String>>>,
}

/// In-memory synchronization session for one collection (RFC 6578).
///
/// Holds the collection path, the cached capability probe, the last sync
/// token, and (internally) the previous href→etag state used for the
/// transparent full-list fallback and the added/modified classification.
/// Clones share the token and probe cache, like [`WebDavClient`] clones share
/// the connection pool.
///
/// The session itself persists nothing: store
/// [`SyncSnapshot::sync_token`] / [`SyncDelta::sync_token`] in your own
/// storage and restore it on the next session via [`with_sync_token`]
/// ([`SyncSession::with_sync_token`]).
///
/// # Algorithm (DAVx⁵)
///
/// 1. probe `supported-report-set` once per session;
/// 2. `sync-collection` REPORTs while the server supports them (507
///    result-set truncation is continued transparently);
/// 3. on an unsupported server — or one that rejects the report with `403`/
///    `405` — fall back transparently to `PROPFIND Depth: 1` + etag diff,
///    fetching content for changed members via batched multiget REPORTs;
/// 4. a stale token (`410 Gone`, `403` + `valid-sync-token`) resets to a
///    full initial sync flagged `resynced`;
/// 5. conflicts: the server wins.
///
/// # Examples
///
/// CalDAV (the same shape applies to [`CardDavClient::sync_session`]):
///
/// ```
/// use fast_dav_rs::CalDavClient;
///
/// # async fn example() -> fast_dav_rs::Result<()> {
/// let client = CalDavClient::new("https://cal.example.com/", Some("user"), Some("pass"))?;
/// let session = client.sync_session("calendars/user/work/");
///
/// let snapshot = session.initial().await?;
/// // Persist the token (and, if you want added/modified classification
/// // across process restarts, the snapshot entries) in your own store.
/// let saved_token = snapshot.sync_token.clone();
///
/// let delta = session.incremental().await?;
/// if delta.resynced {
///     // Stale token: the delta is a full snapshot — rebuild your cache
///     // from `delta.added` instead of applying it incrementally.
/// }
/// for entry in delta.added.iter().chain(&delta.modified) {
///     println!("fetch {}", entry.href);
/// }
/// for href in &delta.deleted {
///     println!("remove {href}");
/// }
/// let _ = saved_token;
/// # Ok(())
/// # }
/// ```
///
/// Restoring a persisted token in a new session:
///
/// ```
/// use fast_dav_rs::CardDavClient;
///
/// # async fn example() -> fast_dav_rs::Result<()> {
/// let client = CardDavClient::new("https://contacts.example.com/", None, None)?;
/// let session = client
///     .sync_session("addressbooks/user/contacts/")
///     .with_sync_token(Some("http://example.com/sync/42"));
/// let delta = session.incremental().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct SyncSession {
    client: WebDavClient,
    collection: String,
    /// `Some` for CalDAV/CardDAV sessions: data is fetched alongside etags.
    data: Option<SyncDataSpec>,
    state: Arc<Mutex<SessionState>>,
}

impl SyncSession {
    /// Create a session for `collection` on a raw WebDAV client.
    ///
    /// The session requests `getetag` only; use the
    /// [`CalDavClient::sync_session`] / [`CardDavClient::sync_session`]
    /// constructors for sessions that also fetch the resource data.
    pub fn new(client: WebDavClient, collection: impl Into<String>) -> Self {
        Self {
            client,
            collection: collection.into(),
            data: None,
            state: Arc::new(Mutex::new(SessionState::default())),
        }
    }

    /// Attach the protocol data spec (CalDAV/CardDAV constructors only).
    pub(crate) fn with_data_spec(mut self, spec: SyncDataSpec) -> Self {
        self.data = Some(spec);
        self
    }

    /// The collection path this session synchronizes.
    pub fn collection(&self) -> &str {
        &self.collection
    }

    /// The last sync token stored by this session (shared across clones);
    /// `None` before the first successful sync.
    pub fn sync_token(&self) -> Option<String> {
        self.state.lock().token.clone()
    }

    /// Restore a sync token persisted by the caller (e.g. from a previous
    /// process run) and clear the session's cached previous state, so the
    /// next `incremental()` resumes from the server-side token.
    pub fn with_sync_token(self, token: Option<&str>) -> Self {
        let mut state = self.state.lock();
        state.token = token.map(str::to_owned);
        state.prev = None;
        drop(state);
        self
    }

    /// Full state snapshot of the collection.
    ///
    /// On a sync-capable server this is a `sync-collection` REPORT with an
    /// empty token; on an unsupported server a `PROPFIND Depth: 1` etag list
    /// (plus batched multiget content for data-enabled sessions). The result
    /// is stored as the session's baseline: the following `incremental()`
    /// only reports later changes.
    ///
    /// # Errors
    ///
    /// Propagates the underlying report/PROPFIND error.
    pub async fn initial(&self) -> Result<SyncSnapshot> {
        if self.capability().await != SyncCapability::Unsupported {
            match self.sync_snapshot().await {
                Ok(snapshot) => return Ok(snapshot),
                Err(err) if self.downgrade_on_rejected_report(&err) => {}
                Err(err) => return Err(err),
            }
        }
        self.propfind_snapshot().await
    }

    /// Incremental delta since the last sync (an initial full snapshot when
    /// the session has no prior state).
    ///
    /// The session must have state for a meaningful delta: call
    /// [`initial`](Self::initial) first, or restore a persisted token with
    /// [`with_sync_token`](Self::with_sync_token). Without prior state the
    /// delta is the full current state in `added`.
    ///
    /// # Classification
    ///
    /// Against the session's cached previous state: `added` = hrefs the
    /// session has not seen, `modified` = known hrefs with a changed etag,
    /// `deleted` = previously known hrefs missing from the server answer.
    /// A restored session that only carries a persisted token (no cached
    /// state) classifies every server-reported change as `added`.
    ///
    /// # Errors
    ///
    /// Propagates the underlying report/PROPFIND error.
    pub async fn incremental(&self) -> Result<SyncDelta> {
        if self.capability().await != SyncCapability::Unsupported {
            match self.sync_collection_delta().await {
                Ok(delta) => return Ok(delta),
                Err(err) if self.downgrade_on_rejected_report(&err) => {}
                Err(err) => return Err(err),
            }
        }
        self.propfind_delta().await
    }

    /// Capability probe with session-lifetime caching (shared by clones).
    async fn capability(&self) -> SyncCapability {
        if let Some(cap) = self.state.lock().capability {
            return cap;
        }
        let cap = self
            .client
            .supports_webdav_sync_on(&self.collection)
            .await
            .unwrap_or(SyncCapability::Unknown);
        self.state.lock().capability = Some(cap);
        cap
    }

    /// A `403`/`405` from the sync-collection report means the server does
    /// not honor the report (despite the probe, or after a retry): pin the
    /// capability to `Unsupported` and take the full-list path.
    fn downgrade_on_rejected_report(&self, err: &Error) -> bool {
        let rejected = matches!(
            err,
            Error::UnexpectedStatus {
                status: StatusCode::FORBIDDEN | StatusCode::METHOD_NOT_ALLOWED,
                ..
            }
        );
        if rejected {
            self.state.lock().capability = Some(SyncCapability::Unsupported);
        }
        rejected
    }

    /// Initial snapshot via `sync-collection` with an empty token.
    async fn sync_snapshot(&self) -> Result<SyncSnapshot> {
        let (rows, token, _) = self.sync_pages(None).await?;
        let items: Vec<SyncEntry> = rows.into_iter().map(entry_from_row).collect();
        let prev: HashMap<String, Option<String>> = items
            .iter()
            .map(|entry| (entry.href.clone(), entry.etag.clone()))
            .collect();
        {
            let mut state = self.state.lock();
            state.token = token.clone();
            state.prev = Some(prev);
        }
        Ok(SyncSnapshot {
            items,
            sync_token: token,
        })
    }

    /// Incremental delta via `sync-collection` with the stored token.
    /// Result-set truncation (RFC 6578 §3.6, a 507 status inside the
    /// multistatus) is continued with the returned token until a page
    /// arrives without truncation (or stops handing out new tokens).
    async fn sync_collection_delta(&self) -> Result<SyncDelta> {
        let start = self.state.lock().token.clone();
        let (rows, token, resynced) = self.sync_pages(start.clone()).await?;
        let prev = self.state.lock().prev.clone();
        // A response to an initial sync (no prior token) or to a stale-token
        // resync is the full current state: replace the cached state instead
        // of merging. Per RFC 6578 §3.4 initial-sync answers carry no
        // deletion markers, so `deleted` stays empty there.
        let replace = resynced || start.is_none();
        let diff = diff_rows(&prev, rows, replace);
        {
            let mut state = self.state.lock();
            state.token = token.clone();
            state.prev = Some(diff.next);
        }
        Ok(SyncDelta {
            added: diff.added,
            modified: diff.modified,
            deleted: diff.deleted,
            sync_token: token,
            resynced,
        })
    }

    /// Run `sync-collection` REPORTs until the result set is complete,
    /// following 507 truncation pages. Returns the flattened rows, the last
    /// sync token, and whether a stale-token resync happened.
    async fn sync_pages(
        &self,
        start: Option<String>,
    ) -> Result<(Vec<SyncRow>, Option<String>, bool)> {
        let mut token = start;
        let mut rows: Vec<SyncRow> = Vec::new();
        let mut resynced = false;
        let mut final_token: Option<String> = None;
        loop {
            let (headers, items, top_token, was_resync) = self
                .client
                .sync_collection_resilient(
                    &self.collection,
                    token.as_deref(),
                    None,
                    self.data.is_some(),
                    self.data.map_or("DAV:", |spec| spec.namespace),
                    self.data.map_or("", |spec| spec.data_element),
                )
                .await?;
            resynced |= was_resync;
            let (page_token, page_rows, truncated) =
                map_sync_rows(&headers, items, top_token, extract_data);
            rows.extend(page_rows);
            if page_token.is_some() {
                final_token = page_token.clone();
            }
            match (truncated, page_token) {
                // Continue with the page token; a server that repeats the
                // same (or no) token cannot be continued — stop instead of
                // looping forever.
                (true, Some(next)) if Some(&next) != token.as_ref() => token = Some(next),
                _ => return Ok((dedup_rows(rows), final_token, resynced)),
            }
        }
    }

    /// Initial snapshot via the full-list fallback: `PROPFIND Depth: 1`
    /// etag list, content fetched via batched multiget REPORTs.
    async fn propfind_snapshot(&self) -> Result<SyncSnapshot> {
        let current = self.propfind_state().await?;
        let mut items: Vec<SyncEntry> = current
            .iter()
            .map(|(href, etag)| SyncEntry {
                href: href.clone(),
                etag: etag.clone(),
                data: None,
            })
            .collect();
        items.sort_by(|a, b| a.href.cmp(&b.href));
        if let Some(spec) = self.data {
            let hrefs: Vec<String> = items.iter().map(|e| e.href.clone()).collect();
            let data = self.multiget_data(&hrefs, spec).await?;
            for item in &mut items {
                item.data = data.get(&item.href).cloned();
            }
        }
        let token = {
            let mut state = self.state.lock();
            state.prev = Some(current);
            state.token.clone()
        };
        Ok(SyncSnapshot {
            items,
            sync_token: token,
        })
    }

    /// Incremental delta via the full-list fallback: diff the `PROPFIND
    /// Depth: 1` etag list against the cached previous state, then fetch
    /// content for added/modified members via batched multiget REPORTs.
    async fn propfind_delta(&self) -> Result<SyncDelta> {
        let current = self.propfind_state().await?;
        let prev = self.state.lock().prev.clone();
        let (mut added, mut modified, deleted) = diff_maps(&prev, &current);
        if let Some(spec) = self.data {
            let hrefs: Vec<String> = added
                .iter()
                .chain(modified.iter())
                .map(|entry| entry.href.clone())
                .collect();
            if !hrefs.is_empty() {
                let data = self.multiget_data(&hrefs, spec).await?;
                for entry in added.iter_mut().chain(modified.iter_mut()) {
                    entry.data = data.get(&entry.href).cloned();
                }
            }
        }
        let token = {
            let mut state = self.state.lock();
            state.prev = Some(current);
            state.token.clone()
        };
        Ok(SyncDelta {
            added,
            modified,
            deleted,
            sync_token: token,
            resynced: false,
        })
    }

    /// Full etag list of the collection (`PROPFIND Depth: 1`), excluding the
    /// collection entry itself and error-status members.
    async fn propfind_state(&self) -> Result<HashMap<String, Option<String>>> {
        const PROPFIND_ETAG_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getetag/>
  </D:prop>
</D:propfind>"#;
        let resp = self
            .client
            .propfind(&self.collection, Depth::One, PROPFIND_ETAG_BODY)
            .await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindCollections,
                status: resp.status(),
            });
        }
        let parsed = parse_multistatus_bytes(resp.body())?;
        Ok(parsed
            .items
            .into_iter()
            .filter(|item| !item.is_collection && !has_error_status(item.status.as_deref()))
            .map(|item| (item.href, item.etag))
            .collect())
    }

    /// Fetch resource bodies for `hrefs` via batched multiget REPORTs
    /// (chunked, concurrency-bounded). Any failed chunk fails the call so
    /// the session state stays unchanged and the caller can retry cleanly.
    async fn multiget_data(
        &self,
        hrefs: &[String],
        spec: SyncDataSpec,
    ) -> Result<HashMap<String, String>> {
        let requests: Vec<(String, Arc<Bytes>)> = hrefs
            .chunks(MULTIGET_BATCH_SIZE)
            .filter_map(|chunk| {
                build_multiget_body(
                    chunk.iter(),
                    true,
                    spec.namespace,
                    spec.multiget_root,
                    spec.data_element,
                    None,
                )
                .map(|xml| (self.collection.clone(), Arc::new(Bytes::from(xml))))
            })
            .collect();
        if requests.is_empty() {
            return Ok(HashMap::new());
        }

        let batches = self
            .client
            .report_many_bodies(requests, MULTIGET_CONCURRENCY)
            .await;
        let mut out = HashMap::new();
        for batch in batches {
            let resp = batch.result?;
            if !resp.status().is_success() {
                return Err(Error::UnexpectedStatus {
                    operation: spec.multiget_operation,
                    status: resp.status(),
                });
            }
            let parsed = parse_multistatus_bytes(resp.body())?;
            for item in parsed.items {
                if let Some(data) = item.calendar_data.or(item.address_data) {
                    if !item.href.is_empty() {
                        out.insert(item.href, data);
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Flatten a raw sync row into a public entry.
fn entry_from_row(row: SyncRow) -> SyncEntry {
    SyncEntry {
        href: row.href,
        etag: row.etag,
        data: row.data,
    }
}

/// Collapse repeated hrefs across truncation pages: the last page's row
/// wins, first-occurrence order is kept.
fn dedup_rows(rows: Vec<SyncRow>) -> Vec<SyncRow> {
    let mut position: HashMap<String, usize> = HashMap::with_capacity(rows.len());
    let mut out: Vec<SyncRow> = Vec::with_capacity(rows.len());
    for row in rows {
        match position.get(&row.href) {
            Some(&pos) => out[pos] = row,
            None => {
                position.insert(row.href.clone(), out.len());
                out.push(row);
            }
        }
    }
    out
}

/// Extract the data payload from a sync-collection item (CalDAV or CardDAV
/// property; only one is ever requested).
fn extract_data(item: &mut DavItem) -> Option<String> {
    item.calendar_data
        .take()
        .or_else(|| item.address_data.take())
}

/// True when the status line carries an HTTP error status (4xx/5xx).
fn has_error_status(status: Option<&str>) -> bool {
    status
        .and_then(http_status_code)
        .is_some_and(|code| code >= 400)
}

/// Classified `sync-collection` rows plus the resulting state map.
struct RowDiff {
    added: Vec<SyncEntry>,
    modified: Vec<SyncEntry>,
    deleted: Vec<String>,
    next: HashMap<String, Option<String>>,
}

/// Classify `sync-collection` rows against the previous state.
///
/// `replace` marks a full-state response (initial sync or stale-token
/// resync): the new state map replaces the previous one instead of merging,
/// and deletions are suppressed (RFC 6578 §3.4: an initial sync must not
/// report deletions). Non-deleted rows with an error status (e.g. the 507
/// truncation marker) are continuation bookkeeping, not members, and are
/// dropped from both the delta and the state map.
fn diff_rows(
    prev: &Option<HashMap<String, Option<String>>>,
    rows: Vec<SyncRow>,
    replace: bool,
) -> RowDiff {
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();
    let mut next: HashMap<String, Option<String>> = if replace {
        HashMap::new()
    } else {
        prev.clone().unwrap_or_default()
    };

    for row in rows {
        if row.is_deleted {
            if !replace {
                deleted.push(row.href.clone());
            }
            next.remove(&row.href);
            continue;
        }
        if has_error_status(row.status.as_deref()) {
            continue;
        }
        let entry = SyncEntry {
            href: row.href.clone(),
            etag: row.etag.clone(),
            data: row.data,
        };
        match prev.as_ref().and_then(|map| map.get(&row.href)) {
            None => added.push(entry),
            Some(old) if old != &row.etag => modified.push(entry),
            Some(_) => {}
        }
        next.insert(row.href, row.etag);
    }

    RowDiff {
        added,
        modified,
        deleted,
        next,
    }
}

/// Classify a full etag list against the previous state (fallback path):
/// added = hrefs not seen before, modified = changed etags, deleted =
/// hrefs gone from the list. Output is sorted by href for determinism.
fn diff_maps(
    prev: &Option<HashMap<String, Option<String>>>,
    current: &HashMap<String, Option<String>>,
) -> (Vec<SyncEntry>, Vec<SyncEntry>, Vec<String>) {
    let empty = HashMap::new();
    let prev = prev.as_ref().unwrap_or(&empty);
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    for (href, etag) in current {
        let entry = SyncEntry {
            href: href.clone(),
            etag: etag.clone(),
            data: None,
        };
        match prev.get(href) {
            None => added.push(entry),
            Some(old) if old != etag => modified.push(entry),
            Some(_) => {}
        }
    }
    for href in prev.keys() {
        if !current.contains_key(href) {
            deleted.push(href.clone());
        }
    }

    added.sort_by(|a, b| a.href.cmp(&b.href));
    modified.sort_by(|a, b| a.href.cmp(&b.href));
    deleted.sort();
    (added, modified, deleted)
}
