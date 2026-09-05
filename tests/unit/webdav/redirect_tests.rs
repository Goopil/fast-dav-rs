use fast_dav_rs::webdav::client::{
    ensure_redirect_allowed, is_https_to_http_downgrade, redirect_target_not_https,
    resolve_location, same_origin,
};
use fast_dav_rs::{Error, WebDavClient};
use hyper::{HeaderMap, Method, header};

const REDIRECT_302: &str =
    "HTTP/1.1 302 Found\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const REDIRECT_303: &str =
    "HTTP/1.1 303 See Other\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const REDIRECT_307: &str = "HTTP/1.1 307 Temporary Redirect\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

fn redirect_head(template: &str, location: &str) -> String {
    template.replace("{loc}", location)
}

#[test]
fn resolve_location_variants() {
    let base: hyper::Uri = "http://127.0.0.1:9000/base/cal/".parse().unwrap();

    // Root-relative path.
    let u = resolve_location(&base, "/final/").unwrap();
    assert_eq!(u.to_string(), "http://127.0.0.1:9000/final/");

    // Relative segment reference (RFC 3986 §5 merge).
    let u = resolve_location(&base, "new/").unwrap();
    assert_eq!(u.to_string(), "http://127.0.0.1:9000/base/cal/new/");

    // Bare query reference keeps the current path.
    let u = resolve_location(&base, "?page=2").unwrap();
    assert_eq!(u.to_string(), "http://127.0.0.1:9000/base/cal/?page=2");

    // Fragments are stripped before the request.
    let u = resolve_location(&base, "/final/#frag").unwrap();
    assert_eq!(u.to_string(), "http://127.0.0.1:9000/final/");

    // Relative reference with a query string.
    let u = resolve_location(&base, "new?q=1").unwrap();
    assert_eq!(u.to_string(), "http://127.0.0.1:9000/base/cal/new?q=1");

    // Absolute URLs are taken as-is (any scheme).
    let u = resolve_location(&base, "https://other.example.com/x").unwrap();
    assert_eq!(u.to_string(), "https://other.example.com/x");
}

#[test]
fn resolve_location_rfc3986_5_4_merge_and_dot_segment_matrix() {
    // RFC 3986 §5.4.1 normal examples, base = "http://a/b/c/d;p?q".
    let base: hyper::Uri = "http://a/b/c/d;p?q".parse().unwrap();
    let r = |loc: &str| resolve_location(&base, loc).unwrap().to_string();

    assert_eq!(r("g"), "http://a/b/c/g");
    assert_eq!(r("./g"), "http://a/b/c/g");
    assert_eq!(r("g/"), "http://a/b/c/g/");
    assert_eq!(r("/g"), "http://a/g");
    assert_eq!(r("?y"), "http://a/b/c/d;p?y");
    assert_eq!(r("g?y"), "http://a/b/c/g?y");
    assert_eq!(r("g?y#s"), "http://a/b/c/g?y");
    // An empty reference keeps the base path (the base query is not
    // inherited — pre-existing behavior outside issue #139's scope).
    assert_eq!(r(""), "http://a/b/c/d;p");
    assert_eq!(r("."), "http://a/b/c/");
    assert_eq!(r("./"), "http://a/b/c/");
    assert_eq!(r(".."), "http://a/b/");
    assert_eq!(r("../"), "http://a/b/");
    assert_eq!(r("../g"), "http://a/b/g");
    assert_eq!(r("../.."), "http://a/");
    assert_eq!(r("../../"), "http://a/");
    assert_eq!(r("../../g"), "http://a/g");

    // §5.4.2 abnormal examples: climbing past the root is ignored.
    assert_eq!(r("../../../g"), "http://a/g");
    assert_eq!(r("../../../../g"), "http://a/g");
    assert_eq!(r("/./g"), "http://a/g");
    assert_eq!(r("/../g"), "http://a/g");

    // Dot-segments must be removed wherever they appear.
    assert_eq!(r("g."), "http://a/b/c/g.");
    assert_eq!(r(".g"), "http://a/b/c/.g");
    assert_eq!(r("g.."), "http://a/b/c/g..");
    assert_eq!(r("./../g"), "http://a/b/g");
    assert_eq!(r("./g."), "http://a/b/c/g.");
    assert_eq!(r("g/./h"), "http://a/b/c/g/h");
    assert_eq!(r("g/../h"), "http://a/b/c/h");
    assert_eq!(r("g;x=1/./y"), "http://a/b/c/g;x=1/y");
    assert_eq!(r("g;x=1/../y"), "http://a/b/c/y");
    assert_eq!(r("g?y/./x"), "http://a/b/c/g?y/./x");
    assert_eq!(r("g?y/../x"), "http://a/b/c/g?y/../x");
    assert_eq!(r("g#s/./x"), "http://a/b/c/g");

    // Empty segments are not dot-segments: internal `//` is preserved.
    assert_eq!(r("/a//b/../c"), "http://a/a//c");
}

#[test]
fn resolve_location_dot_segments_in_issue_scenario() {
    // Issue #139: `Location: ../caldav/` from `/.well-known/caldav` must
    // resolve to `/caldav/`, not the literal `/.well-known/../caldav/`.
    let base: hyper::Uri = "http://h.example/.well-known/caldav".parse().unwrap();
    let u = resolve_location(&base, "../caldav/").unwrap();
    assert_eq!(u.to_string(), "http://h.example/caldav/");
}

#[test]
fn resolve_location_network_path_reference() {
    // RFC 3986 §4.2: `//host/path` keeps the current scheme.
    let http: hyper::Uri = "http://127.0.0.1:9000/base/".parse().unwrap();
    let u = resolve_location(&http, "//mirror.example.com/dav/").unwrap();
    assert_eq!(u.to_string(), "http://mirror.example.com/dav/");

    let https: hyper::Uri = "https://dav.example/dav/".parse().unwrap();
    let u = resolve_location(&https, "//mirror.example.com/dav/").unwrap();
    assert_eq!(u.to_string(), "https://mirror.example.com/dav/");
}

#[test]
fn resolve_location_uppercase_scheme_is_absolute() {
    // RFC 3986 §3.1: the scheme is case-insensitive.
    let base: hyper::Uri = "http://127.0.0.1:9000/base/".parse().unwrap();
    let u = resolve_location(&base, "HTTPS://Other.Example.com/x").unwrap();
    assert_eq!(
        u.scheme_str(),
        Some("https"),
        "Uri canonicalizes the scheme"
    );
    assert_eq!(u.host(), Some("Other.Example.com"));
    assert_eq!(u.path(), "/x");

    let u = resolve_location(&base, "Http://other.example.com/y").unwrap();
    assert_eq!(u.scheme_str(), Some("http"));
    assert_eq!(u.path(), "/y");
}

#[test]
fn resolve_location_unresolvable_returns_none() {
    let base: hyper::Uri = "http://127.0.0.1:9000/base/".parse().unwrap();

    // Malformed absolute URL.
    assert!(resolve_location(&base, "http://").is_none());

    // Scheme-less current URI has nothing to resolve against.
    let schemeless: hyper::Uri = "/onlypath".parse().unwrap();
    assert!(resolve_location(&schemeless, "/x").is_none());
    // …including for network-path references, which need the scheme.
    assert!(resolve_location(&schemeless, "//mirror/x").is_none());
}

#[test]
fn same_origin_host_comparison_is_case_insensitive() {
    let upper: hyper::Uri = "http://H.Example/".parse().unwrap();
    let lower: hyper::Uri = "http://h.example/".parse().unwrap();
    assert!(
        same_origin(&upper, &lower),
        "host is case-insensitive (RFC 3986 §3.2.2): credentials must survive"
    );
    assert!(same_origin(&lower, &upper));

    let other: hyper::Uri = "http://a.example/".parse().unwrap();
    assert!(!same_origin(&upper, &other));
}

#[test]
fn https_to_http_downgrade_is_detected() {
    let https: hyper::Uri = "https://dav.example/a".parse().unwrap();
    let http: hyper::Uri = "http://dav.example/a".parse().unwrap();
    let https_other_path: hyper::Uri = "https://dav.example/b".parse().unwrap();

    assert!(is_https_to_http_downgrade(&https, &http));
    assert!(!is_https_to_http_downgrade(&https, &https_other_path));
    assert!(
        !is_https_to_http_downgrade(&http, &http),
        "http→http is not a downgrade"
    );
}

#[test]
fn same_origin_variants() {
    let http: hyper::Uri = "http://h.example/".parse().unwrap();
    let http_explicit_port: hyper::Uri = "http://h.example:80/".parse().unwrap();
    let https: hyper::Uri = "https://h.example/".parse().unwrap();
    let other_port: hyper::Uri = "http://h.example:8080/".parse().unwrap();
    let other_host: hyper::Uri = "http://a.example/".parse().unwrap();
    let schemeless: hyper::Uri = "/onlypath".parse().unwrap();

    // Default port (80) is equivalent to an explicit :80.
    assert!(same_origin(&http, &http_explicit_port));
    assert!(!same_origin(&http, &https), "scheme mismatch");
    assert!(!same_origin(&http, &other_port), "port mismatch");
    assert!(!same_origin(&http, &other_host), "host mismatch");
    // Scheme-less URIs fall back to the unknown-port arm.
    assert!(same_origin(&schemeless, &schemeless));
    assert!(!same_origin(&schemeless, &http));
}

#[test]
fn require_https_redirect_guard_rejects_non_https_targets() {
    let https: hyper::Uri = "https://dav.example/a".parse().unwrap();
    let http: hyper::Uri = "http://dav.example/a".parse().unwrap();
    let ftp: hyper::Uri = "ftp://dav.example/a".parse().unwrap();

    assert!(
        redirect_target_not_https(&http),
        "http redirect target must be rejected (https→http downgrade)"
    );
    assert!(
        redirect_target_not_https(&ftp),
        "any non-https redirect target must be rejected"
    );
    assert!(
        !redirect_target_not_https(&https),
        "an https redirect target is followable under require_https"
    );
}

#[test]
fn ensure_redirect_allowed_rejects_non_https_targets_when_flag_on() {
    let userinfo_http: hyper::Uri = "http://user:pass@dav.example/a".parse().unwrap();
    let ftp: hyper::Uri = "ftp://dav.example/a".parse().unwrap();

    let Err(err) = ensure_redirect_allowed(true, &userinfo_http) else {
        panic!("http target must be rejected when require_https is on");
    };
    assert!(
        matches!(err, Error::InvalidInput(ref msg) if msg.contains("require_https")),
        "should be InvalidInput mentioning require_https, got: {err}"
    );
    assert!(
        !err.to_string().contains("user:pass"),
        "error display must not echo userinfo: {err}"
    );

    assert!(ensure_redirect_allowed(true, &ftp).is_err());
}

#[test]
fn ensure_redirect_allowed_accepts_https_target_when_flag_on() {
    let https: hyper::Uri = "https://dav.example/a".parse().unwrap();
    assert!(
        ensure_redirect_allowed(true, &https).is_ok(),
        "an https redirect target is followable under require_https"
    );
}

#[test]
fn ensure_redirect_allowed_accepts_everything_when_flag_off() {
    let http: hyper::Uri = "http://dav.example/a".parse().unwrap();
    let ftp: hyper::Uri = "ftp://dav.example/a".parse().unwrap();
    assert!(
        ensure_redirect_allowed(false, &http).is_ok(),
        "with the flag off, non-https targets are not policed by this guard"
    );
    assert!(ensure_redirect_allowed(false, &ftp).is_ok());
}

fn make_client(base: &str) -> WebDavClient {
    let client = WebDavClient::builder(base).build().unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);
    client
}

#[tokio::test]
async fn redirect_302_same_origin_follows_and_keeps_auth() {
    let ok_body = b"ok".to_vec();
    let second = (
        crate::common::http_helpers::response_head("", ok_body.len()),
        ok_body,
    );
    let first = (redirect_head(REDIRECT_302, "/final/"), Vec::new());
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![first, second]).await;

    let client = WebDavClient::builder(&base)
        .basic_auth("user", "pass")
        .build()
        .unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body().as_ref(), b"ok");

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "both hops must be captured: {reqs:?}");
    let first = String::from_utf8_lossy(&reqs[0]);
    let second = String::from_utf8_lossy(&reqs[1]);
    assert!(
        first.contains("GET / HTTP/1.1"),
        "first hop should target the base path: {first}"
    );
    assert!(
        second.contains("GET /final/ HTTP/1.1"),
        "second hop should target the Location path: {second}"
    );
    assert!(
        first.to_ascii_lowercase().contains("authorization: basic"),
        "auth must be sent to the same origin: {first}"
    );
    assert!(
        second.to_ascii_lowercase().contains("authorization: basic"),
        "auth must survive same-origin redirects: {second}"
    );
}

#[tokio::test]
async fn redirect_303_switches_to_get_and_drops_body() {
    let done = b"done".to_vec();
    let second = (
        crate::common::http_helpers::response_head("", done.len()),
        done,
    );
    let first = (redirect_head(REDIRECT_303, "/get-here/"), Vec::new());
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![first, second]).await;
    let client = make_client(&base);

    let resp = client
        .send(
            Method::POST,
            "",
            HeaderMap::new(),
            Some(bytes::Bytes::from_static(b"POST-BODY")),
            None,
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body().as_ref(), b"done");

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "both hops must be captured: {reqs:?}");
    let second_req = String::from_utf8_lossy(&reqs[1]);
    assert!(
        second_req.contains("GET /get-here/ HTTP/1.1"),
        "303 must switch the method to GET: {second_req}"
    );
    assert!(
        !second_req.contains("POST-BODY"),
        "303 must drop the request body: {second_req}"
    );
    assert!(
        !second_req.to_ascii_lowercase().contains("content-type:"),
        "303 must drop content headers alongside the body: {second_req}"
    );
}

#[tokio::test]
async fn redirect_307_keeps_method_and_body() {
    let done = b"done".to_vec();
    let second = (
        crate::common::http_helpers::response_head("", done.len()),
        done,
    );
    let first = (redirect_head(REDIRECT_307, "/elsewhere/"), Vec::new());
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![first, second]).await;
    let client = make_client(&base);

    let resp = client
        .send(
            Method::POST,
            "",
            HeaderMap::new(),
            Some(bytes::Bytes::from_static(b"POST-PAYLOAD")),
            None,
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "both hops must be captured: {reqs:?}");
    let second_req = String::from_utf8_lossy(&reqs[1]);
    assert!(
        second_req.contains("POST /elsewhere/ HTTP/1.1"),
        "307 must keep the method: {second_req}"
    );
    assert!(
        second_req.contains("POST-PAYLOAD"),
        "307 must keep the request body: {second_req}"
    );
}

#[tokio::test]
async fn redirect_cross_origin_strips_authorization() {
    // Destination server: answers 200 and captures the redirected request.
    let ok_body = b"ok".to_vec();
    let (target_base, captured_b) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", ok_body.len()),
        ok_body,
    )
    .await;

    // Origin server: redirects (absolute URL) to the destination server.
    let location = format!("{target_base}target");
    let (origin_base, captured_a) = crate::common::http_helpers::serve_capture(
        redirect_head(REDIRECT_302, &location),
        Vec::new(),
    )
    .await;

    let client = WebDavClient::builder(&origin_base)
        .basic_auth("user", "pass")
        .build()
        .unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body().as_ref(), b"ok");

    let guard_a = captured_a.lock().unwrap();
    let guard_b = captured_b.lock().unwrap();
    let first = String::from_utf8_lossy(&guard_a);
    let second = String::from_utf8_lossy(&guard_b);
    assert!(
        first.to_ascii_lowercase().contains("authorization: basic"),
        "auth must be sent to the original origin: {first}"
    );
    assert!(
        !second.to_ascii_lowercase().contains("authorization:"),
        "auth must be stripped on cross-origin redirects: {second}"
    );
    assert!(
        !second.to_ascii_lowercase().contains("cookie:"),
        "cookies must be stripped on cross-origin redirects: {second}"
    );
}

#[tokio::test]
async fn redirect_cross_origin_strips_conditional_headers() {
    // Destination server: answers 200 and captures the redirected request.
    let ok_body = b"ok".to_vec();
    let (target_base, captured_b) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", ok_body.len()),
        ok_body,
    )
    .await;

    // Origin server: redirects (absolute URL) to the destination server.
    let location = format!("{target_base}target");
    let (origin_base, _captured_a) = crate::common::http_helpers::serve_capture(
        redirect_head(REDIRECT_302, &location),
        Vec::new(),
    )
    .await;

    let client = WebDavClient::builder(&origin_base).build().unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let mut headers = HeaderMap::new();
    headers.insert(header::IF_MATCH, "\"v1\"".parse().unwrap());
    headers.insert(header::IF_NONE_MATCH, "\"v2\"".parse().unwrap());

    let resp = client
        .send(Method::GET, "", headers, None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let guard = captured_b.lock().unwrap();
    let second = String::from_utf8_lossy(&guard);
    assert!(
        !second.to_ascii_lowercase().contains("if-match:"),
        "If-Match must not leak across origins (RFC 9110 §13.1.1): {second}"
    );
    assert!(
        !second.to_ascii_lowercase().contains("if-none-match:"),
        "If-None-Match must not leak across origins (RFC 9110 §13.1.1): {second}"
    );
}

#[tokio::test]
async fn redirect_follow_disabled_returns_redirect_response() {
    let location = "http://127.0.0.1:1/never-requested/";
    let (base, captured) = crate::common::http_helpers::serve_capture(
        redirect_head(REDIRECT_302, location),
        Vec::new(),
    )
    .await;

    let client = WebDavClient::builder(&base)
        .follow_redirects(false)
        .build()
        .unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("location").and_then(|v| v.to_str().ok()),
        Some(location)
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        !req.contains("/never-requested/"),
        "the redirect target must not be requested: {req}"
    );
}

#[tokio::test]
async fn redirect_loop_exceeding_max_redirects_errors() {
    let base =
        crate::common::http_helpers::serve_always(redirect_head(REDIRECT_302, "/"), Vec::new())
            .await;

    let client = WebDavClient::builder(&base)
        .max_redirects(2)
        .build()
        .unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let err = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::TooManyRedirects { limit, .. } if limit == 2),
        "expected TooManyRedirects with limit 2, got: {err:?}"
    );
    assert!(
        err.to_string().contains("2 redirects"),
        "display should mention the limit, got: {err}"
    );
}

#[tokio::test]
async fn redirect_without_location_returns_response_as_is() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        "HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let client = make_client(&base);

    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        302,
        "a Location-less redirect is returned as-is"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("GET / HTTP/1.1"),
        "exactly one request must have been sent: {req}"
    );
}

#[tokio::test]
async fn redirect_is_followed_on_streaming_path() {
    let streamed = b"streamed".to_vec();
    let second = (
        crate::common::http_helpers::response_head("", streamed.len()),
        streamed,
    );
    let first = (redirect_head(REDIRECT_302, "/stream/"), Vec::new());
    let (base, captured) = crate::common::http_helpers::serve_sequence(vec![first, second]).await;
    let client = make_client(&base);

    let resp = client
        .send_stream(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    use http_body_util::BodyExt;
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"streamed");

    let reqs = captured.lock().unwrap();
    assert_eq!(reqs.len(), 2, "both hops must be captured: {reqs:?}");
    let second_req = String::from_utf8_lossy(&reqs[1]);
    assert!(
        second_req.contains("GET /stream/ HTTP/1.1"),
        "streaming path must follow the redirect: {second_req}"
    );
}
