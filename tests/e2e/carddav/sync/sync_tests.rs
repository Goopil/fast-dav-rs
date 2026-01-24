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

fn build_vcard(uid: &str, full_name: &str) -> Bytes {
    Bytes::from(format!(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:{}\r\nFN:{}\r\nEND:VCARD\r\n",
        uid, full_name
    ))
}

#[tokio::test]
async fn test_sync_collection_roundtrip() {
    let client = create_test_client();
    let book_name = unique_addressbook_name("sync_book");
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

    let initial = client
        .sync_collection(&book_path, None, Some(50), false)
        .await
        .expect("initial sync");
    let sync_token = initial
        .sync_token
        .clone()
        .expect("Expected sync token from initial sync");

    let contact_uri = unique_contact_uri("sync_contact");
    let contact_path = format!("{}{}", book_path, contact_uri);
    let uid = unique_uid("sync");
    let vcard = build_vcard(&uid, "Sync Example");

    let create_resp = client
        .put_if_none_match(&contact_path, vcard)
        .await
        .expect("PUT contact for sync");
    assert!(
        create_resp.status().is_success(),
        "Expected contact creation"
    );

    let delta = client
        .sync_collection(&book_path, Some(&sync_token), Some(50), true)
        .await
        .expect("delta sync");

    assert!(
        !delta.items.is_empty(),
        "Expected at least one sync item after change"
    );

    if let Some(item) = delta.items.first() {
        if let Some(etag) = item.etag.as_deref() {
            let _ = client.delete_if_match(&item.href, etag).await;
        } else {
            let _ = client.delete(&item.href).await;
        }
    }

    let _ = client.delete(&book_path).await;
}
