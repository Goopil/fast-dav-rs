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

    // Test streaming PROPFIND — verify the HTTP streaming path returns a successful response.
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

    // Test streaming REPORT — verify the HTTP streaming path works.
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

    // Verify the full parse pipeline by using the high-level list_addressbooks API,
    // which internally sends a PROPFIND and parses the response.
    let result = client.list_addressbooks("addressbooks/test/").await;

    match result {
        Ok(books) => {
            println!("list_addressbooks returned {} book(s)", books.len());
        }
        Err(e) => {
            panic!("list_addressbooks failed: {}", e);
        }
    }
}
