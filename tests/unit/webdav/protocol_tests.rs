use fast_dav_rs::WebDavClient;

#[tokio::test]
async fn proppatch_sends_depth_zero() {
    let body = b"".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;

    let client = WebDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let resp = client
        .proppatch(
            "cal/event.ics",
            r#"<D:propertyupdate xmlns:D="DAV:"><D:set><D:prop><D:displayname>New</D:displayname></D:prop></D:set></D:propertyupdate>"#,
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.contains("PROPPATCH"),
        "expected PROPPATCH method in request: {req}"
    );
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected explicit 'Depth: 0' on PROPPATCH (RFC 4918 §9.2): {req}"
    );
}

#[tokio::test]
async fn send_attaches_configured_user_agent() {
    let body = b"".to_vec();
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", body.len()),
        body,
    )
    .await;

    let client = WebDavClient::builder(&base)
        .user_agent("fast-dav-tests/1.0")
        .build()
        .unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let resp = client.get("").await.unwrap();
    assert_eq!(resp.status(), 200);

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(
        req.to_ascii_lowercase()
            .contains("user-agent: fast-dav-tests/1.0"),
        "expected the configured User-Agent on the wire: {req}"
    );
}

#[tokio::test]
async fn send_returns_empty_head_body_without_decompressing() {
    // RFC 9110 §9.3.2: a HEAD response may advertise `Content-Encoding` while
    // carrying an empty body. Feeding that to a decoder fails, and the
    // header rewrite would mask the server-reported `Content-Length`: the
    // empty body must be returned as-is, headers untouched.
    let (base, captured) = crate::common::http_helpers::serve_capture(
        "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned(),
        Vec::new(),
    )
    .await;

    let client = WebDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(fast_dav_rs::RequestCompressionMode::Disabled);

    let resp = client
        .send(hyper::Method::HEAD, "", hyper::HeaderMap::new(), None, None)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.body().is_empty(), "the HEAD body must stay empty");
    assert_eq!(
        resp.headers().get("content-encoding").unwrap(),
        "gzip",
        "the advertised Content-Encoding must be left untouched"
    );
    assert_eq!(
        resp.headers().get("content-length").unwrap(),
        "0",
        "the server-reported Content-Length must not be rewritten"
    );

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(req.starts_with("HEAD"), "the request must be a HEAD: {req}");
}
