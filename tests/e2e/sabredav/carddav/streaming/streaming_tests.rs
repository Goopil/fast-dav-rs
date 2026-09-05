use crate::util::unique_addressbook_name;
use fast_dav_rs::CardDavClient;
use fast_dav_rs::carddav::Depth;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CardDavClient {
    CardDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CardDAV client")
}

#[tokio::test]
async fn test_propfind_stream() {
    let client = create_test_client();

    let response = client
        .propfind_stream(
            "addressbooks/test/",
            Depth::One,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <D:displayname/>
    <D:getetag/>
    <D:resourcetype/>
    <C:addressbook-description/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match response {
        Ok(stream_response) => {
            println!(
                "PROPFIND stream request succeeded with status: {}",
                stream_response.status()
            );
            assert!(
                stream_response.status().is_success(),
                "Expected successful PROPFIND stream"
            );

            let encodings = fast_dav_rs::detect_encodings(stream_response.headers());
            let items = fast_dav_rs::carddav::parse_multistatus_stream(
                stream_response.into_body(),
                &encodings,
            )
            .await;

            match items {
                Ok(parsed_result) => {
                    println!("Parsed {} items from stream", parsed_result.items.len());
                }
                Err(e) => {
                    panic!("Failed to parse streamed response: {}", e);
                }
            }
        }
        Err(e) => {
            panic!("PROPFIND stream request failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_report_stream() {
    let client = create_test_client();
    let book_name = unique_addressbook_name("stream_book");
    let book_path = format!("addressbooks/test/{}/", book_name);

    let book_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkaddressbook xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkaddressbook>"#,
        book_name
    );

    let mk_response = client.mkaddressbook(&book_path, &book_xml).await;
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful addressbook creation"
    );

    let response = client
        .report_stream(
            &book_path,
            Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:sync-collection xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:sync-token/>
  <D:sync-level>1</D:sync-level>
  <D:prop>
    <D:getetag/>
  </D:prop>
</D:sync-collection>"#,
        )
        .await;

    match response {
        Ok(stream_response) => {
            println!(
                "REPORT stream request succeeded with status: {}",
                stream_response.status()
            );
            if stream_response.status().is_success() {
                let encodings = fast_dav_rs::detect_encodings(stream_response.headers());
                let items = fast_dav_rs::carddav::parse_multistatus_stream(
                    stream_response.into_body(),
                    &encodings,
                )
                .await;

                if let Ok(parsed_result) = items {
                    println!(
                        "Parsed {} items from report stream",
                        parsed_result.items.len()
                    );
                }
            }
        }
        Err(e) => {
            println!("REPORT stream request failed (may be expected): {}", e);
        }
    }

    let _ = client.delete(&book_path).await;
}

#[tokio::test]
async fn test_streaming_parser() {
    let client = create_test_client();

    let response = client
        .propfind(
            "addressbooks/test/",
            Depth::One,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match response {
        Ok(regular_response) => {
            assert!(
                regular_response.status().is_success(),
                "Expected successful PROPFIND"
            );

            let body_bytes = regular_response.into_body();
            let items = fast_dav_rs::carddav::parse_multistatus_bytes(&body_bytes);

            match items {
                Ok(parsed_result) => {
                    println!(
                        "Streaming parser parsed {} items from regular response",
                        parsed_result.items.len()
                    );
                }
                Err(e) => {
                    panic!(
                        "Failed to parse regular response with streaming parser: {}",
                        e
                    );
                }
            }
        }
        Err(e) => {
            panic!("PROPFIND request failed: {}", e);
        }
    }
}
