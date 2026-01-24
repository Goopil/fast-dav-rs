use crate::util::{unique_addressbook_name, unique_contact_uri, unique_uid};
use bytes::Bytes;
use fast_dav_rs::CardDavClient;

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_client() -> CardDavClient {
    CardDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CardDAV client")
}

fn build_vcard(uid: &str, full_name: &str, email: &str) -> Bytes {
    Bytes::from(format!(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:{}\r\nFN:{}\r\nEMAIL:{}\r\nEND:VCARD\r\n",
        uid, full_name, email
    ))
}

#[tokio::test]
async fn test_contact_crud() {
    let client = create_test_client();
    let book_name = unique_addressbook_name("contacts");
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

    let contact_uri = unique_contact_uri("contact");
    let contact_path = format!("{}{}", book_path, contact_uri);
    let uid = unique_uid("contact");

    let vcard = build_vcard(&uid, "Alice Example", "alice@example.com");
    let create_resp = client
        .put_if_none_match(&contact_path, vcard)
        .await
        .expect("PUT contact");
    assert!(
        create_resp.status().is_success(),
        "Expected successful contact creation"
    );

    let query_results = client
        .addressbook_query_uid(&book_path, &uid, true)
        .await
        .expect("query by UID");
    assert!(!query_results.is_empty(), "Expected query results");

    let head_resp = client.head(&contact_path).await.expect("HEAD contact");
    let etag = CardDavClient::etag_from_headers(head_resp.headers())
        .expect("Expected ETag for contact");

    let updated = build_vcard(&uid, "Alice Updated", "alice@example.com");
    let update_resp = client
        .put_if_match(&contact_path, updated, &etag)
        .await
        .expect("PUT update contact");
    assert!(
        update_resp.status().is_success(),
        "Expected successful contact update"
    );

    let head_resp = client.head(&contact_path).await.expect("HEAD updated contact");
    let delete_etag = CardDavClient::etag_from_headers(head_resp.headers())
        .expect("Expected ETag for contact");

    let delete_resp = client
        .delete_if_match(&contact_path, &delete_etag)
        .await
        .expect("DELETE contact");
    assert!(
        delete_resp.status().is_success(),
        "Expected successful contact deletion"
    );

    let _ = client.delete(&book_path).await;
}
