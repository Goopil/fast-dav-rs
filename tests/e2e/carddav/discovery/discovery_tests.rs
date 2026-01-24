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
async fn test_propfind_principals() {
    let client = create_test_client();

    let response = client
        .propfind(
            "principals/",
            Depth::One,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match response {
        Ok(resp) => {
            println!("PROPFIND on principals succeeded with status: {}", resp.status());
            assert!(
                resp.status().is_success(),
                "Expected successful PROPFIND on principals"
            );
            let body = resp.into_body();
            assert!(!body.is_empty(), "Expected non-empty response body");
        }
        Err(e) => {
            panic!("PROPFIND on principals failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_propfind_user_principal() {
    let client = create_test_client();

    let response = client
        .propfind(
            "principals/test/",
            Depth::Zero,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match response {
        Ok(resp) => {
            println!(
                "PROPFIND on user principal succeeded with status: {}",
                resp.status()
            );
            assert!(
                resp.status().is_success(),
                "Expected successful PROPFIND on user principal"
            );
            let body = resp.into_body();
            assert!(!body.is_empty(), "Expected non-empty response body");
        }
        Err(e) => {
            panic!("PROPFIND on user principal failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_propfind_user_addressbooks() {
    let client = create_test_client();

    let response = client
        .propfind(
            "addressbooks/test/",
            Depth::One,
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
    <C:addressbook-description/>
  </D:prop>
</D:propfind>"#,
        )
        .await;

    match response {
        Ok(resp) => {
            println!(
                "PROPFIND on user addressbooks succeeded with status: {}",
                resp.status()
            );
            assert!(
                resp.status().is_success(),
                "Expected successful PROPFIND on user addressbooks"
            );
            let body = resp.into_body();
            assert!(!body.is_empty(), "Expected non-empty response body");
        }
        Err(e) => {
            panic!("PROPFIND on user addressbooks failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_discovery_operations() {
    let client = create_test_client();

    let principal_result = client.discover_current_user_principal().await;
    match principal_result {
        Ok(Some(principal)) => {
            println!("Discovered user principal: {}", principal);
            assert!(!principal.is_empty(), "Expected non-empty principal URL");
        }
        Ok(None) => {
            panic!("No user principal found");
        }
        Err(e) => {
            panic!("Failed to discover user principal: {}", e);
        }
    }

    let home_sets_result = client
        .discover_addressbook_home_set("principals/test/")
        .await;
    match home_sets_result {
        Ok(home_sets) => {
            println!("Discovered {} addressbook home sets", home_sets.len());
            assert!(
                !home_sets.is_empty(),
                "Expected at least one addressbook home set"
            );
            for home_set in home_sets {
                assert!(!home_set.is_empty(), "Expected non-empty home set URL");
            }
        }
        Err(e) => {
            panic!("Failed to discover addressbook home sets: {}", e);
        }
    }

    let books_result = client.list_addressbooks("addressbooks/test/").await;
    match books_result {
        Ok(books) => {
            println!("Found {} addressbooks", books.len());
            assert!(!books.is_empty(), "Expected at least one addressbook");
            for book in books {
                assert!(
                    book.displayname.is_some(),
                    "Expected addressbook to have a display name"
                );
            }
        }
        Err(e) => {
            panic!("Failed to list addressbooks: {}", e);
        }
    }
}
