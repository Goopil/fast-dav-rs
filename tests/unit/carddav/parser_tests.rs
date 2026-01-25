use fast_dav_rs::carddav::parse_multistatus_bytes;

#[test]
fn parse_multistatus_extracts_addressbook_properties() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/</D:href>
    <D:propstat>
      <D:prop>
        <C:addressbook-home-set>
          <D:href>/dav/user01/</D:href>
        </C:addressbook-home-set>
        <D:resourcetype>
          <D:collection/>
        </D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <D:getetag>"etag-123"</D:getetag>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
        <C:supported-address-data>
          <C:address-data-type content-type="text/vcard" version="4.0"/>
          <C:address-data-type content-type="text/vcard" version="3.0"/>
        </C:supported-address-data>
        <C:address-data><![CDATA[BEGIN:VCARD
VERSION:4.0
END:VCARD
]]></C:address-data>
        <D:sync-token>token-123</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let items = parse_multistatus_bytes(xml.as_bytes())
        .expect("xml parsing succeeds")
        .items;
    assert_eq!(items.len(), 2);

    let collection = &items[0];
    assert!(collection.is_collection);
    assert_eq!(collection.href, "/dav/user01/");
    assert_eq!(collection.addressbook_home_set, vec!["/dav/user01/"]);

    let book = &items[1];
    assert!(book.is_addressbook);
    assert_eq!(book.displayname.as_deref(), Some("Personal"));
    assert_eq!(
        book.supported_address_data,
        vec![
            "text/vcard;version=4.0".to_string(),
            "text/vcard;version=3.0".to_string()
        ]
    );
    assert_eq!(book.etag.as_deref(), Some("\"etag-123\""));
    assert_eq!(book.sync_token.as_deref(), Some("token-123"));
    let data = book.address_data.as_ref().expect("address data present");
    assert!(data.contains("BEGIN:VCARD"));
    assert_eq!(book.href, "/dav/user01/personal/");
}

#[test]
fn parse_multistatus_extracts_common_properties_and_top_level_sync_token() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:sync-token>top-token</D:sync-token>
  <D:response>
    <D:href>/dav/user01/ab/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Contacts</D:displayname>
        <D:getetag>"etag-777"</D:getetag>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
        <D:sync-token>item-token</D:sync-token>
        <D:current-user-principal>
          <D:href>/principals/user01/</D:href>
        </D:current-user-principal>
        <D:owner>
          <D:href>/principals/user01/</D:href>
        </D:owner>
        <D:getcontenttype>text/vcard</D:getcontenttype>
        <D:getlastmodified>Mon, 01 Jan 2024 00:00:00 GMT</D:getlastmodified>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("xml parsing succeeds");
    assert_eq!(result.sync_token.as_deref(), Some("top-token"));
    assert_eq!(result.items.len(), 1);

    let item = &result.items[0];
    assert_eq!(item.href, "/dav/user01/ab/");
    assert_eq!(item.status.as_deref(), Some("HTTP/1.1 200 OK"));
    assert_eq!(item.displayname.as_deref(), Some("Contacts"));
    assert_eq!(item.etag.as_deref(), Some("\"etag-777\""));
    assert!(item.is_collection);
    assert!(item.is_addressbook);
    assert_eq!(item.sync_token.as_deref(), Some("item-token"));
    assert_eq!(item.current_user_principal, vec!["/principals/user01/"]);
    assert_eq!(item.owner.as_deref(), Some("/principals/user01/"));
    assert_eq!(item.content_type.as_deref(), Some("text/vcard"));
    assert_eq!(
        item.last_modified.as_deref(),
        Some("Mon, 01 Jan 2024 00:00:00 GMT")
    );
}

#[test]
fn parse_multistatus_preserves_multiline_address_data() {
    let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/ab/</D:href>
    <D:propstat>
      <D:prop>
        <C:address-data><![CDATA[BEGIN:VCARD
]]><![CDATA[END:VCARD
]]></C:address-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let result = parse_multistatus_bytes(xml.as_bytes()).expect("xml parsing succeeds");
    assert_eq!(result.items.len(), 1);

    let item = &result.items[0];
    let data = item.address_data.as_ref().expect("address data present");
    assert_eq!(data, "BEGIN:VCARD\nEND:VCARD\n");
}
