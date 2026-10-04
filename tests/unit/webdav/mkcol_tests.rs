//! Wire tests for `WebDavClient::mkcol` (RFC 4918 §9.1) and the RFC 5689
//! extended-MKCOL body.
//!
//! Status-handling contract: `mkcol` returns the raw response for **every**
//! status — non-success statuses (405, 409, 507, …) and 207 multi-status
//! bodies are passed through unchanged and the caller decides (same raw-verb
//! contract as `delete`/`put`). RFC 4918 §9.1 defines `201 Created` as the
//! MKCOL success status and no 207 for MKCOL; servers that answer 207 with
//! error propstats remain fully visible to the caller (nothing is swallowed
//! or misread as success), so the client applies no extra mapping.

use fast_dav_rs::{RequestCompressionMode, WebDavClient};

/// Response head with a custom status line and `Content-Length`.
fn dav_head(status_line: &str, body_len: usize) -> String {
    format!("HTTP/1.1 {status_line}\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n")
}

/// Split a captured request into (head, body) at the header terminator.
fn request_parts(captured: &[u8]) -> (String, String) {
    let req = String::from_utf8_lossy(captured).into_owned();
    match req.split_once("\r\n\r\n") {
        Some((head, body)) => (head.to_string(), body.to_owned()),
        None => (req, String::new()),
    }
}

#[tokio::test]
async fn mkcol_without_body_sends_mkcol_with_empty_body() {
    let (base, captured) =
        crate::common::http_helpers::serve_capture(dav_head("201 Created", 0), Vec::new()).await;

    let client = WebDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client.mkcol("col/", None).await.unwrap();

    assert_eq!(
        resp.status(),
        201,
        "RFC 4918 §9.1: MKCOL answers 201 Created"
    );

    let (head, body) = request_parts(&captured.lock().unwrap());
    assert!(
        head.starts_with("MKCOL /col/ HTTP/1.1"),
        "expected a bare MKCOL request line: {head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("content-type:"),
        "bare MKCOL must not advertise a Content-Type: {head}"
    );
    assert!(
        body.is_empty(),
        "MKCOL without a body must send no request body: {body:?}"
    );
}

#[tokio::test]
async fn mkcol_passes_non_success_status_through_to_caller() {
    // Raw-verb contract: a 405 (RFC 4918 §9.1 — MKCOL on an existing
    // resource) is returned as-is for the caller to inspect via
    // `status()`; the client maps no MKCOL failure status to an error.
    let (base, captured) = crate::common::http_helpers::serve_capture(
        dav_head("405 Method Not Allowed", 0),
        Vec::new(),
    )
    .await;

    let client = WebDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client.mkcol("col/", None).await.unwrap();

    assert_eq!(resp.status(), 405);

    let (head, _) = request_parts(&captured.lock().unwrap());
    assert!(
        head.starts_with("MKCOL /col/ HTTP/1.1"),
        "expected a bare MKCOL request line: {head}"
    );
}

#[tokio::test]
async fn mkcol_extended_body_sets_content_type_and_body() {
    // RFC 5689 extended-MKCOL: the optional XML body must reach the wire
    // verbatim, with `Content-Type: application/xml`.
    let xml = r#"<D:mkcol xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:set><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype></D:prop></D:set></D:mkcol>"#;
    let (base, captured) =
        crate::common::http_helpers::serve_capture(dav_head("201 Created", 0), Vec::new()).await;

    let client = WebDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client.mkcol("cal/", Some(xml)).await.unwrap();

    assert_eq!(resp.status(), 201);

    let (head, body) = request_parts(&captured.lock().unwrap());
    assert!(
        head.starts_with("MKCOL /cal/ HTTP/1.1"),
        "expected an MKCOL request line: {head}"
    );
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: application/xml"),
        "extended MKCOL must carry Content-Type: application/xml (RFC 5689 §2): {head}"
    );
    assert_eq!(body, xml, "extended-MKCOL body must be sent verbatim");
}

#[tokio::test]
async fn mkcol_passes_207_error_propstat_through_to_caller() {
    // Server-dependent shape: RFC 4918 §9.1 defines no 207 for MKCOL, but
    // some servers answer with a multi-status carrying an error propstat.
    // The raw response is handed to the caller untouched — a 207 is never
    // mistaken for success (the caller sees the 207 status and the full
    // error body), so no client-side mapping is applied.
    let body = "<?xml version=\"1.0\"?>\
<D:multistatus xmlns:D=\"DAV:\">\
<D:response><D:href>/col/</D:href><D:propstat><D:prop><D:resourcetype/></D:prop>\
<D:status>HTTP/1.1 423 Locked</D:status></D:propstat></D:response>\
</D:multistatus>";
    let (base, captured) = crate::common::http_helpers::serve_capture(
        dav_head("207 Multi-Status", body.len()),
        body.as_bytes().to_vec(),
    )
    .await;

    let client = WebDavClient::builder(&base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);

    let resp = client.mkcol("col/", None).await.unwrap();

    assert_eq!(resp.status(), 207);
    let resp_body = String::from_utf8_lossy(resp.body());
    assert!(
        resp_body.contains("HTTP/1.1 423 Locked"),
        "the 207 error-propstat body must reach the caller: {resp_body}"
    );

    let (head, _) = request_parts(&captured.lock().unwrap());
    assert!(
        head.starts_with("MKCOL /col/ HTTP/1.1"),
        "expected a bare MKCOL request line: {head}"
    );
}
