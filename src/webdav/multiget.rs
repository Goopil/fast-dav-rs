//! Shared multiget engine: empty-href filtering, chunked REPORT round-trips
//! with href reconciliation, used by both `CalDavClient::calendar_multiget_many`
//! and `CardDavClient::addressbook_multiget_many`.

use std::sync::Arc;

use bytes::Bytes;

use crate::webdav::client::WebDavClient;
use crate::webdav::streaming::parse_multistatus_bytes;
use crate::webdav::types::{BatchItem, DavItem};
use crate::{Error, Operation, Result};

/// Run chunked multiget REPORTs concurrently and reconcile the answers.
///
/// Empty hrefs are dropped from `hrefs` **before** chunking (so every
/// recorded `BatchItem::hrefs` matches the hrefs its chunk's REPORT actually
/// carried); an input with no non-empty href produces no batches and no
/// network I/O. The remaining hrefs are split into slices of `batch_size`;
/// one REPORT is issued per chunk via [`WebDavClient::report_many_bodies`]
/// (bounded by `max_concurrency`, `Depth: 0`), with `build_body` producing
/// each chunk's request body (a `None` return skips the chunk). Each chunk
/// then becomes one or more [`BatchItem`]s, preserving chunk order:
///
/// - A chunk answered with a non-success status or a transport error, or
///   whose multistatus body fails to parse, produces exactly **one** error
///   `BatchItem` whose `hrefs` name everything to re-fetch; sibling chunks
///   are unaffected.
/// - A parsed chunk produces one `BatchItem` per mapped object, in server
///   order, each carrying the chunk's `pub_path`, `hrefs`, and
///   `missing_hrefs` — the requested hrefs the server did not echo with a
///   `<D:response>` (exact href string comparison). For compliant servers
///   `missing_hrefs` is empty.
///
/// `map` turns the chunk's parsed multistatus entries into the caller's
/// objects (one `BatchItem` per returned object, mirroring the CalDAV
/// semantics); a chunk whose multistatus carries no entries contributes no
/// items. Non-success statuses, transport errors and unparsable bodies
/// surface as error `BatchItem`s; the engine performs no input validation of
/// its own, so an `Err` return comes only from precondition checks its
/// callers run before any I/O.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn multiget_many<T, B, M>(
    webdav: &WebDavClient,
    operation: Operation,
    collection_path: &str,
    hrefs: &[String],
    batch_size: usize,
    max_concurrency: usize,
    build_body: B,
    map: M,
) -> Result<Vec<BatchItem<T>>>
where
    B: Fn(&[String]) -> Option<String>,
    M: Fn(Vec<DavItem>) -> Vec<T>,
{
    let filtered: Vec<String> = hrefs.iter().filter(|h| !h.is_empty()).cloned().collect();
    if filtered.is_empty() {
        return Ok(Vec::new());
    }

    let mut requests = Vec::new();
    let mut chunk_hrefs = Vec::new();
    for chunk in filtered.chunks(batch_size) {
        let Some(xml) = build_body(chunk) else {
            continue;
        };
        requests.push((collection_path.to_owned(), Arc::new(Bytes::from(xml))));
        chunk_hrefs.push(chunk.to_vec());
    }

    let batches = webdav.report_many_bodies(requests, max_concurrency).await;

    let mut out = Vec::new();
    for (batch, requested) in batches.into_iter().zip(chunk_hrefs) {
        match batch.result {
            Ok(resp) if !resp.status().is_success() => {
                out.push(BatchItem {
                    pub_path: batch.pub_path,
                    hrefs: requested,
                    missing_hrefs: Vec::new(),
                    result: Err(Error::UnexpectedStatus {
                        operation,
                        status: resp.status(),
                    }),
                });
            }
            Ok(resp) => {
                let body = resp.into_body();
                match parse_multistatus_bytes(&body) {
                    Ok(parsed) => {
                        let items = parsed.items;
                        // Exact href string comparison: a compliant server
                        // echoes every requested href (RFC 4791 §9.6.1,
                        // RFC 6352 §8.7); anything not echoed is reported.
                        let returned: Vec<&str> =
                            items.iter().map(|item| item.href.as_str()).collect();
                        let missing: Vec<String> = requested
                            .iter()
                            .filter(|href| !returned.contains(&href.as_str()))
                            .cloned()
                            .collect();
                        for object in map(items) {
                            out.push(BatchItem {
                                pub_path: batch.pub_path.clone(),
                                hrefs: requested.clone(),
                                missing_hrefs: missing.clone(),
                                result: Ok(object),
                            });
                        }
                    }
                    Err(e) => out.push(BatchItem {
                        pub_path: batch.pub_path,
                        hrefs: requested,
                        missing_hrefs: Vec::new(),
                        result: Err(e),
                    }),
                }
            }
            Err(e) => out.push(BatchItem {
                pub_path: batch.pub_path,
                hrefs: requested,
                missing_hrefs: Vec::new(),
                result: Err(e),
            }),
        }
    }
    Ok(out)
}
