use anyhow::{Result, anyhow};
use bytes::Bytes;
use fast_dav_rs::carddav::streaming::*;
use http_body_util::Full;
use hyper::Request;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

async fn parse_streaming_xml(xml: &str) -> Result<ParseResult<Vec<fast_dav_rs::carddav::DavItem>>> {
    let (client_io, mut server_io) = io::duplex(16 * 1024);
    let body = xml.as_bytes().to_vec();
    let header = format!(
        "HTTP/1.1 207 Multi-Status\r\nContent-Length: {}\r\nContent-Type: application/xml; charset=utf-8\r\nConnection: close\r\n\r\n",
        body.len()
    );

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        let mut seen = Vec::new();
        loop {
            let n = server_io.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
            if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if seen.len() > 8192 {
                break;
            }
        }

        server_io.write_all(header.as_bytes()).await?;
        let split = body.len() / 2;
        server_io.write_all(&body[..split]).await?;
        server_io.write_all(&body[split..]).await?;
        server_io.shutdown().await?;
        Ok::<(), std::io::Error>(())
    });

    let (mut sender, conn) = http1::handshake(TokioIo::new(client_io)).await?;
    let conn_task = tokio::spawn(conn);

    let req = Request::builder()
        .method("GET")
        .uri("http://localhost/")
        .body(Full::<Bytes>::default())?;

    let resp = sender.send_request(req).await?;
    let encodings = fast_dav_rs::detect_encodings(resp.headers());
    let parsed = parse_multistatus_stream(resp.into_body(), &encodings).await?;

    server_task.await??;
    conn_task.await??;

    Ok(parsed)
}

#[test]
fn test_element_from_bytes() {
    // Test basic elements
    assert_eq!(element_from_bytes(b"multistatus"), ElementName::Multistatus);
    assert_eq!(element_from_bytes(b"response"), ElementName::Response);
    assert_eq!(element_from_bytes(b"propstat"), ElementName::Propstat);
    assert_eq!(element_from_bytes(b"href"), ElementName::Href);
    assert_eq!(element_from_bytes(b"displayname"), ElementName::Displayname);
    assert_eq!(element_from_bytes(b"getetag"), ElementName::Getetag);

    // Test namespaced elements
    assert_eq!(element_from_bytes(b"D:href"), ElementName::Href);
    assert_eq!(
        element_from_bytes(b"C:address-data"),
        ElementName::AddressData
    );

    // Test unknown elements
    assert_eq!(element_from_bytes(b"unknown-element"), ElementName::Other);
}

#[test]
fn test_decode_text() {
    // Test normal text
    assert_eq!(decode_text(b"hello").unwrap(), "hello");

    // Test escaped text
    assert_eq!(decode_text(b"hello &amp; world").unwrap(), "hello & world");
    assert_eq!(decode_text(b"test &lt;tag&gt;").unwrap(), "test <tag>");

    // Test invalid UTF-8 handling
    assert!(decode_text(b"\xFF\xFE").is_ok()); // Should handle gracefully
}

#[test]
fn test_multistatus_visit_matches_vec() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/cal1/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Addressbook One</D:displayname>
        <D:getetag>"etag-1"</D:getetag>
      </D:prop>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal2/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Addressbook Two</D:displayname>
        <D:getetag>"etag-2"</D:getetag>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

    let items = parse_multistatus_bytes(xml.as_bytes())
        .expect("parse bytes")
        .items;

    let mut visited = Vec::new();
    parse_multistatus_bytes_visit(xml.as_bytes(), |item| {
        visited.push(item);
        Ok(())
    })
    .expect("visit parse");

    assert_eq!(items.len(), visited.len());
    for (lhs, rhs) in items.iter().zip(&visited) {
        assert_eq!(lhs.href, rhs.href);
        assert_eq!(lhs.displayname, rhs.displayname);
        assert_eq!(lhs.etag, rhs.etag);
    }
}

#[test]
fn test_multistatus_visit_error_propagates() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/err/</D:href>
  </D:response>
</D:multistatus>
"#;

    let err = parse_multistatus_bytes_visit(xml.as_bytes(), |_item| Err(anyhow!("boom")));

    assert!(err.is_err(), "expected visitor error to propagate");
}

#[tokio::test]
async fn test_streaming_preserves_multiline_address_data() -> Result<()> {
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

    let result = parse_streaming_xml(xml).await?;
    assert_eq!(result.items.len(), 1);

    let item = &result.items[0];
    let data = item.address_data.as_ref().expect("address data present");
    assert_eq!(data, "BEGIN:VCARD\nEND:VCARD\n");
    Ok(())
}
