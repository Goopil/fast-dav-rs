use fast_dav_rs::carddav::{
    build_addressbook_multiget_body, build_addressbook_query_body,
    build_addressbook_query_filter_uid, build_sync_collection_body, map_address_objects,
    map_addressbook_list, map_sync_response, parse_multistatus_bytes,
};
use hyper::http::HeaderMap;

#[test]
fn builds_addressbook_query_with_filter() {
    let filter = build_addressbook_query_filter_uid("contact-123");
    let body = build_addressbook_query_body(&filter, true);
    assert!(body.contains("<C:address-data/>"));
    assert!(body.contains("prop-filter name=\"UID\""));
    assert!(body.contains("contact-123"));
}

#[test]
fn builds_addressbook_multiget_and_escapes() {
    let body = build_addressbook_multiget_body(
        vec![
            "/dav/user01/AddressBooks/Personal/contact.vcf",
            "/dav/user01/AddressBooks/Personal/contact&special.vcf",
        ],
        true,
    )
    .expect("hrefs present");
    assert!(body.contains("<C:address-data/>"));
    assert!(body.contains("contact&amp;special.vcf"));
}

#[test]
fn builds_sync_collection_body_with_token_and_limit() {
    let body = build_sync_collection_body(Some("http://token"), Some(50), false);
    assert!(body.contains("<D:sync-token>http://token</D:sync-token>"));
    assert!(body.contains("<D:nresults>50</D:nresults>"));
    assert!(!body.contains("<C:address-data/>"));
}

#[test]
fn maps_carddav_multistatus_structures() {
    let books_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <C:addressbook-description>Friends</C:addressbook-description>
        <C:addressbook-color>#00ff00</C:addressbook-color>
        <D:getetag>"ab-etag"</D:getetag>
        <D:sync-token>http://sabre.io/ns/sync/42</D:sync-token>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
        <C:supported-address-data>
          <C:address-data-type content-type="text/vcard" version="4.0"/>
        </C:supported-address-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let books = map_addressbook_list(parse_multistatus_bytes(books_xml.as_bytes()).unwrap().items);
    assert_eq!(books.len(), 1);
    let book = &books[0];
    assert_eq!(book.href, "/dav/user01/AddressBooks/Personal/");
    assert_eq!(book.displayname.as_deref(), Some("Personal"));
    assert_eq!(
        book.sync_token.as_deref(),
        Some("http://sabre.io/ns/sync/42")
    );
    assert_eq!(
        book.supported_address_data,
        vec!["text/vcard;version=4.0".to_string()]
    );

    let multiget_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/contact.vcf</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"1234-1"</D:getetag>
        <C:address-data><![CDATA[BEGIN:VCARD
VERSION:4.0
UID:contact-1
END:VCARD]]></C:address-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let objects = map_address_objects(
        parse_multistatus_bytes(multiget_xml.as_bytes())
            .unwrap()
            .items,
    );
    assert_eq!(objects.len(), 1);
    let object = &objects[0];
    assert_eq!(object.href, "/dav/user01/AddressBooks/Personal/contact.vcf");
    assert!(
        object
            .address_data
            .as_ref()
            .unwrap()
            .contains("UID:contact-1")
    );

    let sync_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:sync-token>http://sabre.io/ns/sync/43</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/contact.vcf</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"1234-2"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/user01/AddressBooks/Personal/outdated.vcf</D:href>
    <D:propstat>
      <D:prop/>
      <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", "http://sabre.io/ns/sync/43".parse().unwrap());
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://sabre.io/ns/sync/43")
    );
    assert_eq!(sync.items.len(), 2);
    assert!(sync.items.iter().any(|item| item.is_deleted));
}

#[test]
fn test_top_level_sync_token_parsing() {
    // Test Apple CardDAV format where sync-token is at top level of multistatus
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/book/event1.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"abc123"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Verify the sync token was captured at the top level
    assert_eq!(
        parsed.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // The sync response should use the top-level token
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );
    assert_eq!(sync.items.len(), 1);
    assert_eq!(sync.items[0].href, "/addressbooks/user/book/event1.vcf");
    assert_eq!(sync.items[0].etag.as_deref(), Some("\"abc123\""));
}

#[test]
fn test_sync_token_no_changes() {
    // Test response with no changes - only sync token returned (Apple CardDAV behavior)
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<D:multistatus xmlns="DAV:">
    <sync-token>HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==</sync-token>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Verify the sync token was captured
    assert_eq!(
        parsed.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );

    // No items should be present
    assert_eq!(parsed.items.len(), 0);

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // The sync response should have the token but no items
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("HwoQEgwAADqM3oJGTQACAAEYAhgAIhYI54zB8/3Z68KdARC8lqbdi9Sjz7wBKABIAA==")
    );
    assert_eq!(sync.items.len(), 0);
}

#[test]
fn test_sync_token_priority_top_level_over_header() {
    // Test that top-level sync token takes priority over header
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>top-level-token-123</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/event.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", "header-token-456".parse().unwrap());
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Top-level token should win
    assert_eq!(sync.sync_token.as_deref(), Some("top-level-token-123"));
}

#[test]
fn test_sync_token_priority_top_level_over_per_item() {
    // Test that top-level sync token takes priority over per-item tokens
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>top-level-token-abc</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/</D:href>
        <D:propstat>
            <D:prop>
                <D:sync-token>per-item-token-xyz</D:sync-token>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Top-level token should win over per-item token
    assert_eq!(sync.sync_token.as_deref(), Some("top-level-token-abc"));
}

#[test]
fn test_sync_token_fallback_per_item() {
    // Test that per-item sync token is used when no top-level token exists
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/addressbooks/user/</D:href>
        <D:propstat>
            <D:prop>
                <D:sync-token>per-item-token-fallback</D:sync-token>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Per-item token should be used as fallback
    assert_eq!(sync.sync_token.as_deref(), Some("per-item-token-fallback"));
}

#[test]
fn test_sync_token_fallback_header() {
    // Test that header sync token is used when no top-level or per-item tokens exist
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/addressbooks/user/event.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let mut headers = HeaderMap::new();
    headers.insert("Sync-Token", "header-token-fallback".parse().unwrap());
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Header token should be used as last fallback
    assert_eq!(sync.sync_token.as_deref(), Some("header-token-fallback"));
}

#[test]
fn test_sync_with_deleted_items() {
    // Test sync response with deleted items (404 status)
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>sync-token-with-deletes</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/deleted1.vcf</D:href>
        <D:propstat>
            <D:prop/>
            <D:status>HTTP/1.1 404 Not Found</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/deleted2.vcf</D:href>
        <D:status>HTTP/1.1 410 Gone</D:status>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/updated.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"new-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    assert_eq!(sync.sync_token.as_deref(), Some("sync-token-with-deletes"));
    assert_eq!(sync.items.len(), 3);

    // Check that deleted items are marked correctly
    let deleted_count = sync.items.iter().filter(|item| item.is_deleted).count();
    assert_eq!(deleted_count, 2);

    // Check that updated item is not marked as deleted
    let updated = sync
        .items
        .iter()
        .find(|item| item.href.contains("updated.vcf"))
        .unwrap();
    assert!(!updated.is_deleted);
    assert_eq!(updated.etag.as_deref(), Some("\"new-etag\""));
}

#[test]
fn test_sync_with_multiple_changes() {
    // Test sync response with multiple changed items
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>multi-change-token</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/book/</D:href>
        <D:propstat>
            <D:prop>
                <D:resourcetype>
                    <D:collection/>
                </D:resourcetype>
                <D:getetag>"cal-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/book/event1.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag1"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/book/event2.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"etag2"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();
    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    assert_eq!(sync.sync_token.as_deref(), Some("multi-change-token"));
    // Collection item should be filtered out, only event items remain
    assert_eq!(sync.items.len(), 2);
    assert!(sync.items.iter().all(|item| item.href.ends_with(".vcf")));
}

#[test]
fn test_sync_token_without_namespace_prefix() {
    // Test Apple CardDAV format without namespace prefix (default namespace)
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<multistatus xmlns="DAV:">
    <sync-token>token-no-prefix</sync-token>
    <response>
        <href>/addressbooks/user/event.vcf</href>
        <propstat>
            <prop>
                <getetag>"etag-no-prefix"</getetag>
            </prop>
            <status>HTTP/1.1 200 OK</status>
        </propstat>
    </response>
</multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Should parse correctly even without D: prefix
    assert_eq!(parsed.sync_token.as_deref(), Some("token-no-prefix"));
    assert_eq!(parsed.items.len(), 1);

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);
    assert_eq!(sync.sync_token.as_deref(), Some("token-no-prefix"));
    assert_eq!(sync.items[0].etag.as_deref(), Some("\"etag-no-prefix\""));
}

#[test]
fn test_sync_collection_filters_out_collections() {
    // Test that collection items are properly filtered out
    let sync_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:sync-token>filter-test-token</D:sync-token>
    <D:response>
        <D:href>/addressbooks/user/book/</D:href>
        <D:propstat>
            <D:prop>
                <D:resourcetype>
                    <D:collection/>
                </D:resourcetype>
                <D:getetag>"cal-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/addressbooks/user/book/event.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>"event-etag"</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    let headers = HeaderMap::new();
    let parsed = parse_multistatus_bytes(sync_xml.as_bytes()).unwrap();

    // Both items should be parsed
    assert_eq!(parsed.items.len(), 2);

    let sync = map_sync_response(&headers, parsed.items, parsed.sync_token);

    // Collection should be filtered out, only event remains
    assert_eq!(sync.items.len(), 1);
    assert_eq!(sync.items[0].href, "/addressbooks/user/book/event.vcf");
}

#[test]
fn test_apple_namespace_addressbook_color() {
    // Test that Apple's addressbook-color namespace is parsed correctly
    let books_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/addressbooks/user/personal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Personal</D:displayname>
        <addressbook-color xmlns="http://apple.com/ns/ical/">#FF6B35FF</addressbook-color>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let books = map_addressbook_list(parse_multistatus_bytes(books_xml.as_bytes()).unwrap().items);
    assert_eq!(books.len(), 1);
    let book = &books[0];

    // Verify Apple namespace addressbook-color is parsed
    assert_eq!(book.color.as_deref(), Some("#FF6B35FF"));
    assert_eq!(book.displayname.as_deref(), Some("Personal"));
}

#[test]
fn test_carddav_namespace_addressbook_color() {
    // Test standard CardDAV namespace addressbook-color
    let books_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/addressbooks/user/work/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Work</D:displayname>
        <C:addressbook-color>#0066CC</C:addressbook-color>
        <D:resourcetype>
          <D:collection/>
          <C:addressbook/>
        </D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    let books = map_addressbook_list(parse_multistatus_bytes(books_xml.as_bytes()).unwrap().items);
    assert_eq!(books.len(), 1);
    let book = &books[0];

    // Verify standard CardDAV namespace addressbook-color is parsed
    assert_eq!(book.color.as_deref(), Some("#0066CC"));
    assert_eq!(book.displayname.as_deref(), Some("Work"));
}
