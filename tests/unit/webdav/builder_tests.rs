use fast_dav_rs::{ContentEncoding, RequestCompressionMode, WebDavClient};
use std::str::FromStr;
use std::time::Duration;

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

#[cfg(feature = "dangerous")]
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
