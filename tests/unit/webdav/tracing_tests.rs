#![cfg(feature = "tracing")]
//! Wire tests for the optional `tracing` instrumentation (feature `tracing`).
//!
//! A minimal hand-written capturing `Subscriber` records the events emitted by
//! the shared request pipeline so the tests can assert that a request produces
//! start + finish records (method/status), that a transient retry logs the
//! retry event (and its exhaustion), that a failed compression probe logs the
//! probe outcome, and that a per-request timeout is logged. Compiled out
//! entirely when the feature is disabled.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use fast_dav_rs::{Error, WebDavClient};
use hyper::{HeaderMap, Method};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::subscriber::DefaultGuard;
use tracing::{Event, Id, Level, Metadata, Subscriber};

use crate::common::http_helpers::{response_head, serve_capture, serve_sequence};

fn status_head(status_line: &str) -> String {
    format!("HTTP/1.1 {status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}

fn ok(body: &[u8]) -> (String, Vec<u8>) {
    (response_head("", body.len()), body.to_vec())
}

#[derive(Debug, Clone)]
struct CapturedRecord {
    level: Level,
    message: String,
    fields: Vec<(String, String)>,
}

impl CapturedRecord {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

struct CapturingSubscriber {
    records: Arc<Mutex<Vec<CapturedRecord>>>,
    next_id: AtomicU64,
}

impl Subscriber for CapturingSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _attrs: &Attributes<'_>) -> Id {
        Id::from_u64(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut record = CapturedRecord {
            level: *event.metadata().level(),
            message: String::new(),
            fields: Vec::new(),
        };
        event.record(&mut RecordVisitor {
            record: &mut record,
        });
        self.records.lock().unwrap().push(record);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

struct RecordVisitor<'a> {
    record: &'a mut CapturedRecord,
}

impl Visit for RecordVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.record.message = format!("{value:?}");
        } else {
            self.record
                .fields
                .push((field.name().to_owned(), format!("{value:?}")));
        }
    }
}

/// Install the capturing subscriber as the thread-local default (the tests run
/// on a current-thread tokio runtime, so pipeline events are all captured).
/// The returned guard must be kept alive for the duration of the request.
fn install_subscriber() -> (Arc<Mutex<Vec<CapturedRecord>>>, DefaultGuard) {
    let records = Arc::new(Mutex::new(Vec::new()));
    let guard = tracing::subscriber::set_default(CapturingSubscriber {
        records: records.clone(),
        next_id: AtomicU64::new(1),
    });
    (records, guard)
}

#[tokio::test]
async fn send_emits_request_start_and_finish_records() {
    let (base, _captured) = serve_capture(response_head("", 2), b"ok".to_vec()).await;
    let client = WebDavClient::new(&base, None, None).unwrap();
    let (records, _guard) = install_subscriber();

    let resp = client.options("/").await.unwrap();
    assert_eq!(resp.status(), 200);

    let records = records.lock().unwrap();
    let start = records
        .iter()
        .find(|r| r.message == "dav request start")
        .expect("a request start record must be emitted");
    assert_eq!(start.level, Level::DEBUG);
    assert_eq!(start.field("method"), Some("OPTIONS"));
    assert!(start.field("uri").is_some());

    let finish = records
        .iter()
        .find(|r| r.message == "dav request finished")
        .expect("a request finish record must be emitted");
    assert_eq!(finish.level, Level::DEBUG);
    assert_eq!(finish.field("method"), Some("OPTIONS"));
    assert_eq!(finish.field("status"), Some("200 OK"));
    assert!(
        finish
            .field("duration_us")
            .and_then(|v| v.parse::<u64>().ok())
            .is_some(),
        "finish record must carry the request duration: {finish:?}"
    );

    let size = records
        .iter()
        .find(|r| r.message == "decompressed response body")
        .expect("a decompressed-size record must be emitted at trace level");
    assert_eq!(size.level, Level::TRACE);
    assert_eq!(size.field("bytes"), Some("2"));
}

#[tokio::test]
async fn transient_retry_is_logged() {
    let (base, captured) = serve_sequence(vec![
        (status_head("503 Service Unavailable"), Vec::new()),
        ok(b"done"),
    ])
    .await;
    let mut client = WebDavClient::builder(&base).max_retries(1).build().unwrap();
    client.set_retry_delays_for_testing(Duration::from_millis(1), Duration::from_millis(2));
    let (records, _guard) = install_subscriber();

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        captured.lock().unwrap().len(),
        2,
        "the 503 must be retried once"
    );

    let records = records.lock().unwrap();
    let retry = records
        .iter()
        .find(|r| r.message == "retrying after transient failure")
        .expect("a retry record must be emitted");
    assert_eq!(retry.level, Level::DEBUG);
    assert_eq!(retry.field("status"), Some("503 Service Unavailable"));
    assert_eq!(retry.field("attempt"), Some("1"));
    assert!(retry.field("delay_ms").is_some());
}

#[tokio::test]
async fn retry_budget_exhaustion_is_logged() {
    let (base, captured) = serve_sequence(vec![
        (status_head("503 Service Unavailable"), Vec::new()),
        (status_head("503 Service Unavailable"), Vec::new()),
    ])
    .await;
    let mut client = WebDavClient::builder(&base).max_retries(1).build().unwrap();
    client.set_retry_delays_for_testing(Duration::from_millis(1), Duration::from_millis(2));
    let (records, _guard) = install_subscriber();

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        503,
        "exhausted retries must return the last response as-is"
    );
    assert_eq!(
        captured.lock().unwrap().len(),
        2,
        "1 + max_retries attempts"
    );

    let records = records.lock().unwrap();
    let exhausted = records
        .iter()
        .find(|r| r.message == "retry budget exhausted; returning last response as-is")
        .expect("an exhausted-retries record must be emitted");
    assert_eq!(exhausted.field("attempts"), Some("1"));
}

#[tokio::test]
async fn compression_probe_failure_is_logged() {
    // 1) probe → 500 (failure outcome), 2) real request (uncompressed).
    let (base, captured) = serve_sequence(vec![
        (status_head("500 Internal Server Error"), Vec::new()),
        ok(b"first"),
    ])
    .await;
    let client = WebDavClient::builder(&base).build().unwrap();
    let (records, _guard) = install_subscriber();

    let resp = client
        .send(
            Method::PUT,
            "one.txt",
            HeaderMap::new(),
            Some(Bytes::from_static(b"first")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(captured.lock().unwrap().len(), 2, "probe + request");

    let records = records.lock().unwrap();
    let probe = records
        .iter()
        .find(|r| r.message == "request compression probe failed; re-probing on the next request")
        .expect("a probe-failure outcome record must be emitted");
    assert_eq!(probe.level, Level::DEBUG);
}

#[tokio::test]
async fn send_timeout_is_logged() {
    // Unroutable TEST-NET address: the connection never completes, so the
    // per-request timeout must fire (and be logged). A zero timeout makes the
    // failure deterministic and the test fast.
    let client = WebDavClient::new("http://203.0.113.1:81/", None, None).unwrap();
    let (records, _guard) = install_subscriber();

    let err = client
        .send(
            Method::GET,
            "/",
            HeaderMap::new(),
            None,
            Some(Duration::ZERO),
        )
        .await;
    assert!(
        matches!(err, Err(Error::Timeout { .. })),
        "expected a timeout, got {err:?}"
    );

    assert!(
        records
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.message == "dav request timed out"),
        "a timeout record must be emitted"
    );
}
