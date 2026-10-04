use fast_dav_rs::{Depth, Error, RequestCompressionMode, WebDavClient};
use std::sync::{Arc, Mutex};

use crate::common::http_helpers::{response_head, serve_capture};

fn make_client(base: &str) -> WebDavClient {
    let client = WebDavClient::builder(base).build().unwrap();
    client.set_request_compression_mode(RequestCompressionMode::Disabled);
    client
}

/// The captured wire request, lowercased for header-name matching (hyper
/// writes HTTP/1.1 header names in lowercase).
fn captured_request(captured: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&captured.lock().unwrap()).to_ascii_lowercase()
}

#[tokio::test]
async fn copy_with_depth_zero_sends_depth_destination_and_overwrite() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = make_client(&base);
    let dest = format!("{base}dest/");

    client
        .copy_with_depth("src/", &dest, true, Depth::Zero)
        .await
        .unwrap();

    let req = captured_request(&captured);
    assert!(
        req.starts_with("copy /src/ http/1.1"),
        "expected a COPY request: {req}"
    );
    assert!(
        req.contains("depth: 0"),
        "shallow copy must send 'Depth: 0' (RFC 4918 §9.8.3): {req}"
    );
    assert!(
        req.contains(&format!("destination: {dest}")),
        "the Destination header must carry the absolute URL: {req}"
    );
    assert!(
        req.contains("overwrite: t"),
        "overwrite=true must send 'Overwrite: T': {req}"
    );
}

#[tokio::test]
async fn copy_with_depth_sends_other_depth_values_verbatim() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = make_client(&base);
    let dest = format!("{base}dest/");

    client
        .copy_with_depth("src/", &dest, false, Depth::Infinity)
        .await
        .unwrap();

    let req = captured_request(&captured);
    assert!(
        req.contains("depth: infinity"),
        "the Depth header must be sent verbatim: {req}"
    );
    assert!(
        req.contains("overwrite: f"),
        "overwrite=false must send 'Overwrite: F': {req}"
    );
}

#[tokio::test]
async fn move_with_depth_accepts_depth_zero() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = make_client(&base);
    let dest = format!("{base}dest/");

    client
        .move_with_depth("src/", &dest, true, Depth::Zero)
        .await
        .unwrap();

    let req = captured_request(&captured);
    assert!(
        req.starts_with("move /src/ http/1.1"),
        "expected a MOVE request: {req}"
    );
    assert!(
        req.contains("depth: 0"),
        "a non-collection move must send 'Depth: 0' (RFC 4918 §9.9.3): {req}"
    );
    assert!(req.contains("destination:"), "Destination required: {req}");
    assert!(req.contains("overwrite: t"), "Overwrite required: {req}");
}

#[tokio::test]
async fn move_with_depth_rejects_depth_one_and_infinity_before_any_io() {
    for depth in [Depth::One, Depth::Infinity] {
        let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
        let client = make_client(&base);
        let dest = format!("{base}dest/");

        let err = client.move_with_depth("src/", &dest, true, depth).await;

        match &err {
            Err(Error::InvalidInput(message)) => {
                assert!(
                    message.contains("Depth"),
                    "the rejection must explain the Depth restriction: {message}"
                );
            }
            other => panic!("expected InvalidInput for {depth:?}, got: {other:?}"),
        }
        assert!(
            captured.lock().unwrap().is_empty(),
            "the Depth check must happen before any network I/O"
        );
    }
}

#[tokio::test]
async fn plain_copy_sends_no_depth_header() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = make_client(&base);
    let dest = format!("{base}dest/");

    client.copy("src/", &dest, true).await.unwrap();

    let req = captured_request(&captured);
    assert!(
        req.starts_with("copy /src/ http/1.1"),
        "expected a COPY request: {req}"
    );
    assert!(
        !req.contains("depth:"),
        "the existing copy must not send a Depth header: {req}"
    );
    assert!(
        req.contains("destination:") && req.contains("overwrite: t"),
        "existing copy behavior is unchanged: {req}"
    );
}

#[tokio::test]
async fn plain_move_sends_no_depth_header() {
    let (base, captured) = serve_capture(response_head("", 0), Vec::new()).await;
    let client = make_client(&base);
    let dest = format!("{base}dest/");

    client.r#move("src/", &dest, true).await.unwrap();

    let req = captured_request(&captured);
    assert!(
        req.starts_with("move /src/ http/1.1"),
        "expected a MOVE request: {req}"
    );
    assert!(
        !req.contains("depth:"),
        "the existing move must not send a Depth header: {req}"
    );
    assert!(
        req.contains("destination:") && req.contains("overwrite: t"),
        "existing move behavior is unchanged: {req}"
    );
}
