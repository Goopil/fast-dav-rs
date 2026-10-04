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

#[test]
fn build_acl_body_grant_deny() {
    use fast_dav_rs::webdav::Privilege;
    use fast_dav_rs::webdav::acl::{Ace, AcePrincipal};

    let body = fast_dav_rs::webdav::acl::build_acl_body(&[Ace {
        principal: AcePrincipal::Href("/principals/users/bob/".into()),
        grant: vec![Privilege::Read],
        deny: vec![Privilege::WriteContent],
        protected: false,
    }])
    .unwrap();
    assert!(body.contains("<D:acl xmlns:D=\"DAV:\">"));
    assert!(body.contains("<D:ace>"));
    assert!(body.contains("<D:principal><D:href>/principals/users/bob/</D:href></D:principal>"));
    assert!(body.contains("<D:grant><D:privilege><D:read/></D:privilege></D:grant>"));
    assert!(body.contains("<D:deny><D:privilege><D:write-content/></D:privilege></D:deny>"));
}

#[test]
fn build_acl_body_rejects_conflicting_ace_and_empty() {
    use fast_dav_rs::webdav::Privilege;
    use fast_dav_rs::webdav::acl::{Ace, AcePrincipal};
    let ace = Ace {
        principal: AcePrincipal::All,
        grant: vec![Privilege::Read],
        deny: vec![Privilege::Read],
        protected: false,
    };
    assert!(fast_dav_rs::webdav::acl::build_acl_body(&[ace]).is_err());
    assert!(fast_dav_rs::webdav::acl::build_acl_body(&[]).is_err());
}

#[test]
fn build_acl_body_principal_elements_and_protection() {
    use fast_dav_rs::webdav::Privilege;
    use fast_dav_rs::webdav::acl::{Ace, AcePrincipal};

    let body = fast_dav_rs::webdav::acl::build_acl_body(&[
        Ace {
            principal: AcePrincipal::SelfPrincipal,
            grant: vec![Privilege::Read],
            deny: Vec::new(),
            protected: true,
        },
        Ace {
            principal: AcePrincipal::Unauthenticated,
            grant: vec![Privilege::Other("all".into())],
            deny: Vec::new(),
            protected: false,
        },
        Ace {
            principal: AcePrincipal::All,
            grant: Vec::new(),
            deny: vec![Privilege::Unlock],
            protected: false,
        },
    ])
    .unwrap();
    assert!(body.contains("<D:principal><D:self/></D:principal>"));
    assert!(body.contains("<D:protected/>"));
    assert!(body.contains("<D:principal><D:unauthenticated/></D:principal>"));
    assert!(body.contains("<D:privilege><D:all/></D:privilege>"));
    assert!(body.contains("<D:principal><D:all/></D:principal>"));
    assert!(body.contains("<D:privilege><D:unlock/></D:privilege>"));
    assert!(body.ends_with("</D:acl>"));
}

#[test]
fn build_acl_body_escapes_href_and_rejects_bad_privilege() {
    use fast_dav_rs::webdav::Privilege;
    use fast_dav_rs::webdav::acl::{Ace, AcePrincipal};

    let body = fast_dav_rs::webdav::acl::build_acl_body(&[Ace {
        principal: AcePrincipal::Href("/principals/users/a<b>/".into()),
        grant: vec![Privilege::Read],
        deny: Vec::new(),
        protected: false,
    }])
    .unwrap();
    assert!(
        body.contains("<D:href>/principals/users/a&lt;b&gt;/</D:href>"),
        "hrefs must be XML-escaped: {body}"
    );

    let err = fast_dav_rs::webdav::acl::build_acl_body(&[Ace {
        principal: AcePrincipal::Href("/p/".into()),
        grant: vec![Privilege::Other("Not A Privilege".into())],
        deny: Vec::new(),
        protected: false,
    }])
    .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "got: {err:?}");
}
