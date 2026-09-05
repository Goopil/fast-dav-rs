use fast_dav_rs::webdav::{discover_caldav, discover_carddav};
use fast_dav_rs::{Error, Operation, RequestCompressionMode, WebDavClient};

use crate::common::http_helpers::{
    response_head, serve_always, serve_capture, serve_sequence, unreachable_base,
};

const REDIRECT_301: &str = "HTTP/1.1 301 Moved Permanently\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const REDIRECT_307: &str = "HTTP/1.1 307 Temporary Redirect\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

fn redirect_head(template: &str, location: &str) -> String {
    template.replace("{loc}", location)
}

fn make_client(base: &str) -> WebDavClient {
    let client = WebDavClient::builder(base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

#[tokio::test]
async fn discover_caldav_follows_multi_hop_redirects_to_final_url() {
    let ok_body = b"ok".to_vec();
    let first = (redirect_head(REDIRECT_301, "/a/"), Vec::new());
    let second = (redirect_head(REDIRECT_307, "/cal/"), Vec::new());
    let third = (response_head("", ok_body.len()), ok_body);
    let (base, captured) = serve_sequence(vec![first, second, third]).await;
    let client = make_client(&base);

    let service_url = discover_caldav(&client).await.unwrap();
    assert_eq!(
        service_url,
        format!("{base}cal/"),
        "the final redirect target is the discovered service URL"
    );

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 3, "all three hops must be captured: {reqs:?}");
    let first_req = String::from_utf8_lossy(&reqs[0]);
    assert!(
        first_req.contains("PROPFIND /.well-known/caldav HTTP/1.1"),
        "the probe must target .well-known/caldav (RFC 6764 §5): {first_req}"
    );
    assert!(
        first_req.to_ascii_lowercase().contains("depth: 0"),
        "the probe must send Depth: 0 (RFC 6764 §6): {first_req}"
    );
}

#[tokio::test]
async fn discover_caldav_404_falls_back_to_base_url() {
    let (base, captured) = serve_capture(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);

    let service_url = discover_caldav(&client).await.unwrap();
    assert_eq!(
        service_url, base,
        "404 means the server does not advertise: fall back to the base URL"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("PROPFIND /.well-known/caldav HTTP/1.1"),
        "the probe must target .well-known/caldav (RFC 6764 §5): {req}"
    );
}

#[tokio::test]
async fn discover_caldav_direct_success_returns_base_url() {
    let ok_body = b"ok".to_vec();
    let (base, _captured) = serve_capture(response_head("", ok_body.len()), ok_body).await;
    let client = make_client(&base);

    let service_url = discover_caldav(&client).await.unwrap();
    assert_eq!(
        service_url, base,
        "a success served directly on .well-known is not the service endpoint \
         (RFC 5785 §1.1): fall back to the base URL"
    );
}

#[tokio::test]
async fn discover_caldav_redirect_with_follow_disabled_fails_with_clear_error() {
    // RFC 6764 §5 requires clients to handle .well-known redirects: with
    // `follow_redirects(false)` the probe returns the 3xx and discovery
    // must fail with an error that points at the cause (issue #139).
    let (base, captured) = serve_capture(redirect_head(REDIRECT_301, "/cal/"), Vec::new()).await;
    let client = WebDavClient::builder(&base)
        .follow_redirects(false)
        .build()
        .unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let err = discover_caldav(&client).await.unwrap_err();
    assert!(
        matches!(err, Error::Other { .. }),
        "an unfollowed redirect must surface as a descriptive error, got: {err:?}"
    );
    assert!(
        !matches!(err, Error::UnexpectedStatus { .. }),
        "a followable 3xx must not surface as a bare UnexpectedStatus, got: {err:?}"
    );
    assert!(
        err.to_string().contains("follow_redirects"),
        "the error should point at follow_redirects, got: {err}"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        !req.contains("PROPFIND /cal/"),
        "the redirect target must not be requested: {req}"
    );
}

#[tokio::test]
async fn discover_caldav_connection_refused_yields_typed_error() {
    let base = unreachable_base().await;
    let client = make_client(&base);

    let err = discover_caldav(&client).await.unwrap_err();
    assert!(
        matches!(err, Error::Connection(_)),
        "connection refusal must surface as a typed Error::Connection, got: {err:?}"
    );
}

#[tokio::test]
async fn discover_caldav_unexpected_status_is_typed_error() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);

    let err = discover_caldav(&client).await.unwrap_err();
    assert!(
        matches!(
            err,
            Error::UnexpectedStatus {
                operation: Operation::DiscoverWellKnownCaldav,
                ..
            }
        ),
        "a non-success, non-404 status must surface as Error::UnexpectedStatus, got: {err:?}"
    );
    assert!(
        err.to_string().contains(".well-known/caldav"),
        "display should name the failed probe, got: {err}"
    );
}

#[tokio::test]
async fn discover_carddav_unexpected_status_is_typed_error() {
    let (base, _captured) = serve_capture(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);

    let err = discover_carddav(&client).await.unwrap_err();
    assert!(
        matches!(
            err,
            Error::UnexpectedStatus {
                operation: Operation::DiscoverWellKnownCarddav,
                ..
            }
        ),
        "the CardDAV wrapper must map errors to DiscoverWellKnownCarddav, got: {err:?}"
    );
    assert!(
        err.to_string().contains(".well-known/carddav"),
        "display should name the failed probe, got: {err}"
    );
}

#[tokio::test]
async fn discover_carddav_probes_well_known_carddav_and_resolves() {
    let ok_body = b"ok".to_vec();
    let first = (redirect_head(REDIRECT_301, "/addr/"), Vec::new());
    let second = (response_head("", ok_body.len()), ok_body);
    let (base, captured) = serve_sequence(vec![first, second]).await;
    let client = make_client(&base);

    let service_url = discover_carddav(&client).await.unwrap();
    assert_eq!(service_url, format!("{base}addr/"));

    let reqs = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&reqs[0]);
    assert!(
        req.contains("PROPFIND /.well-known/carddav HTTP/1.1"),
        "the probe must target .well-known/carddav (RFC 6764 §5): {req}"
    );
}

#[tokio::test]
async fn discover_caldav_strips_userinfo_from_hostile_redirect_location() {
    // A hostile redirect hop controls the `Location` and may embed
    // credentials in it (RFC 3986 userinfo, RFC 6764 §5). The discovered
    // service URL must never echo them: strip the userinfo before the URL
    // leaves discovery, while keeping host and path intact.
    let ok_body = b"ok".to_vec();
    let target_base = serve_always(response_head("", ok_body.len()), ok_body).await;

    let hostile_location = format!(
        "{}caldav/",
        target_base.replacen("http://", "http://user:secret@", 1)
    );
    let (base, _captured) =
        serve_capture(redirect_head(REDIRECT_301, &hostile_location), Vec::new()).await;
    let client = make_client(&base);

    let service_url = discover_caldav(&client).await.unwrap();
    assert!(
        !service_url.contains("user"),
        "no username may leak into the discovered service URL"
    );
    assert!(
        !service_url.contains("secret"),
        "no password may leak into the discovered service URL"
    );
    assert!(
        service_url.starts_with(&target_base),
        "the discovered URL must still point at the redirect target host"
    );
    assert!(
        service_url.ends_with("/caldav/"),
        "the discovered service path must be preserved"
    );
}

// Provider-A-shaped fixtures (anonymous): the auth layer accepts the
// credentials — the server never answers 401 — but the
// `current-user-principal` PROPFIND returns 404. This is the signature of a
// wrong username form (e.g. an email address where the provider expects an
// internal short account ID) and must surface as a first-class, actionable
// error, not a bare `UnexpectedStatus`.

#[tokio::test]
async fn discover_current_user_principal_404_after_auth_is_principal_not_found() {
    let (base, captured) = serve_capture(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = WebDavClient::builder(&base)
        .basic_auth("user@example.com", "app-password")
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();

    let err = client
        .discover_current_user_principal()
        .await
        .expect_err("404 after successful auth must fail with PrincipalNotFound");

    match &err {
        Error::PrincipalNotFound { url, .. } => {
            assert_eq!(url, &base, "the error carries the probed (redacted) URL");
        }
        other => panic!("expected Error::PrincipalNotFound, got: {other:?}"),
    }
    let msg = err.to_string();
    assert!(
        msg.contains("404") && msg.contains("authentication succeeded"),
        "the message must name the auth-OK-but-404 failure mode: {msg}"
    );
    assert!(
        msg.contains("username form"),
        "the message must point at the wrong username form cause: {msg}"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase().contains("authorization: basic"),
        "the fixture shape requires credentials attached (auth layer passes, server \
         still answers 404 — never 401): {req}"
    );
    assert!(
        req.contains("PROPFIND / HTTP/1.1"),
        "the principal probe targets the authenticated root: {req}"
    );
}

#[tokio::test]
async fn discover_current_user_principal_401_stays_unexpected_status() {
    // A real credentials rejection must remain distinguishable from the
    // principal-404-after-auth failure mode.
    let (base, _captured) = serve_capture(
        "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"dav\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);

    let err = client.discover_current_user_principal().await.unwrap_err();
    assert!(
        matches!(err, Error::UnexpectedStatus { .. }),
        "401 is an auth failure and must NOT surface as PrincipalNotFound, got: {err:?}"
    );
}
