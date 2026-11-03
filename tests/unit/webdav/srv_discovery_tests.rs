//! SRV/TXT service discovery (RFC 6764 §3/§6, `srv` feature): mocked
//! resolver + mock HTTP servers prove the resolution order, the graceful
//! fallback when DNS yields nothing, and the strict semantics when it does
//! not.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use fast_dav_rs::webdav::srv::{SrvRecord, SrvResolver};
use fast_dav_rs::webdav::{discover_caldav_srv, discover_carddav_srv};
use fast_dav_rs::{Error, Operation, RequestCompressionMode, WebDavClient};

use crate::common::http_helpers::{response_head, serve_capture, serve_sequence, unreachable_base};

const REDIRECT_301: &str = "HTTP/1.1 301 Moved Permanently\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const SERVER_ERROR: &str =
    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

fn redirect_head(template: &str, location: &str) -> String {
    template.replace("{loc}", location)
}

/// Configurable fake DNS: per-query SRV record sets, per-query error
/// injection, and per-query TXT strings.
#[derive(Default)]
struct MockResolver {
    srv: HashMap<String, Vec<SrvRecord>>,
    srv_errors: HashSet<String>,
    txt: HashMap<String, Vec<String>>,
}

impl MockResolver {
    fn with_srv(mut self, query: &str, records: Vec<SrvRecord>) -> Self {
        self.srv.insert(query.to_string(), records);
        self
    }
    fn with_srv_error(mut self, query: &str) -> Self {
        self.srv_errors.insert(query.to_string());
        self
    }
    fn with_txt(mut self, query: &str, strings: Vec<String>) -> Self {
        self.txt.insert(query.to_string(), strings);
        self
    }
}

impl SrvResolver for MockResolver {
    fn lookup_srv(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = fast_dav_rs::Result<Vec<SrvRecord>>> + Send + '_>> {
        if self.srv_errors.contains(query) {
            return Box::pin(async { Err(Error::other("dns unavailable")) });
        }
        let records = self.srv.get(query).cloned().unwrap_or_default();
        Box::pin(async move { Ok(records) })
    }

    fn lookup_txt(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = fast_dav_rs::Result<Vec<String>>> + Send + '_>> {
        let strings = self.txt.get(query).cloned().unwrap_or_default();
        Box::pin(async move { Ok(strings) })
    }
}

fn make_client(base: &str, resolver: MockResolver) -> WebDavClient {
    let client = WebDavClient::builder(base)
        .srv_resolver(Arc::new(resolver))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

fn mock_port(base: &str) -> u16 {
    base.trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap()
}

fn tls_record(port: u16) -> SrvRecord {
    SrvRecord {
        target: "127.0.0.1".to_string(),
        port,
        priority: 0,
        weight: 0,
    }
}

const DOMAIN: &str = "example.com";
const TLS_QUERY: &str = "_caldavs._tcp.example.com";
const PLAIN_QUERY: &str = "_caldav._tcp.example.com";

#[tokio::test]
async fn srv_with_txt_path_probes_resolved_authority() {
    let ok_body = b"ok".to_vec();
    let probe = (redirect_head(REDIRECT_301, "/caldav/"), Vec::new());
    let final_hop = (response_head("", ok_body.len()), ok_body);
    let (base, captured) = serve_sequence(vec![probe, final_hop]).await;
    // Plain (http) record: the mock serves plain HTTP, so the SRV authority
    // must be an http:// URL (the TLS record would produce https://).
    let resolver = MockResolver::default()
        .with_srv(PLAIN_QUERY, vec![tls_record(mock_port(&base))])
        .with_txt(PLAIN_QUERY, vec!["path=/cal/".to_string()]);
    let client = make_client("https://dav.example.com/", resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    // The probe targets the SRV authority root (the TXT path belongs to
    // the 404 fallback, not to the well-known URI); the redirect resolves
    // the final service URL.
    assert_eq!(service_url, format!("{base}caldav/"));
    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    let first = String::from_utf8_lossy(&reqs[0]);
    assert!(
        first.contains("PROPFIND /.well-known/caldav HTTP/1.1"),
        "{first}"
    );
    // The probe crosses origins (base is dav.example.com): the existing
    // same-origin invariant must keep credentials off the wire.
    assert!(
        !first.to_ascii_lowercase().contains("authorization:"),
        "credentials must not be sent to a foreign SRV authority: {first}"
    );
}

#[tokio::test]
async fn srv_absent_falls_back_to_base_well_known() {
    let (base, captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let client = make_client(&base, MockResolver::default());

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    assert_eq!(
        service_url, base,
        "no SRV record anywhere: the plain .well-known flow on the base URL runs unchanged"
    );
    let lock = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&lock);
    assert!(
        req.contains("PROPFIND /.well-known/caldav HTTP/1.1"),
        "{req}"
    );
}

#[tokio::test]
async fn dns_error_falls_back_like_absence() {
    let (base, _captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let resolver = MockResolver::default().with_srv_error(TLS_QUERY);
    let client = make_client(&base, resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();
    assert_eq!(
        service_url, base,
        "a DNS failure degrades gracefully to the plain .well-known flow"
    );
}

#[tokio::test]
async fn srv_found_probe_404_returns_candidate_base() {
    let (base, _captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let resolver = MockResolver::default()
        .with_srv(PLAIN_QUERY, vec![tls_record(mock_port(&base))])
        .with_txt(PLAIN_QUERY, vec!["path=/cal/".to_string()]);
    let client = make_client("https://dav.example.com/", resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    // RFC 6764 §6: 404 on the probe → the SRV+TXT candidate base itself.
    assert_eq!(service_url, format!("{base}cal/"));
}

#[tokio::test]
async fn srv_found_probe_failure_is_strict() {
    let (base, _captured) = serve_capture(SERVER_ERROR.to_owned(), Vec::new()).await;
    let resolver =
        MockResolver::default().with_srv(PLAIN_QUERY, vec![tls_record(mock_port(&base))]);
    let client = make_client("https://dav.example.com/", resolver);

    let err = discover_caldav_srv(&client, DOMAIN).await.unwrap_err();
    assert!(
        matches!(
            err,
            Error::UnexpectedStatus {
                operation: Operation::DiscoverWellKnownCaldav,
                ..
            }
        ),
        "a found SRV record makes the probe authoritative: {err:?}"
    );
}

#[tokio::test]
async fn require_https_skips_plain_srv_records() {
    // The plain (http) SRV record points at a working mock that answers 404:
    // if the implementation ever consulted it, the probe would succeed and
    // return that candidate base. The client base is an https:// URL on a
    // refused port (the builder enforces an https base under require_https),
    // so the correct behavior — skipping the plain record and probing the
    // base — fails fast with a transport error.
    let (mock, captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let unreachable = unreachable_base().await.replacen("http://", "https://", 1);
    let resolver =
        MockResolver::default().with_srv(PLAIN_QUERY, vec![tls_record(mock_port(&mock))]);
    let client = WebDavClient::builder(&unreachable)
        .require_https(true)
        .srv_resolver(Arc::new(resolver))
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let result = discover_caldav_srv(&client, DOMAIN).await;

    assert!(
        result.is_err(),
        "the plain (http) record must never be consulted under require_https: {result:?}"
    );
    assert!(
        captured.lock().unwrap().is_empty(),
        "no request may reach the plain record's authority"
    );
}

#[tokio::test]
async fn tls_srv_record_wins_over_plain() {
    // TLS record present (points at a refused port) AND plain record present
    // (points at a working 404 mock): the TLS record must win. If the plain
    // record were picked, the probe would succeed and return its candidate
    // base; TLS-first makes the probe fail on the refused port (strict).
    let (mock, _captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let unreachable = unreachable_base().await;
    let resolver = MockResolver::default()
        .with_srv(
            TLS_QUERY,
            vec![SrvRecord {
                target: "127.0.0.1".to_string(),
                port: 1,
                priority: 0,
                weight: 0,
            }],
        )
        .with_srv(PLAIN_QUERY, vec![tls_record(mock_port(&mock))]);
    let client = make_client(&unreachable, resolver);

    let result = discover_caldav_srv(&client, DOMAIN).await;

    // If the plain record had been picked, the probe would have succeeded
    // (Ok(candidate)); TLS-first makes the refused port's probe fail.
    assert!(
        result.is_err(),
        "the TLS record must win over the plain record: {result:?}"
    );
}

#[tokio::test]
async fn plain_srv_record_used_without_require_https() {
    let ok_body = b"ok".to_vec();
    let direct = (response_head("", ok_body.len()), ok_body);
    let (base, _captured) = serve_sequence(vec![direct]).await;
    let resolver =
        MockResolver::default().with_srv(PLAIN_QUERY, vec![tls_record(mock_port(&base))]);
    let client = make_client("https://dav.example.com/", resolver);

    let service_url = discover_caldav_srv(&client, DOMAIN).await.unwrap();

    // Success served directly on the probe URI (RFC 5785 §1.1): the
    // candidate base (http authority, no TXT path → root) is returned.
    assert_eq!(service_url, base);
}

#[tokio::test]
async fn carddav_srv_queries_the_carddav_service_name() {
    let (base, captured) = serve_capture(NOT_FOUND.to_owned(), Vec::new()).await;
    let resolver = MockResolver::default().with_srv_error("_carddavs._tcp.example.com");
    let client = make_client(&base, resolver);

    let service_url = discover_carddav_srv(&client, DOMAIN).await.unwrap();
    assert_eq!(service_url, base);
    let lock = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&lock);
    assert!(
        req.contains("PROPFIND /.well-known/carddav HTTP/1.1"),
        "{req}"
    );
}

#[tokio::test]
async fn empty_domain_is_rejected_before_any_lookup() {
    let client = make_client("https://dav.example.com/", MockResolver::default());
    let err = discover_caldav_srv(&client, "   ").await.unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
}
