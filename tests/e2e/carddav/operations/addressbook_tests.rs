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
async fn test_create_addressbook() {
    let client = create_test_client();
    let book_name = unique_addressbook_name("test_book");
    let book_path = format!("addressbooks/test/{}/", book_name);

    let book_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkaddressbook xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:addressbook-description>Test addressbook created by fast-dav-rs tests</C:addressbook-description>
    </D:prop>
  </D:set>
</C:mkaddressbook>"#,
        book_name
    );

    let response = client.mkaddressbook(&book_path, &book_xml).await;
    match response {
        Ok(resp) => {
            println!(
                "MKADDRESSBOOK request succeeded with status: {}",
                resp.status()
            );
            assert!(
                resp.status().is_success(),
                "Expected successful addressbook creation, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("MKADDRESSBOOK request failed: {}", e);
        }
    }

    let verify_response = client
        .propfind(
            &book_path,
            Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match verify_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected to find created addressbook, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to verify addressbook creation: {}", e);
        }
    }

    let delete_response = client.delete(&book_path).await;
    match delete_response {
        Ok(resp) => {
            println!("Cleaned up addressbook with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful addressbook deletion, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to clean up addressbook: {}", e);
        }
    }
}

#[tokio::test]
async fn test_proppatch_operation() {
    let client = create_test_client();
    let book_name = unique_addressbook_name("proppatch_book");
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
    if let Err(e) = mk_response {
        panic!("Failed to create addressbook: {}", e);
    }
    assert!(
        mk_response.unwrap().status().is_success(),
        "Expected successful addressbook creation"
    );

    let proppatch_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propertyupdate xmlns:D="DAV:">
  <D:set>
    <D:prop>
      <D:displayname>Updated Test Addressbook</D:displayname>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;

    let proppatch_response = client.proppatch(&book_path, proppatch_xml).await;
    match proppatch_response {
        Ok(resp) => {
            println!("PROPPATCH request succeeded with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful PROPPATCH operation, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("PROPPATCH request failed: {}", e);
        }
    }

    let verify_response = client
        .propfind(
            &book_path,
            Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match verify_response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Expected successful verification PROPFIND, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to verify PROPPATCH update: {}", e);
        }
    }

    let cleanup_response = client.delete(&book_path).await;
    match cleanup_response {
        Ok(resp) => {
            println!("Cleaned up addressbook with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful addressbook deletion, got status: {}",
                resp.status()
            );
        }
        Err(e) => {
            panic!("Failed to clean up addressbook: {}", e);
        }
    }
}
