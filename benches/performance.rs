//! Criterion benchmarks implementing the three scenarios specified in
//! `docs/audit/PERFORMANCE.md` §5:
//!
//! - **B1** — `sync_collection` over 1k/10k synthetic items, `include_data`
//!   on/off (4 cases).
//! - **B2** — first-request latency in `Auto` compression mode with 32
//!   concurrent callers (plus a `Disabled` baseline isolating the probe cost).
//! - **B3** — aggregated parse vs `parse_multistatus_stream_visit` throughput
//!   on a ~50 MB multistatus.
//!
//! The fixture is an **in-process** hyper HTTP/1.1 server bound to an
//! ephemeral `127.0.0.1` port — no network, no Docker. Synthetic multistatus
//! payloads are generated once per group, outside the measured closures; the
//! measured code is only the client call / parse.

use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fast_dav_rs::RequestCompressionMode;
use fast_dav_rs::webdav::streaming::{parse_multistatus_stream, parse_multistatus_stream_visit};
use fast_dav_rs::webdav::{Depth, SyncLevel, WebDavClient};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};

const CALDAV_NS: &str = "urn:ietf:params:xml:ns:caldav";

/// Request body sent with the benched `REPORT`s. Only its presence matters —
/// the fixture routes on method + path — but it is a realistic
/// `sync-collection` request.
const SYNC_REQUEST_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:sync-collection xmlns:D="DAV:">
  <D:sync-token/>
  <D:sync-level>1</D:sync-level>
  <D:prop><D:getetag/></D:prop>
</D:sync-collection>"#;

/// One filler iCalendar line (~100 chars); repeated to size each
/// `calendar-data` blob. Kept XML-safe (no `<`, `&`, quotes).
const ICS_FILLER_LINE: &str = "DESCRIPTION:Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore 0123456789\n";

/// Sync token embedded in every canned payload; asserted after parsing to
/// prove the benched call consumed the intended payload.
const SYNC_TOKEN: &str = "http://example.com/sync/bench";

/// Build a canned `207 Multi-Status` `sync-collection` response with `items`
/// `<D:response>` entries. When `include_data` is set, each item carries a
/// `calendar-data` blob of `data_lines` filler lines.
fn build_sync_payload(items: usize, include_data: bool, data_lines: usize) -> Bytes {
    let mut body = String::with_capacity(items * (420 + data_lines * ICS_FILLER_LINE.len()));
    body.push_str(
        "<?xml version=\"1.0\"?>\n<D:multistatus xmlns:D=\"DAV:\" \
         xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\n",
    );
    for i in 0..items {
        let n = i.to_string();
        body.push_str("  <D:response>\n    <D:href>/cal/event-");
        body.push_str(&n);
        body.push_str(
            ".ics</D:href>\n    <D:propstat>\n      <D:prop>\n        <D:getetag>\"etag-",
        );
        body.push_str(&n);
        body.push_str("\"</D:getetag>\n");
        if include_data {
            body.push_str(
                "        <C:calendar-data>BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//bench//EN\n",
            );
            for _ in 0..data_lines {
                body.push_str(ICS_FILLER_LINE);
            }
            body.push_str("END:VCALENDAR</C:calendar-data>\n");
        }
        body.push_str(
            "      </D:prop>\n      <D:status>HTTP/1.1 200 OK</D:status>\n    </D:propstat>\n  \
             </D:response>\n",
        );
    }
    body.push_str(&format!(
        "  <D:sync-token>{SYNC_TOKEN}</D:sync-token>\n</D:multistatus>\n"
    ));
    Bytes::from(body)
}

/// Canned-response table: REPORT path → 207 body.
type Routes = HashMap<&'static str, Bytes>;

/// Minimal 207 served to every non-REPORT request (the `Auto` compression
/// probe is a PROPFIND; it only needs a success status to negotiate).
const PROBE_RESPONSE: &str = "<?xml version=\"1.0\"?>\n<D:multistatus xmlns:D=\"DAV:\">\n  \
                              <D:response>\n    <D:href>/</D:href>\n  </D:response>\n\
                              </D:multistatus>\n";

/// Bind the in-process HTTP/1.1 fixture on an ephemeral `127.0.0.1` port and
/// spawn its accept loop on `rt`. Returns the client base URL.
fn start_fixture(rt: &tokio::runtime::Runtime, routes: Routes) -> String {
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture listener");
        let base = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("local addr").port()
        );
        let routes = Arc::new(routes);
        let probe_response = Bytes::from_static(PROBE_RESPONSE.as_bytes());
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let routes = routes.clone();
                let probe_response = probe_response.clone();
                tokio::spawn(async move {
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(
                            hyper_util::rt::TokioIo::new(stream),
                            service_fn(move |req: Request<Incoming>| {
                                route(req, routes.clone(), probe_response.clone())
                            }),
                        )
                        .await;
                });
            }
        });
        base
    })
}

/// Fixture handler: REPORT + known path → canned 207 payload; anything else
/// (including the compression probe) → the small 207. The request body is
/// drained so keep-alive connections can be pooled and reused.
async fn route(
    req: Request<Incoming>,
    routes: Arc<Routes>,
    probe_response: Bytes,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let (parts, body) = req.into_parts();
    let _ = body.collect().await;
    let payload = if parts.method.as_str() == "REPORT" {
        routes.get(parts.uri.path()).unwrap_or(&probe_response)
    } else {
        &probe_response
    };
    Ok(Response::builder()
        .status(207)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(Full::new(payload.clone()))
        .expect("static response parts are valid"))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
}

/// B1 — `sync_collection` over 1k/10k synthetic items, `include_data` on/off.
fn bench_b1_sync_collection(c: &mut Criterion) {
    let rt = runtime();

    let mut routes = Routes::new();
    let mut payload_sizes = HashMap::new();
    // Single source of truth for label ↔ path ↔ item count, so the fixture
    // route and the expected item-count guard cannot drift apart.
    let cases = [
        ("1k_with_data", 1_000usize, true, "/sync/1k/data/"),
        ("1k_etags_only", 1_000, false, "/sync/1k/nodata/"),
        ("10k_with_data", 10_000, true, "/sync/10k/data/"),
        ("10k_etags_only", 10_000, false, "/sync/10k/nodata/"),
    ];
    for (label, items, include_data, path) in cases {
        let payload = build_sync_payload(items, include_data, 12);
        payload_sizes.insert(label, payload.len());
        routes.insert(path, payload);
    }
    let base = start_fixture(&rt, routes);
    let client = WebDavClient::builder(base.as_str())
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .expect("client");

    let mut group = c.benchmark_group("B1_sync_collection");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));
    for (label, items_expected, include_data, path) in cases {
        group.throughput(Throughput::Bytes(payload_sizes[label] as u64));
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.to_async(&rt).iter(|| {
                let client = client.clone();
                async move {
                    let (_, items, _) = client
                        .sync_collection_with_level(
                            path,
                            None,
                            None,
                            include_data,
                            CALDAV_NS,
                            "calendar-data",
                            SyncLevel::One,
                        )
                        .await
                        .expect("sync-collection REPORT");
                    assert_eq!(items.len(), items_expected);
                    black_box(items.len())
                }
            });
        });
    }
    group.finish();
}

/// B2 — first-request latency in `Auto` compression mode, 32 concurrent
/// callers (PERFORMANCE.md §2.1: the probe head-of-line-blocking scenario).
/// `disabled` is the subtraction baseline isolating the probe cost. One
/// client is reused across iterations; each iteration clears the negotiation
/// cache (`set_request_compression_mode(Auto)`) so the measured wave is a
/// genuine first wave, without per-iteration socket churn.
fn bench_b2_first_request_auto(c: &mut Criterion) {
    let rt = runtime();

    let payload = build_sync_payload(25, true, 2);
    let payload_len = payload.len();
    let routes = HashMap::from([("/coll/", payload)]);
    let base = start_fixture(&rt, routes);
    let client = WebDavClient::builder(base.as_str())
        .build()
        .expect("client");

    let mut group = c.benchmark_group("B2_first_request_32_callers");
    group.sample_size(30);
    for (label, mode) in [
        ("auto", RequestCompressionMode::Auto),
        ("disabled", RequestCompressionMode::Disabled),
    ] {
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.to_async(&rt).iter(|| {
                let client = client.clone();
                async move {
                    client.set_request_compression_mode(mode);
                    let mut handles = Vec::with_capacity(32);
                    for _ in 0..32 {
                        let caller = client.clone();
                        handles.push(tokio::spawn(async move {
                            caller.report("coll/", Depth::Zero, SYNC_REQUEST_BODY).await
                        }));
                    }
                    let mut total = 0usize;
                    for handle in handles {
                        let resp = handle.await.expect("caller task").expect("report");
                        total += resp.body().len();
                    }
                    // Every caller must have received the full canned payload.
                    assert_eq!(total, 32 * payload_len);
                    black_box(total);
                }
            });
        });
    }
    group.finish();
}

/// B3 — aggregated parse vs `parse_multistatus_stream_visit` throughput on a
/// ~50 MB multistatus (5,000 items × ~10 KB of `calendar-data`).
fn bench_b3_multistatus_parse(c: &mut Criterion) {
    let rt = runtime();

    let payload = build_sync_payload(5_000, true, 80);
    let payload_len = payload.len() as u64;
    let routes = HashMap::from([("/big/", payload)]);
    let base = start_fixture(&rt, routes);
    let client = WebDavClient::builder(base.as_str())
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .expect("client");

    let mut group = c.benchmark_group("B3_multistatus_parse_50mb");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(payload_len));

    group.bench_function("aggregated", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            async move {
                let resp = client
                    .report_stream("big/", Depth::Zero, SYNC_REQUEST_BODY)
                    .await
                    .expect("report_stream");
                let parsed = parse_multistatus_stream(resp.into_body(), &[])
                    .await
                    .expect("parse multistatus");
                assert_eq!(parsed.items.len(), 5_000);
                assert_eq!(parsed.sync_token.as_deref(), Some(SYNC_TOKEN));
                black_box(parsed.items.len())
            }
        });
    });

    group.bench_function("visit", |b| {
        b.to_async(&rt).iter(|| {
            let client = client.clone();
            async move {
                let resp = client
                    .report_stream("big/", Depth::Zero, SYNC_REQUEST_BODY)
                    .await
                    .expect("report_stream");
                let sync_token = parse_multistatus_stream_visit(resp.into_body(), &[], |item| {
                    black_box(&item);
                    Ok(())
                })
                .await
                .expect("parse multistatus");
                assert_eq!(sync_token.as_deref(), Some(SYNC_TOKEN));
                black_box(sync_token.is_some())
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_b1_sync_collection,
    bench_b2_first_request_auto,
    bench_b3_multistatus_parse
);
criterion_main!(benches);
