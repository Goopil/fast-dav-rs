use crate::carddav::types::DavItem;
use crate::common::compression::ContentEncoding;
use crate::webdav::streaming::{CommonParser, path_ends_with};
use anyhow::{Result, anyhow};
use futures_util::TryStreamExt;
use http_body_util::BodyStream;
use hyper::body::Incoming;
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};
use std::io::{BufRead, Cursor};
use tokio::io::AsyncBufRead;
use tokio::io::BufReader;
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
    Addressbook,
    SupportedAddressData,
    AddressDataType,
    AddressData,
    AddressbookDescription,
    AddressbookColor,
    SyncToken,
    AddressbookHomeSet,
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
    } else if local.eq_ignore_ascii_case(b"addressbook") {
        ElementName::Addressbook
    } else if local.eq_ignore_ascii_case(b"supported-address-data") {
        ElementName::SupportedAddressData
    } else if local.eq_ignore_ascii_case(b"address-data-type") {
        ElementName::AddressDataType
    } else if local.eq_ignore_ascii_case(b"address-data") {
        ElementName::AddressData
    } else if local.eq_ignore_ascii_case(b"addressbook-description") {
        ElementName::AddressbookDescription
    } else if local.eq_ignore_ascii_case(b"addressbook-color") {
        ElementName::AddressbookColor
    } else if local.eq_ignore_ascii_case(b"sync-token") {
        ElementName::SyncToken
    } else if local.eq_ignore_ascii_case(b"addressbook-home-set") {
        ElementName::AddressbookHomeSet
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

    fn on_start(&mut self, event: &BytesStart<'_>) -> Result<()> {
        self.common.on_start(event.name().as_ref());
        let element = element_from_bytes(event.name().as_ref());
        self.stack.push(element);

        match element {
            ElementName::Response => {
                self.current = DavItem::new();
            }
            ElementName::Addressbook => {
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::Resourcetype,
                    ElementName::Addressbook,
                ]) {
                    self.current.is_addressbook = true;
                }
            }
            ElementName::AddressDataType => {
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::SupportedAddressData,
                    ElementName::AddressDataType,
                ]) {
                    let mut content_type = None;
                    let mut version = None;
                    for attr in event.attributes().with_checks(false) {
                        let attr = attr?;
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                        if key == "content-type" {
                            let value = attr
                                .unescape_value()
                                .map_err(|e| anyhow!("Invalid XML attribute: {e}"))?
                                .into_owned();
                            if !value.is_empty() {
                                content_type = Some(value);
                            }
                        } else if key == "version" {
                            let value = attr
                                .unescape_value()
                                .map_err(|e| anyhow!("Invalid XML attribute: {e}"))?
                                .into_owned();
                            if !value.is_empty() {
                                version = Some(value);
                            }
                        }
                    }
                    if let Some(content_type) = content_type {
                        let value = if let Some(version) = version {
                            format!("{content_type};version={version}")
                        } else {
                            content_type
                        };
                        if !self
                            .current
                            .supported_address_data
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(&value))
                        {
                            self.current.supported_address_data.push(value);
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

        // address-data is often multi-line and may arrive in chunks; keep exact payload.
        if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::AddressData,
        ]) {
            if let Some(existing) = self.current.address_data.as_mut() {
                existing.push_str(&text);
            } else {
                self.current.address_data = Some(text);
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
            ElementName::AddressbookDescription,
        ]) {
            self.current.addressbook_description = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::AddressbookColor,
        ]) {
            self.current.addressbook_color = Some(trimmed.to_string());
        } else if self.path_ends_with(&[ElementName::Multistatus, ElementName::SyncToken]) {
            // Top-level sync-token in sync-collection responses (RFC 6578)
            self.sync_token = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::AddressbookHomeSet,
            ElementName::Href,
        ]) {
            self.current.addressbook_home_set.push(trimmed.to_string());
        }
    }
}

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
            Ok(Event::Start(e)) => parser.on_start(&e)?,
            Ok(Event::Empty(e)) => {
                parser.on_start(&e)?;
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
            Ok(Event::Start(e)) => parser.on_start(&e)?,
            Ok(Event::Empty(e)) => {
                parser.on_start(&e)?;
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
/// for very large CardDAV/WebDAV collections.
pub async fn parse_multistatus_stream(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
) -> Result<ParseResult<Vec<DavItem>>> {
    parse_multistatus_stream_with(resp_body, encodings, Vec::<DavItem>::new()).await
}

/// Stream parse a WebDAV `207 Multi-Status` response and invoke a callback for each item.
pub async fn parse_multistatus_stream_visit<F>(
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
pub fn parse_multistatus_bytes(body: &[u8]) -> Result<ParseResult<Vec<DavItem>>> {
    let cursor = Cursor::new(body);
    parse_multistatus_bytes_with(cursor, Vec::<DavItem>::new())
}

/// Stream parse an aggregated multistatus body via callback.
pub fn parse_multistatus_bytes_visit<F>(body: &[u8], on_item: F) -> Result<Option<String>>
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
