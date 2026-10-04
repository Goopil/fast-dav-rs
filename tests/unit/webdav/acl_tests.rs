use fast_dav_rs::{Error, RequestCompressionMode, WebDavClient};

#[tokio::test]
async fn acl_success_sends_method_and_body() {
    let (base, captured) = crate::common::http_helpers::serve_capture(
        crate::common::http_helpers::response_head("", 0),
        Vec::new(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client
        .acl(
            "principals/users/test/calendar-proxy-read/",
            "<D:ace>...</D:ace>",
        )
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let guard = captured.lock().unwrap();
    let req = String::from_utf8_lossy(&guard);
    assert!(req.starts_with("ACL "), "expected ACL method: {req}");
    assert!(
        req.to_ascii_lowercase().contains("depth: 0"),
        "expected 'Depth: 0': {req}"
    );
    assert!(req.contains("<D:ace>"), "expected body: {req}");
}

#[tokio::test]
async fn acl_non_success_maps_to_unexpected_status() {
    let base = crate::common::http_helpers::serve_once(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
        Vec::new(),
    )
    .await;
    let client = WebDavClient::new(&base, None, None).unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    let err = client.acl("p/", "<D:ace/>").await.unwrap_err();
    assert!(
        matches!(err, Error::UnexpectedStatus { .. }),
        "got: {err:?}"
    );
}
