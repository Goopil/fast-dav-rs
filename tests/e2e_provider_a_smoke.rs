//! Opt-in smoke tier against Provider A (the real-world deployment this
//! crate is validated against). **Zero credentials are used, ever** — these
//! tests only probe the unauthenticated surface:
//!
//! 1. `OPTIONS /`
//! 2. `GET /.well-known/caldav`
//! 3. `GET /.well-known/carddav`
//! 4. unauthenticated `PROPFIND` of the current-user-principal
//!
//! (4 requests total, under the 5-request budget.)
//!
//! Asserted (loosely): the endpoint is reachable (TLS terminates), the
//! unauthenticated status is `401` with a `WWW-Authenticate: Basic` header
//! on data/DAV requests, and the well-known shape (redirect vs 401) is
//! *recorded* via `println!` rather than hard-asserted.
//!
//! The tier never runs in CI. Run it locally against your own deployment:
//!
//! ```text
//! PROVIDER_A_DAV_URL=https://dav.example.test \
//!   cargo test --test e2e_provider_a_smoke -- --ignored --nocapture
//! ```
//!
//! The provider is deliberately not named anywhere in the repository; the
//! endpoint URL is read from the `PROVIDER_A_DAV_URL` env var and the tier
//! skips itself (with a printed note) when the var is unset.

use fast_dav_rs::{Depth, RequestCompressionMode, WebDavClient};

/// Reads `PROVIDER_A_DAV_URL`; prints a skip note and returns `None` when it
/// is unset or blank. This is a second safety net next to `#[ignore]`, so
/// that even `cargo test -- --ignored` stays green without the env var.
fn provider_url() -> Option<String> {
    match std::env::var("PROVIDER_A_DAV_URL") {
        Ok(url) if !url.trim().is_empty() => {
            if !url.starts_with("https://") {
                println!(
                    "NOTE: PROVIDER_A_DAV_URL is not https:// — TLS reachability is only proven for https endpoints."
                );
            }
            Some(url)
        }
        _ => {
            println!(
                "SKIP: PROVIDER_A_DAV_URL is not set — the Provider A smoke tier is opt-in. \
                 Set it to an https DAV endpoint (no credentials needed) to run."
            );
            None
        }
    }
}

/// Credential-free client with redirect following disabled, so the raw
/// well-known shape (redirect vs direct 401) can be recorded. Compression is
/// disabled to keep the auto-probe out of the request budget.
fn smoke_client(url: &str) -> WebDavClient {
    WebDavClient::builder(url)
        .follow_redirects(false)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .expect("smoke client construction")
}

/// Asserts a 401 response that must carry a `WWW-Authenticate: Basic`
/// challenge; returns the challenge for printing.
fn assert_basic_challenge(
    status: hyper::StatusCode,
    www_authenticate: Option<&hyper::header::HeaderValue>,
    what: &str,
) {
    assert_eq!(
        status.as_u16(),
        401,
        "{what}: an unauthenticated request must be rejected with 401 (observed {status})"
    );
    let challenge = www_authenticate
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| panic!("{what}: the 401 must carry a WWW-Authenticate header"));
    assert!(
        challenge.to_ascii_lowercase().contains("basic"),
        "{what}: the challenge must advertise Basic auth, got: {challenge}"
    );
}

/// `OPTIONS /` unauthenticated: must be reachable and either demand Basic
/// auth (401) or advertise its DAV compliance classes on the open OPTIONS
/// (RFC 4918 §10.1 — several engines, e.g. Radicale, answer 200 here). The
/// observed shape is printed; the 401 shape additionally asserts the
/// challenge is present.
#[tokio::test]
#[ignore = "opt-in Provider A smoke tier; set PROVIDER_A_DAV_URL and run with --ignored"]
async fn test_smoke_options_root_unauthenticated() {
    let Some(url) = provider_url() else {
        return;
    };
    let client = smoke_client(&url);

    let resp = client
        .options("/")
        .await
        .expect("OPTIONS / must complete (this also proves the TLS endpoint is reachable)");
    let status = resp.status();
    let dav = resp
        .headers()
        .get("dav")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    if status.as_u16() == 401 {
        assert_basic_challenge(status, resp.headers().get("www-authenticate"), "OPTIONS /");
        println!("OPTIONS / shape: 401 + Basic challenge, DAV header: {dav:?}");
    } else if status.is_success() {
        println!("OPTIONS / shape: open {status}, DAV header: {dav:?}");
    } else {
        panic!("OPTIONS /: unexpected status {status} for an unauthenticated OPTIONS request");
    }
}

/// `GET /.well-known/caldav` unauthenticated: records the shape — some
/// deployments answer 301 to the DAV root, others 401 right away. Both are
/// acceptable; anything else is flagged.
#[tokio::test]
#[ignore = "opt-in Provider A smoke tier; set PROVIDER_A_DAV_URL and run with --ignored"]
async fn test_smoke_well_known_caldav_shape() {
    let Some(url) = provider_url() else {
        return;
    };
    let client = smoke_client(&url);

    let resp = client
        .get("/.well-known/caldav")
        .await
        .expect("GET /.well-known/caldav must complete");
    let status = resp.status();
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    println!("well-known/caldav: status {status}, location {location:?}");

    if status.is_redirection() {
        assert!(
            location.is_some(),
            "a redirect response must carry a Location header"
        );
        println!("well-known/caldav shape: REDIRECT -> {location:?}");
    } else if status.as_u16() == 401 {
        assert_basic_challenge(
            status,
            resp.headers().get("www-authenticate"),
            "well-known/caldav",
        );
        println!("well-known/caldav shape: DIRECT 401");
    } else {
        panic!("well-known/caldav: unexpected shape — expected 3xx redirect or 401, got {status}");
    }
}

/// `GET /.well-known/carddav` unauthenticated: same recording rules as the
/// CalDAV well-known probe.
#[tokio::test]
#[ignore = "opt-in Provider A smoke tier; set PROVIDER_A_DAV_URL and run with --ignored"]
async fn test_smoke_well_known_carddav_shape() {
    let Some(url) = provider_url() else {
        return;
    };
    let client = smoke_client(&url);

    let resp = client
        .get("/.well-known/carddav")
        .await
        .expect("GET /.well-known/carddav must complete");
    let status = resp.status();
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    println!("well-known/carddav: status {status}, location {location:?}");

    if status.is_redirection() {
        assert!(
            location.is_some(),
            "a redirect response must carry a Location header"
        );
        println!("well-known/carddav shape: REDIRECT -> {location:?}");
    } else if status.as_u16() == 401 {
        assert_basic_challenge(
            status,
            resp.headers().get("www-authenticate"),
            "well-known/carddav",
        );
        println!("well-known/carddav shape: DIRECT 401");
    } else {
        panic!("well-known/carddav: unexpected shape — expected 3xx redirect or 401, got {status}");
    }
}

/// Unauthenticated PROPFIND of the current-user-principal: must be rejected
/// with 401 + Basic challenge; the principal itself must never leak.
#[tokio::test]
#[ignore = "opt-in Provider A smoke tier; set PROVIDER_A_DAV_URL and run with --ignored"]
async fn test_smoke_unauthenticated_propfind_current_user_principal() {
    let Some(url) = provider_url() else {
        return;
    };
    let client = smoke_client(&url);

    let resp = client
        .propfind(
            "",
            Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:current-user-principal/>
  </D:prop>
</D:propfind>"#,
        )
        .await
        .expect("unauthenticated PROPFIND must complete");
    let status = resp.status();
    assert_basic_challenge(
        status,
        resp.headers().get("www-authenticate"),
        "unauthenticated PROPFIND",
    );
    let body = String::from_utf8_lossy(resp.body()).to_ascii_lowercase();
    assert!(
        !body.contains("<d:href") && !body.contains("<href"),
        "the 401 body must not leak principal information, got: {body}"
    );
    println!("unauthenticated PROPFIND: 401 with Basic challenge, principal not leaked");
}
