use crate::caldav::types::DavItem;
#[cfg(test)]
use crate::common::compression::ContentEncoding;
use crate::webdav::streaming::{CommonParser, path_ends_with};
use anyhow::{Result, anyhow};
#[cfg(test)]
use futures::TryStreamExt;
#[cfg(test)]
use http_body_util::BodyStream;
#[cfg(test)]
use hyper::body::Incoming;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Decoder, Reader, XmlVersion};
use std::io::{BufRead, Cursor};
#[cfg(test)]
use tokio::io::AsyncBufRead;
#[cfg(test)]
use tokio::io::BufReader;
#[cfg(test)]
use tokio_util::io::StreamReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementName {
    Multistatus,
    Response,
    Propstat,
    Prop,
    Href,
    Status,
    Displayname,
    Getetag,
    Resourcetype,
    Collection,
    Calendar,
    SupportedCalendarComponentSet,
    Comp,
    CalendarData,
    CalendarDescription,
    CalendarTimezone,
    CalendarColor,
    SyncToken,
    CalendarHomeSet,
    CurrentUserPrincipal,
    Owner,
    Getcontenttype,
    Getlastmodified,
    Other,
}

pub fn element_from_bytes(raw: &[u8]) -> ElementName {
    let local = match raw.iter().position(|b| *b == b':') {
        Some(idx) => &raw[idx + 1..],
        None => raw,
    };

    if local.eq_ignore_ascii_case(b"multistatus") {
        ElementName::Multistatus
    } else if local.eq_ignore_ascii_case(b"response") {
        ElementName::Response
    } else if local.eq_ignore_ascii_case(b"propstat") {
        ElementName::Propstat
    } else if local.eq_ignore_ascii_case(b"prop") {
        ElementName::Prop
    } else if local.eq_ignore_ascii_case(b"href") {
        ElementName::Href
    } else if local.eq_ignore_ascii_case(b"status") {
        ElementName::Status
    } else if local.eq_ignore_ascii_case(b"displayname") {
        ElementName::Displayname
    } else if local.eq_ignore_ascii_case(b"getetag") {
        ElementName::Getetag
    } else if local.eq_ignore_ascii_case(b"resourcetype") {
        ElementName::Resourcetype
    } else if local.eq_ignore_ascii_case(b"collection") {
        ElementName::Collection
    } else if local.eq_ignore_ascii_case(b"calendar") {
        ElementName::Calendar
    } else if local.eq_ignore_ascii_case(b"supported-calendar-component-set") {
        ElementName::SupportedCalendarComponentSet
    } else if local.eq_ignore_ascii_case(b"comp") {
        ElementName::Comp
    } else if local.eq_ignore_ascii_case(b"calendar-data") {
        ElementName::CalendarData
    } else if local.eq_ignore_ascii_case(b"calendar-description") {
        ElementName::CalendarDescription
    } else if local.eq_ignore_ascii_case(b"calendar-timezone") {
        ElementName::CalendarTimezone
    } else if local.eq_ignore_ascii_case(b"calendar-color") {
        ElementName::CalendarColor
    } else if local.eq_ignore_ascii_case(b"sync-token") {
        ElementName::SyncToken
    } else if local.eq_ignore_ascii_case(b"calendar-home-set") {
        ElementName::CalendarHomeSet
    } else if local.eq_ignore_ascii_case(b"current-user-principal") {
        ElementName::CurrentUserPrincipal
    } else if local.eq_ignore_ascii_case(b"owner") {
        ElementName::Owner
    } else if local.eq_ignore_ascii_case(b"getcontenttype") {
        ElementName::Getcontenttype
    } else if local.eq_ignore_ascii_case(b"getlastmodified") {
        ElementName::Getlastmodified
    } else {
        ElementName::Other
    }
}

pub(crate) trait ItemConsumer {
    fn consume(&mut self, item: DavItem) -> Result<()>;
}

impl ItemConsumer for Vec<DavItem> {
    fn consume(&mut self, item: DavItem) -> Result<()> {
        self.push(item);
        Ok(())
    }
}

impl<F> ItemConsumer for F
where
    F: FnMut(DavItem) -> Result<()>,
{
    fn consume(&mut self, item: DavItem) -> Result<()> {
        (self)(item)
    }
}

/// Result of parsing a multistatus response, including top-level sync-token if present
#[derive(Debug)]
pub struct ParseResult<C> {
    pub items: C,
    pub sync_token: Option<String>,
}

pub(crate) struct MultistatusParser<C> {
    pub stack: Vec<ElementName>,
    pub current: DavItem,
    pub sync_token: Option<String>,
    common: CommonParser,
    sink: C,
}

impl<C: ItemConsumer> MultistatusParser<C> {
    pub fn new(sink: C) -> Self {
        Self {
            stack: Vec::with_capacity(16),
            current: DavItem::new(),
            sync_token: None,
            common: CommonParser::new(),
            sink,
        }
    }

    fn finish(self) -> ParseResult<C> {
        ParseResult {
            items: self.sink,
            sync_token: self.sync_token,
        }
    }

    pub fn path_ends_with(&self, needle: &[ElementName]) -> bool {
        path_ends_with(&self.stack, needle)
    }

    fn on_start(&mut self, event: &BytesStart<'_>, decoder: Decoder) -> Result<()> {
        self.common.on_start(event.name().as_ref());
        let element = element_from_bytes(event.name().as_ref());
        self.stack.push(element);

        match element {
            ElementName::Response => {
                self.current = DavItem::new();
            }
            ElementName::Calendar
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::Resourcetype,
                    ElementName::Calendar,
                ]) =>
            {
                self.current.is_calendar = true;
            }
            ElementName::Comp
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::SupportedCalendarComponentSet,
                    ElementName::Comp,
                ]) =>
            {
                for attr in event.attributes().with_checks(false) {
                    let attr = attr?;
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                    if key == "name" {
                        let value = attr
                            .decoded_and_normalized_value(XmlVersion::default(), decoder)
                            .map_err(|e| anyhow!("Invalid XML attribute: {e}"))?
                            .into_owned();
                        if !value.is_empty()
                            && !self
                                .current
                                .supported_components
                                .iter()
                                .any(|c| c.eq_ignore_ascii_case(&value))
                        {
                            self.current.supported_components.push(value);
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn on_end(&mut self, name: &[u8]) -> Result<()> {
        self.common.on_end(name);
        let element = element_from_bytes(name);
        if let Some(popped) = self.stack.pop()
            && popped == ElementName::Response
        {
            let common = self.common.finish_response();
            self.current.apply_common(common);
            let finished = std::mem::take(&mut self.current);
            self.sink.consume(finished)?;
            // Ignore mismatches silently; the XML is assumed well-formed.
        }
        if element == ElementName::Response && !self.stack.is_empty() {
            // Make sure we consume any residual state if nested (should not happen).
            while let Some(last) = self.stack.last() {
                if *last == ElementName::Response {
                    self.stack.pop();
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    fn on_text(&mut self, text: String) {
        self.handle_text(text);
    }

    fn on_cdata(&mut self, text: String) {
        self.handle_text(text);
    }

    fn handle_text(&mut self, text: String) {
        if text.is_empty() {
            return;
        }

        self.common.on_text(&text);

        // calendar-data is often multi-line and may arrive in chunks; keep exact payload.
        if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarData,
        ]) {
            if let Some(existing) = self.current.calendar_data.as_mut() {
                existing.push_str(&text);
            } else {
                self.current.calendar_data = Some(text);
            }
            return;
        }

        // calendar-timezone can also contain multi-line iCalendar content; preserve it.
        if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarTimezone,
        ]) {
            if let Some(existing) = self.current.calendar_timezone.as_mut() {
                existing.push_str(&text);
            } else {
                self.current.calendar_timezone = Some(text.clone());
            }
            return;
        }

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarDescription,
        ]) {
            self.current.calendar_description = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarColor,
        ]) {
            self.current.calendar_color = Some(trimmed.to_string());
        } else if self.path_ends_with(&[ElementName::Multistatus, ElementName::SyncToken]) {
            // Top-level sync-token in sync-collection responses (RFC 6578)
            self.sync_token = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarHomeSet,
            ElementName::Href,
        ]) {
            self.current.calendar_home_set.push(trimmed.to_string());
        }
    }
}

#[cfg(test)]
async fn parse_multistatus_stream_with<C>(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
    sink: C,
) -> Result<ParseResult<C>>
where
    C: ItemConsumer + Send,
{
    use async_compression::tokio::bufread::{BrotliDecoder, GzipDecoder, ZstdDecoder};

    let stream = BodyStream::new(resp_body)
        .map_ok(|frame| frame.into_data().unwrap_or_default())
        .map_err(std::io::Error::other);
    let mut reader: Box<dyn AsyncBufRead + Unpin + Send> =
        Box::new(BufReader::new(StreamReader::new(stream)));
    for encoding in encodings.iter().rev() {
        reader = match encoding {
            ContentEncoding::Identity => reader,
            ContentEncoding::Br => Box::new(BufReader::new(BrotliDecoder::new(reader))),
            ContentEncoding::Gzip => Box::new(BufReader::new(GzipDecoder::new(reader))),
            ContentEncoding::Zstd => Box::new(BufReader::new(ZstdDecoder::new(reader))),
        };
    }

    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::with_capacity(8 * 1024);
    let mut parser = MultistatusParser::new(sink);

    loop {
        match xml.read_event_into_async(&mut buf).await {
            Ok(Event::Start(e)) => parser.on_start(&e, xml.decoder())?,
            Ok(Event::Empty(e)) => {
                parser.on_start(&e, xml.decoder())?;
                parser.on_end(e.name().as_ref())?;
            }
            Ok(Event::Text(e)) => {
                let text = decode_text(e.as_ref())?;
                parser.on_text(text);
            }
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).into_owned();
                parser.on_cdata(text);
            }
            Ok(Event::End(e)) => parser.on_end(e.name().as_ref())?,
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow!("XML parsing error: {e}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(parser.finish())
}

fn parse_multistatus_bytes_with<R, C>(reader: R, sink: C) -> Result<ParseResult<C>>
where
    R: BufRead,
    C: ItemConsumer,
{
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::with_capacity(8 * 1024);
    let mut parser = MultistatusParser::new(sink);

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => parser.on_start(&e, xml.decoder())?,
            Ok(Event::Empty(e)) => {
                parser.on_start(&e, xml.decoder())?;
                parser.on_end(e.name().as_ref())?;
            }
            Ok(Event::Text(e)) => {
                let text = decode_text(e.as_ref())?;
                parser.on_text(text);
            }
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).into_owned();
                parser.on_cdata(text);
            }
            Ok(Event::End(e)) => parser.on_end(e.name().as_ref())?,
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow!("XML error: {e}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(parser.finish())
}

/// Parse a WebDAV `207 Multi-Status` XML body in **streaming mode**, with optional
/// decompression (br, gzip, zstd).
///
/// This function avoids loading the entire response into memory, making it suitable
/// for very large CalDAV/WebDAV collections.
#[cfg(test)]
pub(crate) async fn parse_multistatus_stream(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
) -> Result<ParseResult<Vec<DavItem>>> {
    parse_multistatus_stream_with(resp_body, encodings, Vec::<DavItem>::new()).await
}

/// Stream parse a WebDAV `207 Multi-Status` response and invoke a callback for each item.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) async fn parse_multistatus_stream_visit<F>(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
    on_item: F,
) -> Result<Option<String>>
where
    F: FnMut(DavItem) -> Result<()> + Send,
{
    let result = parse_multistatus_stream_with(resp_body, encodings, on_item).await?;
    Ok(result.sync_token)
}

/// Parse a WebDAV `207 Multi-Status` XML body from an already aggregated buffer.
pub(crate) fn parse_multistatus_bytes(body: &[u8]) -> Result<ParseResult<Vec<DavItem>>> {
    let cursor = Cursor::new(body);
    parse_multistatus_bytes_with(cursor, Vec::<DavItem>::new())
}

/// Stream parse an aggregated multistatus body via callback.
#[cfg(test)]
pub(crate) fn parse_multistatus_bytes_visit<F>(body: &[u8], on_item: F) -> Result<Option<String>>
where
    F: FnMut(DavItem) -> Result<()>,
{
    let cursor = Cursor::new(body);
    let result = parse_multistatus_bytes_with(cursor, on_item)?;
    Ok(result.sync_token)
}

pub fn decode_text(raw: &[u8]) -> Result<String> {
    match std::str::from_utf8(raw) {
        Ok(s) => Ok(unescape(s)
            .map_err(|err| anyhow!("XML decode error: {err}"))?
            .into_owned()),
        Err(_) => Ok(String::from_utf8_lossy(raw).into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::compression::detect_encodings;
    use anyhow::{Result, anyhow};
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::Request;
    use hyper::client::conn::http1;
    use hyper_util::rt::TokioIo;
    use std::time::Instant;
    use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

    async fn parse_streaming_xml(xml: &str) -> Result<ParseResult<Vec<DavItem>>> {
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
        let encodings = detect_encodings(resp.headers());
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
            element_from_bytes(b"C:calendar-data"),
            ElementName::CalendarData
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
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/cal1/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Calendar One</D:displayname>
        <D:getetag>"etag-1"</D:getetag>
      </D:prop>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/cal2/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Calendar Two</D:displayname>
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
    async fn test_streaming_preserves_multiline_calendar_data() -> Result<()> {
        let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/cal/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-data><![CDATA[BEGIN:VCALENDAR
]]><![CDATA[END:VCALENDAR
]]></C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

        let result = parse_streaming_xml(xml).await?;
        assert_eq!(result.items.len(), 1);

        let item = &result.items[0];
        let data = item.calendar_data.as_ref().expect("calendar data present");
        assert_eq!(data, "BEGIN:VCALENDAR\nEND:VCALENDAR\n");
        Ok(())
    }

    #[test]
    fn parse_multistatus_extracts_calendar_properties() {
        let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-home-set>
          <D:href>/dav/user01/</D:href>
        </C:calendar-home-set>
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
          <C:calendar/>
        </D:resourcetype>
        <C:supported-calendar-component-set>
          <C:comp name="VEVENT"/>
          <C:comp name="VTODO"/>
        </C:supported-calendar-component-set>
        <C:calendar-data><![CDATA[BEGIN:VCALENDAR
END:VCALENDAR
]]></C:calendar-data>
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
        assert_eq!(collection.calendar_home_set, vec!["/dav/user01/"]);

        let calendar = &items[1];
        assert!(calendar.is_calendar);
        assert_eq!(calendar.displayname.as_deref(), Some("Personal"));
        assert_eq!(
            calendar.supported_components,
            vec!["VEVENT".to_string(), "VTODO".to_string()]
        );
        assert_eq!(calendar.etag.as_deref(), Some("\"etag-123\""));
        assert_eq!(calendar.sync_token.as_deref(), Some("token-123"));
        let data = calendar
            .calendar_data
            .as_ref()
            .expect("calendar data present");
        assert!(data.contains("BEGIN:VCALENDAR"));
        assert_eq!(calendar.href, "/dav/user01/personal/");
    }

    #[test]
    fn parse_multistatus_extracts_common_properties_and_top_level_sync_token() {
        let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:sync-token>top-token</D:sync-token>
  <D:response>
    <D:href>/dav/user01/cal/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Work</D:displayname>
        <D:getetag>"etag-999"</D:getetag>
        <D:resourcetype>
          <D:collection/>
          <C:calendar/>
        </D:resourcetype>
        <D:sync-token>item-token</D:sync-token>
        <D:current-user-principal>
          <D:href>/principals/user01/</D:href>
        </D:current-user-principal>
        <D:owner>
          <D:href>/principals/user01/</D:href>
        </D:owner>
        <D:getcontenttype>text/calendar</D:getcontenttype>
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
        assert_eq!(item.href, "/dav/user01/cal/");
        assert_eq!(item.status.as_deref(), Some("HTTP/1.1 200 OK"));
        assert_eq!(item.displayname.as_deref(), Some("Work"));
        assert_eq!(item.etag.as_deref(), Some("\"etag-999\""));
        assert!(item.is_collection);
        assert!(item.is_calendar);
        assert_eq!(item.sync_token.as_deref(), Some("item-token"));
        assert_eq!(item.current_user_principal, vec!["/principals/user01/"]);
        assert_eq!(item.owner.as_deref(), Some("/principals/user01/"));
        assert_eq!(item.content_type.as_deref(), Some("text/calendar"));
        assert_eq!(
            item.last_modified.as_deref(),
            Some("Mon, 01 Jan 2024 00:00:00 GMT")
        );
    }

    #[test]
    fn parse_multistatus_preserves_multiline_calendar_data() {
        let xml = r#"
<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/cal/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-data><![CDATA[BEGIN:VCALENDAR
]]><![CDATA[END:VCALENDAR
]]></C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#;

        let result = parse_multistatus_bytes(xml.as_bytes()).expect("xml parsing succeeds");
        assert_eq!(result.items.len(), 1);

        let item = &result.items[0];
        let data = item.calendar_data.as_ref().expect("calendar data present");
        assert_eq!(data, "BEGIN:VCALENDAR\nEND:VCALENDAR\n");
    }

    #[test]
    fn test_parse_multistatus_performance() {
        // Create a large multistatus response with many items
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<D:multistatus xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">",
        );

        // Add 1000 response items
        for i in 0..1000 {
            xml.push_str(&format!(
                r#"
  <D:response>
    <D:href>/dav/user01/event{}.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-{}"</D:getetag>
        <D:resourcetype/>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>"#,
                i, i
            ));
        }

        xml.push_str("\n</D:multistatus>");

        let start = Instant::now();
        let items = parse_multistatus_bytes(xml.as_bytes())
            .expect("Parsing should succeed")
            .items;
        let duration = start.elapsed();

        assert_eq!(items.len(), 1000);
        assert!(
            duration.as_millis() < 1000,
            "Parsing should complete in less than 1 second"
        );
    }

    #[test]
    fn test_parse_multistatus_malformed_xml() {
        // Test with malformed XML
        let malformed_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/event1.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
      </D:prop>
      <!-- Missing closing tags -->
"#;

        let result = parse_multistatus_bytes(malformed_xml.as_bytes());
        // Depending on the parser implementation, this might either error or partially parse
        // The important thing is that it doesn't panic or cause undefined behavior
        println!("Malformed XML parsing result: {:?}", result.is_ok());
    }

    #[test]
    fn test_parse_multistatus_unexpected_elements() {
        // Test with unexpected XML elements
        let xml_with_extra = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav/user01/event1.ics</D:href>
    <unexpected-element>Should be ignored</unexpected-element>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
        <unknown-property>Should be ignored</unknown-property>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
      <extra-element>Should be ignored</extra-element>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let items = parse_multistatus_bytes(xml_with_extra.as_bytes())
            .expect("Parsing should succeed")
            .items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].href, "/dav/user01/event1.ics");
        assert_eq!(items[0].etag.as_deref(), Some("\"etag-1\""));
    }
}
