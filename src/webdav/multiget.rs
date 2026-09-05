//! Shared multiget engine: chunked REPORT round-trips with href
//! reconciliation, used by both `CalDavClient::calendar_multiget_many` and
//! `CardDavClient::addressbook_multiget_many`.

use std::sync::Arc;

use bytes::Bytes;

use crate::webdav::client::WebDavClient;
use crate::webdav::streaming::parse_multistatus_bytes;
use crate::webdav::types::{BatchItem, DavItem};
use crate::{Error, Operation, Result};

/// Run chunked multiget REPORTs concurrently and reconcile the answers.
///
/// One REPORT per entry of `requests` is issued via
/// [`WebDavClient::report_many_bodies`] (bounded by `max_concurrency`,
/// `Depth: 0`); the i-th result is paired with the i-th entry of
/// `chunk_hrefs` (the hrefs that chunk's REPORT requested). Each chunk then
/// becomes one or more [`BatchItem`]s, preserving chunk order:
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
/// items. The method itself only fails when a precondition violated before
/// any I/O is detected by the caller (the engine performs none of its own).
pub(crate) async fn multiget_many<T, F>(
    webdav: &WebDavClient,
    operation: Operation,
    requests: Vec<(String, Arc<Bytes>)>,
    chunk_hrefs: Vec<Vec<String>>,
    max_concurrency: usize,
    map: F,
) -> Result<Vec<BatchItem<T>>>
where
    F: Fn(Vec<DavItem>) -> Vec<T>,
{
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
