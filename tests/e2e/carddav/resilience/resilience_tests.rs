//! CardDAV resilience e2e (#117): 404s on missing addressbooks/contacts,
//! typed errors, and multiget behavior for partially missing hrefs.

use crate::util::{unique_addressbook_name, unique_contact_uri, unique_uid};
use bytes::Bytes;
use fast_dav_rs::{CardDavClient, Error};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CardDavClient {
    CardDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CardDAV client")
}

fn build_vcard(uid: &str, full_name: &str) -> Bytes {
    Bytes::from(format!(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:{uid}\r\nFN:{full_name}\r\nEND:VCARD\r\n"
    ))
}

async fn create_addressbook(client: &CardDavClient, prefix: &str) -> (String, String) {
    let book_name = unique_addressbook_name(prefix);
    let book_path = format!("addressbooks/{TEST_USER}/{book_name}/");
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
    let resp = client
        .mkaddressbook(&book_path, &book_xml)
        .await
        .expect("mkaddressbook request");
    assert!(
        resp.status().is_success(),
        "Expected successful addressbook creation, got {}",
        resp.status()
    );
    (book_name, book_path)
}

#[tokio::test]
async fn test_resilience_query_nonexistent_addressbook_typed_404() {
    let client = create_test_client();
    let err = client
        .addressbook_query(
            &format!(
                "addressbooks/{TEST_USER}/no_such_book_{}/",
                unique_uid("miss")
            ),
            "<C:filter/>",
            true,
        )
        .await
        .expect_err("addressbook-query on a missing addressbook must fail");
    match err {
        Error::UnexpectedStatus { status, .. } => {
            assert_eq!(
                status.as_u16(),
                404,
                "expected 404 for a missing addressbook"
            );
        }
        other => panic!("expected UnexpectedStatus, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_resilience_get_nonexistent_contact_404() {
    let client = create_test_client();
    let resp = client
        .get(&format!(
            "addressbooks/{TEST_USER}/no_such_book_{}/missing.vcf",
            unique_uid("get-miss")
        ))
        .await
        .expect("GET on a missing contact is a raw response, not a transport error");
    assert_eq!(
        resp.status().as_u16(),
        404,
        "expected 404 for a missing contact"
    );
}

#[tokio::test]
async fn test_resilience_delete_nonexistent_contact_404() {
    let client = create_test_client();
    let resp = client
        .delete(&format!(
            "addressbooks/{TEST_USER}/no_such_book_{}/missing.vcf",
            unique_uid("del-miss")
        ))
        .await
        .expect("DELETE on a missing contact is a raw response, not a transport error");
    assert_eq!(
        resp.status().as_u16(),
        404,
        "expected 404 for deleting a missing contact"
    );
}

#[tokio::test]
async fn test_resilience_multiget_mixed_existing_and_missing_hrefs() {
    let client = create_test_client();
    let (_name, book_path) = create_addressbook(&client, "resilience_mg").await;

    let uid = unique_uid("mg-hit");
    let contact_uri = unique_contact_uri("mg-hit");
    let contact_path = format!("{book_path}{contact_uri}");
    let resp = client
        .put(&contact_path, build_vcard(&uid, "Multiget Hit"))
        .await
        .expect("contact PUT request");
    assert!(resp.status().is_success(), "expected contact creation");

    let objects = client
        .addressbook_multiget(
            &book_path,
            [
                format!("/{contact_path}"),
                format!(
                    "/{book_path}definitely_missing_{}.vcf",
                    unique_uid("mg-miss")
                ),
            ],
            true,
        )
        .await
        .expect("multiget with a missing href must still return a multistatus");

    // Sabre/DAV omits missing hrefs from the multistatus entirely (allowed —
    // servers may report 404 per-href or omit them); pin the actual contract.
    assert_eq!(
        objects.len(),
        1,
        "only the existing contact must be reported, got: {objects:?}"
    );
    let hit = &objects[0];
    assert!(
        hit.href.contains(&contact_uri),
        "existing contact must be reported, got: {hit:?}"
    );
    assert!(
        hit.address_data
            .as_deref()
            .is_some_and(|d| d.contains(&uid)),
        "existing contact must carry (valid) address data, got: {hit:?}"
    );
    assert!(
        hit.status.as_deref().is_some_and(|s| s.contains("200")),
        "existing contact must carry a 200 status, got: {hit:?}"
    );

    let _ = client.delete(&contact_path).await;
    let _ = client.delete(&book_path).await;
}
