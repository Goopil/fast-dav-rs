use fast_dav_rs::common::http::MaybeProxied;
use fast_dav_rs::{
    CalDavClient, CardDavClient, ContentEncoding, Error, HyperClient, RequestCompressionMode,
    WebDavClient,
};
use hyper::{HeaderMap, Method, StatusCode};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use std::str::FromStr;
use std::time::Duration;

use crate::common::http_helpers::{response_head, serve_capture};

const BASE: &str = "https://dav.example.com/user01/";

#[test]
fn builder_defaults() {
    let client = WebDavClient::builder(BASE).build().expect("build succeeds");
    assert_eq!(
        client.request_compression_mode(),
        RequestCompressionMode::Auto
    );
}

#[test]
fn builder_basic_auth() {
    let client = WebDavClient::builder(BASE)
        .basic_auth("user", "pass")
        .build()
        .expect("build succeeds");
    // Verifying via the public API: the client builds successfully
    let _ = client;
}

#[test]
fn builder_bearer_auth() {
    let client = WebDavClient::builder(BASE)
        .bearer_token("token123")
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_auth_last_wins() {
    let client = WebDavClient::builder(BASE)
        .basic_auth("user", "pass")
        .bearer_token("token")
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_no_auth() {
    let client = WebDavClient::builder(BASE).build().expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_invalid_url_errors() {
    assert!(WebDavClient::builder("not a url").build().is_err());
}

#[test]
fn builder_rejects_userinfo_in_base_url() {
    let Err(err) = WebDavClient::builder("https://user:hunter2@dav.example.com/").build() else {
        panic!("userinfo in the base URL must be rejected");
    };
    assert!(
        matches!(err, Error::InvalidConfig(ref msg) if msg.contains("userinfo")),
        "should be InvalidConfig about userinfo, got: {err}"
    );
    assert!(
        !err.to_string().contains("hunter2"),
        "error display must not echo the password: {err}"
    );
}

#[test]
fn builder_rejects_userinfo_caldav() {
    let Err(err) = CalDavClient::builder("https://user:hunter2@cal.example.com/").build() else {
        panic!("userinfo in the base URL must be rejected");
    };
    assert!(
        matches!(err, Error::InvalidConfig(ref msg) if msg.contains("userinfo")),
        "should be InvalidConfig about userinfo, got: {err}"
    );
}

#[test]
fn builder_rejects_userinfo_carddav() {
    let Err(err) = CardDavClient::builder("https://user:hunter2@cards.example.com/").build() else {
        panic!("userinfo in the base URL must be rejected");
    };
    assert!(
        matches!(err, Error::InvalidConfig(ref msg) if msg.contains("userinfo")),
        "should be InvalidConfig about userinfo, got: {err}"
    );
}

#[test]
fn builder_require_https_rejects_http_base_url() {
    let Err(err) = WebDavClient::builder("http://dav.example.com/")
        .require_https(true)
        .build()
    else {
        panic!("an http:// base URL must be rejected with require_https enabled");
    };
    assert!(
        matches!(err, Error::InvalidConfig(ref msg) if msg.contains("require_https")),
        "should be InvalidConfig mentioning require_https, got: {err}"
    );
}

#[test]
fn builder_require_https_accepts_https_base_url() {
    let client = WebDavClient::builder(BASE)
        .require_https(true)
        .build()
        .expect("an https:// base URL is accepted with require_https enabled");
    let _ = client;
}

#[test]
fn builder_require_https_default_off_accepts_http_base_url() {
    let client = WebDavClient::builder("http://dav.example.com/")
        .build()
        .expect("default behavior unchanged: http:// base accepted without the flag");
    let _ = client;
}

#[test]
fn builder_require_https_threads_through_caldav_macro() {
    let Err(err) = CalDavClient::builder("http://cal.example.com/")
        .require_https(true)
        .build()
    else {
        panic!("an http:// base URL must be rejected via the CalDAV builder");
    };
    assert!(
        matches!(err, Error::InvalidConfig(ref msg) if msg.contains("require_https")),
        "should be InvalidConfig mentioning require_https, got: {err}"
    );
}

#[test]
fn builder_require_https_threads_through_carddav_macro() {
    let Err(err) = CardDavClient::builder("http://cards.example.com/")
        .require_https(true)
        .build()
    else {
        panic!("an http:// base URL must be rejected via the CardDAV builder");
    };
    assert!(
        matches!(err, Error::InvalidConfig(ref msg) if msg.contains("require_https")),
        "should be InvalidConfig mentioning require_https, got: {err}"
    );
}

#[test]
fn invalid_url_display_redacts_userinfo() {
    // Malformed URL that also carries userinfo: the parse error must not
    // echo the credentials (belt-and-braces beside the builder rejection).
    let Err(err) = WebDavClient::builder("https://user:hunter2@bad host/").build() else {
        panic!("an invalid URL with userinfo must be rejected");
    };
    assert!(
        matches!(err, Error::InvalidUrl { .. }),
        "should be InvalidUrl, got: {err}"
    );
    assert!(
        !err.to_string().contains("hunter2"),
        "error display must not echo the password: {err}"
    );
}

#[test]
fn builder_timeout_zero_errors() {
    assert!(
        WebDavClient::builder(BASE)
            .timeout(Duration::ZERO)
            .build()
            .is_err()
    );
}

#[test]
fn builder_pool_zero_errors() {
    assert!(
        WebDavClient::builder(BASE)
            .pool_max_idle_per_host(0)
            .build()
            .is_err()
    );
}

#[test]
fn builder_clone_shares_compression() {
    let client_a = WebDavClient::builder(BASE)
        .request_compression(RequestCompressionMode::Force(ContentEncoding::Zstd))
        .build()
        .unwrap();
    let client_b = client_a.clone();

    client_a.set_request_compression_mode(RequestCompressionMode::Disabled);

    assert_eq!(
        client_b.request_compression_mode(),
        RequestCompressionMode::Disabled
    );
}

#[test]
fn builder_set_compression_without_mut() {
    let client = WebDavClient::builder(BASE).build().unwrap();
    // This should compile without `mut`
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    assert_eq!(
        client.request_compression_mode(),
        RequestCompressionMode::Disabled
    );
}

#[test]
fn builder_force_http1() {
    let client = WebDavClient::builder(BASE)
        .force_http1(true)
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_with_proxy() {
    let client = WebDavClient::builder(BASE)
        .proxy(hyper::Uri::from_str("http://127.0.0.1:9090").unwrap())
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_with_proxy_auth() {
    let client = WebDavClient::builder(BASE)
        .proxy(hyper::Uri::from_str("http://127.0.0.1:9090").unwrap())
        .proxy_basic_auth("proxyuser", "proxypass")
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_danger_accept_invalid_certs() {
    let client = WebDavClient::builder(BASE)
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_user_agent() {
    let client = WebDavClient::builder(BASE)
        .user_agent("MyApp/1.0")
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_connect_timeout() {
    let client = WebDavClient::builder(BASE)
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_follow_redirects_and_max_redirects() {
    let client = WebDavClient::builder(BASE)
        .follow_redirects(true)
        .max_redirects(10)
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_pool_idle_timeout() {
    let client = WebDavClient::builder(BASE)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_extra_root_certs_empty() {
    let client = WebDavClient::builder(BASE)
        .extra_root_certs_pem(vec![])
        .build()
        .expect("build succeeds");
    let _ = client;
}

#[test]
fn builder_bearer_token_empty_errors() {
    let result = WebDavClient::builder(BASE).bearer_token("").build();
    assert!(result.is_err());
}

#[test]
fn builder_bearer_token_invalid_chars_errors() {
    let result = WebDavClient::builder(BASE)
        .bearer_token("token with spaces")
        .build();
    assert!(result.is_err());
}

#[test]
fn builder_basic_auth_empty_user_errors() {
    let result = WebDavClient::builder(BASE).basic_auth("", "pass").build();
    assert!(result.is_err());
}

#[test]
fn builder_basic_auth_empty_pass_errors() {
    let result = WebDavClient::builder(BASE).basic_auth("user", "").build();
    assert!(result.is_err());
}

#[test]
fn builder_proxy_basic_auth_empty_user_errors() {
    let result = WebDavClient::builder(BASE)
        .proxy(hyper::Uri::from_str("http://127.0.0.1:9090").unwrap())
        .proxy_basic_auth("", "pass")
        .build();
    assert!(result.is_err());
}

#[test]
fn builder_proxy_basic_auth_empty_pass_errors() {
    let result = WebDavClient::builder(BASE)
        .proxy(hyper::Uri::from_str("http://127.0.0.1:9090").unwrap())
        .proxy_basic_auth("user", "")
        .build();
    assert!(result.is_err());
}

#[test]
fn builder_proxy_basic_auth_without_proxy_errors() {
    let result = WebDavClient::builder(BASE)
        .proxy_basic_auth("user", "pass")
        .build();
    assert!(result.is_err());
}

#[tokio::test]
async fn injected_hyper_client_drives_requests() {
    let (base, captured) = serve_capture(response_head("", 5), b"hello".to_vec()).await;

    let mut http = HttpConnector::new();
    http.enforce_http(false);
    let https = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .wrap_connector(MaybeProxied::direct(http));
    let injected: HyperClient = Client::builder(TokioExecutor::new())
        .pool_max_idle_per_host(1)
        .build(https);

    let client = WebDavClient::builder(base)
        .with_hyper_client(injected)
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();
    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.body().as_ref(), b"hello");

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(req.contains("GET / HTTP/1.1"), "captured request: {req}");
}

#[tokio::test]
async fn injected_hyper_client_replaces_internal_transport() {
    let (base, captured) = serve_capture(response_head("", 5), b"hello".to_vec()).await;

    let https = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .wrap_connector(MaybeProxied::direct(HttpConnector::new()));
    let injected: HyperClient = Client::builder(TokioExecutor::new()).build(https);

    let https_base = base.replacen("http://", "https://", 1);
    let client = WebDavClient::builder(https_base)
        .with_hyper_client(injected)
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    let result = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await;
    assert!(result.is_err(), "injected connector must reject https URIs");
    assert!(
        captured.lock().unwrap().is_empty(),
        "the mock must receive no bytes: the injected client, not the internal one, served the request"
    );
}

fn injected_http1_client() -> HyperClient {
    let https = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .wrap_connector(MaybeProxied::direct(HttpConnector::new()));
    Client::builder(TokioExecutor::new()).build(https)
}

#[tokio::test]
async fn injected_hyper_client_caldav_builder() {
    let (base, captured) = serve_capture(response_head("", 5), b"hello".to_vec()).await;

    let client = CalDavClient::builder(base)
        .with_hyper_client(injected_http1_client())
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();
    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.body().as_ref(), b"hello");
    assert!(
        String::from_utf8_lossy(&captured.lock().unwrap()).contains("GET / HTTP/1.1"),
        "the injected client must serve the request"
    );
}

#[tokio::test]
async fn injected_hyper_client_carddav_builder() {
    let (base, captured) = serve_capture(response_head("", 5), b"hello".to_vec()).await;

    let client = CardDavClient::builder(base)
        .with_hyper_client(injected_http1_client())
        .request_compression(RequestCompressionMode::Disabled)
        .build()
        .unwrap();
    let resp = client
        .send(Method::GET, "", HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.body().as_ref(), b"hello");
    assert!(
        String::from_utf8_lossy(&captured.lock().unwrap()).contains("GET / HTTP/1.1"),
        "the injected client must serve the request"
    );
}
