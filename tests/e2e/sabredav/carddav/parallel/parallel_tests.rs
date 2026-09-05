//! CardDAV batch-API e2e (#117, AUDIT-017 concurrency parity): the
//! semaphore-bound `propfind_many` / `report_many` delegates exercised
//! against addressbook collections.

use crate::util::{unique_addressbook_name, unique_contact_uri, unique_uid};
use bytes::Bytes;
use fast_dav_rs::{CardDavClient, Depth};
use std::sync::Arc;

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

fn propfind_body() -> Arc<Bytes> {
    Arc::new(Bytes::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop><D:displayname/><D:resourcetype/></D:prop>
</D:propfind>"#,
    ))
}

#[tokio::test]
async fn test_propfind_many_addressbooks() {
    let client = create_test_client();
    let paths: Vec<String> = vec![
        format!("addressbooks/{TEST_USER}/"),
        format!("principals/{TEST_USER}/"),
        format!("addressbooks/{TEST_USER}/"),
        format!("principals/{TEST_USER}/"),
    ];

    let results = client
        .propfind_many(paths.clone(), Depth::Zero, propfind_body(), 3)
        .await;

    assert_eq!(
        results.len(),
        paths.len(),
        "one BatchItem per requested path"
    );
    for result in results {
        let resp = result
            .result
            .unwrap_or_else(|e| panic!("propfind_many({}) failed: {e}", result.pub_path));
        assert!(
            resp.status().is_success(),
            "propfind_many({}) returned {}",
            result.pub_path,
            resp.status()
        );
    }
}

#[tokio::test]
async fn test_report_many_addressbooks() {
    let client = create_test_client();
    let (_name, book_path) = create_addressbook(&client, "parallel_report").await;

    // Seed two contacts so the REPORT has data to return.
    let mut contact_paths = Vec::new();
    for i in 1..=2 {
        let uid = unique_uid(&format!("rep-{i}"));
        let contact_uri = unique_contact_uri("rep");
        let contact_path = format!("{book_path}{contact_uri}");
        let resp = client
            .put(
                &contact_path,
                build_vcard(&uid, &format!("Report Many {i}")),
            )
            .await
            .expect("contact PUT request");
        assert!(resp.status().is_success(), "expected contact creation");
        contact_paths.push(contact_path);
    }

    let report_body = Arc::new(Bytes::from(format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<C:addressbook-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop><D:getetag/><C:address-data/></D:prop>
  <D:href>/{}</D:href>
</C:addressbook-multiget>"#,
        contact_paths[0]
    )));

    let results = client
        .report_many(vec![book_path.clone()], Depth::One, report_body, 2)
        .await;

    assert_eq!(results.len(), 1, "one BatchItem per requested path");
    for result in results {
        let resp = result
            .result
            .unwrap_or_else(|e| panic!("report_many({}) failed: {e}", result.pub_path));
        assert!(
            resp.status().is_success(),
            "report_many({}) returned {}",
            result.pub_path,
            resp.status()
        );
        let body = String::from_utf8_lossy(resp.body());
        assert!(
            body.contains("address-data"),
            "multiget REPORT must return address-data, got: {body}"
        );
    }

    for contact_path in contact_paths {
        let _ = client.delete(&contact_path).await;
    }
    let _ = client.delete(&book_path).await;
}
