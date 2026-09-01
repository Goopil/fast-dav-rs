//! CardDAV parsing e2e (#117): vCard data round-trips through the server and
//! the multistatus parser, and malformed payloads are rejected by the server
//! instead of being silently accepted.

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
async fn test_parsing_vcard_roundtrip_preserves_fields() {
    let client = create_test_client();
    let (_name, book_path) = create_addressbook(&client, "parsing_roundtrip").await;

    let uid = unique_uid("parse");
    let contact_uri = unique_contact_uri("parse");
    let contact_path = format!("{book_path}{contact_uri}");
    // Multi-line vCard with accented UTF-8 + structured fields.
    let vcard = format!(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:{uid}\r\nFN:Zoé Müller\r\n\
         N:Müller;Zoé;;;\r\nEMAIL:zoe@example.com\r\nTEL:+41 22 000 00 00\r\n\
         NOTE:Line one\\nLine two\r\nEND:VCARD\r\n"
    );
    let put_resp = client
        .put(&contact_path, Bytes::from(vcard.clone()))
        .await
        .expect("vCard PUT request");
    assert!(
        put_resp.status().is_success(),
        "expected vCard storage, got {}",
        put_resp.status()
    );

    // Query by UID (server-side filter) and verify the parsed payload.
    let objects = client
        .addressbook_query_uid(&book_path, &uid, true)
        .await
        .expect("addressbook_query_uid must succeed");
    assert_eq!(objects.len(), 1, "UID filter must match exactly one card");
    let data = objects[0]
        .address_data
        .as_deref()
        .expect("query with include_data must return address-data");
    for expected in ["Zoé Müller", "zoe@example.com", &uid, "Line two"] {
        assert!(
            data.contains(expected),
            "parsed vCard must preserve {expected:?}, got:\n{data}"
        );
    }

    let _ = client.delete(&contact_path).await;
    let _ = client.delete(&book_path).await;
}

#[tokio::test]
async fn test_parsing_malformed_vcard_rejected_by_server() {
    let client = create_test_client();
    let (_name, book_path) = create_addressbook(&client, "parsing_malformed").await;

    // Sabre/DAV's vobject plugin rejects non-vCard payloads with 415.
    let put_resp = client
        .put(
            &format!("{book_path}malformed-{}.vcf", unique_uid("bad")),
            Bytes::from_static(b"this is not a vcard at all"),
        )
        .await
        .expect("PUT of a malformed vCard is a raw response, not a transport error");
    let status = put_resp.status().as_u16();
    assert!(
        status == 415 || status == 400,
        "server must reject a malformed vCard with 4xx (got {status})"
    );

    let _ = client.delete(&book_path).await;
}
