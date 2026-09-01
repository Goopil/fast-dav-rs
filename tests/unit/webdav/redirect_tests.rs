use fast_dav_rs::webdav::client::{resolve_location, same_origin};
use fast_dav_rs::{Error, WebDavClient};
use hyper::{HeaderMap, Method};

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
fn resolve_location_unresolvable_returns_none() {
    let base: hyper::Uri = "http://127.0.0.1:9000/base/".parse().unwrap();

    // Malformed absolute URL.
    assert!(resolve_location(&base, "http://").is_none());

    // Scheme-less current URI has nothing to resolve against.
    let schemeless: hyper::Uri = "/onlypath".parse().unwrap();
    assert!(resolve_location(&schemeless, "/x").is_none());
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
