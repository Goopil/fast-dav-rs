use crate::common::compression::ContentEncoding;
use crate::webdav::client::{normalize_etag, normalize_sync_token};
use crate::webdav::types::{DavItemCommon, PropStat, WebDavError};
use crate::{Error, Result};
use quick_xml::escape::unescape;
use std::time::Duration;

/// Compact dispatch of a single XML event to a [`MultistatusParser`], shared by
/// the streaming (async) and aggregated (sync) parse loops.
///
/// Returns `true` when `EOF` was reached.
fn dispatch_event<C: ItemConsumer>(
    parser: &mut MultistatusParser<C>,
    decoder: Decoder,
    event: quick_xml::Result<Event<'_>>,
) -> Result<bool> {
    match event {
        Ok(Event::Start(e)) => parser.on_start(&e, decoder)?,
        Ok(Event::Empty(e)) => {
            parser.on_start(&e, decoder)?;
            parser.on_end(e.name().as_ref())?;
        }
        Ok(Event::Text(e)) => parser.on_text(decode_text(e.as_ref())?),
        Ok(Event::CData(e)) => {
            parser.on_cdata(String::from_utf8_lossy(e.as_ref()).into_owned());
        }
        Ok(Event::End(e)) => parser.on_end(e.name().as_ref())?,
        Ok(Event::Eof) => return Ok(true),
        Err(error) => return Err(Error::from_quick_xml(error)),
        _ => {}
    }
    Ok(false)
}

pub(crate) struct CommonParser {
    stack: Vec<ElementName>,
    current: DavItemCommon,
    current_propstat_status: Option<String>,
    current_prop_names: Vec<String>,
    first_200_propstat_applied: bool,
}

pub(crate) fn path_ends_with<T: PartialEq>(stack: &[T], needle: &[T]) -> bool {
    stack.len() >= needle.len() && stack[stack.len() - needle.len()..] == needle[..]
}

impl CommonParser {
    pub(crate) fn new() -> Self {
        Self {
            stack: Vec::with_capacity(16),
            current: DavItemCommon::default(),
            current_propstat_status: None,
            current_prop_names: Vec::new(),
            first_200_propstat_applied: false,
        }
    }

    pub(crate) fn on_start(&mut self, raw: &[u8]) {
        let element = element_from_bytes(raw);
        self.stack.push(element);

        match element {
            ElementName::Response => {
                self.current = DavItemCommon::default();
                self.first_200_propstat_applied = false;
            }
            ElementName::Propstat => {
                self.current_propstat_status = None;
                self.current_prop_names = Vec::new();
            }
            ElementName::Collection
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::Resourcetype,
                    ElementName::Collection,
                ]) =>
            {
                self.current.is_collection = true;
            }
            _ => {}
        }

        if self.stack.len() >= 4
            && self.stack[self.stack.len() - 4] == ElementName::Response
            && self.stack[self.stack.len() - 3] == ElementName::Propstat
            && self.stack[self.stack.len() - 2] == ElementName::Prop
        {
            let name = String::from_utf8_lossy(local_name(raw)).to_string();
            if !self.current_prop_names.contains(&name) {
                self.current_prop_names.push(name);
            }
        }
    }

    pub(crate) fn on_end(&mut self, raw: &[u8]) -> Result<()> {
        let element = element_from_bytes(raw);
        if element == ElementName::Propstat {
            let status = self.current_propstat_status.take();
            let prop_names = std::mem::take(&mut self.current_prop_names);
            let is_200 = status
                .as_deref()
                .and_then(crate::webdav::types::http_status_code)
                .map(|c| c == 200)
                .unwrap_or(false);
            if is_200 && !self.first_200_propstat_applied {
                self.first_200_propstat_applied = true;
            } else if !is_200 && self.current.status.is_none() {
                self.current.status = status.clone();
            }
            self.current.propstats.push(PropStat { status, prop_names });
        }
        match self.stack.pop() {
            Some(popped) if popped == element => Ok(()),
            Some(popped) => Err(Error::XmlStructure(format!(
                "closing tag </{}> does not match the last opened element (expected {popped:?}, found {element:?})",
                String::from_utf8_lossy(raw)
            ))),
            None => Err(Error::XmlStructure(format!(
                "closing tag </{}> without a matching opening tag",
                String::from_utf8_lossy(raw)
            ))),
        }
    }

    pub(crate) fn on_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        if self.path_ends_with(&[ElementName::Response, ElementName::Href]) {
            self.current.href = trimmed.to_string();
        } else if self.path_ends_with(&[ElementName::Response, ElementName::Status]) {
            self.current.response_status = Some(trimmed.to_string());
            if self.current.status.is_none() {
                self.current.status = Some(trimmed.to_string());
            }
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Status,
        ]) {
            self.current_propstat_status = Some(trimmed.to_string());
            if !self.first_200_propstat_applied
                && crate::webdav::types::http_status_code(trimmed) == Some(200)
            {
                self.current.status = Some(trimmed.to_string());
                self.first_200_propstat_applied = true;
            }
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::Displayname,
        ]) {
            self.current.displayname = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::Getetag,
        ]) {
            self.current.etag = Some(normalize_etag(trimmed));
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::SyncToken,
        ]) {
            self.current.sync_token = Some(normalize_sync_token(trimmed));
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CurrentUserPrincipal,
            ElementName::Href,
        ]) {
            self.current
                .current_user_principal
                .push(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::Owner,
            ElementName::Href,
        ]) {
            self.current.owner = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::Getcontenttype,
        ]) {
            self.current.content_type = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::Getlastmodified,
        ]) {
            self.current.last_modified = Some(trimmed.to_string());
        }
    }

    pub(crate) fn finish_response(&mut self) -> DavItemCommon {
        std::mem::take(&mut self.current)
    }

    fn path_ends_with(&self, needle: &[ElementName]) -> bool {
        path_ends_with(&self.stack, needle)
    }
}

// ---------------------------------------------------------------------------
// Unified multistatus parser (shared by CalDAV and CardDAV)
// ---------------------------------------------------------------------------

use crate::common::compression::{body_stream_reader, stack_decoders};
use crate::webdav::types::DavItem;
use hyper::body::Incoming;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Decoder, Reader, XmlVersion};
use std::io::{BufRead, Cursor};

/// Default **idle** timeout for streaming multistatus reads.
///
/// This bounds the time the parser waits for the next XML event to become available,
/// i.e. the maximum period of inactivity between two reads making progress. It is not
/// a cap on the total parse duration: arbitrarily large responses are fine as long as
/// data keeps flowing.
pub const STREAM_READ_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Element names inside a `207 Multi-Status` body — union of the DAV core,
/// CalDAV, and CardDAV element sets. Domain variants only appear in responses
/// from the matching server type.
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
    Addressbook,
    SupportedAddressData,
    AddressDataType,
    AddressData,
    AddressbookDescription,
    AddressbookColor,
    AddressbookHomeSet,
    CurrentUserPrincipal,
    Owner,
    Getcontenttype,
    Getlastmodified,
    Other,
}

pub fn element_from_bytes(raw: &[u8]) -> ElementName {
    let local = local_name(raw);

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

    fn finish(self) -> Result<ParseResult<C>> {
        if let Some(unclosed) = self.stack.last() {
            return Err(Error::XmlStructure(format!(
                "unexpected end of input with unclosed element {unclosed:?}"
            )));
        }

        Ok(ParseResult {
            items: self.sink,
            sync_token: self.sync_token,
        })
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
            ElementName::Addressbook
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::Resourcetype,
                    ElementName::Addressbook,
                ]) =>
            {
                self.current.is_addressbook = true;
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
                for attr in event.attributes().with_checks(true) {
                    let attr = attr?;
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                    if key == "name" {
                        let value = attr
                            .decoded_and_normalized_value(XmlVersion::default(), decoder)?
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
            ElementName::AddressDataType
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::SupportedAddressData,
                    ElementName::AddressDataType,
                ]) =>
            {
                let mut content_type = None;
                let mut version = None;
                for attr in event.attributes().with_checks(true) {
                    let attr = attr?;
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                    if key == "content-type" {
                        let value = attr
                            .decoded_and_normalized_value(XmlVersion::default(), decoder)?
                            .into_owned();
                        if !value.is_empty() {
                            content_type = Some(value);
                        }
                    } else if key == "version" {
                        let value = attr
                            .decoded_and_normalized_value(XmlVersion::default(), decoder)?
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
            _ => {}
        }

        Ok(())
    }

    fn on_end(&mut self, name: &[u8]) -> Result<()> {
        self.common.on_end(name)?;
        if let Some(popped) = self.stack.pop() {
            if popped == ElementName::Response {
                let common = self.common.finish_response();
                self.current.apply_common(common);
                let finished = std::mem::take(&mut self.current);
                self.sink.consume(finished)?;
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

        // calendar-data / address-data are often multi-line and may arrive in
        // chunks; keep the exact payload.
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
        } else if self.path_ends_with(&[
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
            self.sync_token = Some(normalize_sync_token(trimmed));
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarHomeSet,
            ElementName::Href,
        ]) {
            self.current.calendar_home_set.push(trimmed.to_string());
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
    idle_timeout: Duration,
) -> Result<ParseResult<C>>
where
    C: ItemConsumer + Send,
{
    let reader = stack_decoders(body_stream_reader(resp_body), encodings);

    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::with_capacity(8 * 1024);
    let mut parser = MultistatusParser::new(sink);

    while !dispatch_event(
        &mut parser,
        xml.decoder(),
        tokio::time::timeout(idle_timeout, xml.read_event_into_async(&mut buf))
            .await
            .map_err(|_| Error::Timeout {
                limit: idle_timeout,
            })?,
    )? {
        buf.clear();
    }

    parser.finish()
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

    while !dispatch_event(&mut parser, xml.decoder(), xml.read_event_into(&mut buf))? {
        buf.clear();
    }

    parser.finish()
}

/// Parse a WebDAV `207 Multi-Status` XML body in **streaming mode**, with optional
/// decompression (br, gzip, zstd).
///
/// This function avoids loading the entire response into memory, making it suitable
/// for very large CalDAV/WebDAV collections.
///
/// Reads are bounded by the default idle timeout ([`STREAM_READ_IDLE_TIMEOUT`]); use
/// [`parse_multistatus_stream_with_timeout`] to customize it.
pub async fn parse_multistatus_stream(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
) -> Result<ParseResult<Vec<DavItem>>> {
    parse_multistatus_stream_with_timeout(resp_body, encodings, STREAM_READ_IDLE_TIMEOUT).await
}

/// Variant of [`parse_multistatus_stream`] with a caller-provided **idle** timeout.
///
/// `idle_timeout` is the maximum time allowed between two reads making progress
/// (i.e. waiting for the next XML event to arrive from the network). It is **not**
/// a cap on the total parse duration, so huge-but-flowing responses are unaffected.
/// When the timeout elapses, an error is returned and parsing stops.
pub async fn parse_multistatus_stream_with_timeout(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
    idle_timeout: Duration,
) -> Result<ParseResult<Vec<DavItem>>> {
    parse_multistatus_stream_with(resp_body, encodings, Vec::<DavItem>::new(), idle_timeout).await
}

/// Stream parse a WebDAV `207 Multi-Status` response and invoke a callback for each item.
///
/// Reads are bounded by the default idle timeout ([`STREAM_READ_IDLE_TIMEOUT`]); use
/// [`parse_multistatus_stream_visit_with_timeout`] to customize it.
pub async fn parse_multistatus_stream_visit<F>(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
    on_item: F,
) -> Result<Option<String>>
where
    F: FnMut(DavItem) -> Result<()> + Send,
{
    parse_multistatus_stream_visit_with_timeout(
        resp_body,
        encodings,
        STREAM_READ_IDLE_TIMEOUT,
        on_item,
    )
    .await
}

/// Variant of [`parse_multistatus_stream_visit`] with a caller-provided **idle** timeout.
///
/// `idle_timeout` is the maximum time allowed between two reads making progress
/// (i.e. waiting for the next XML event to arrive from the network). It is **not**
/// a cap on the total parse duration, so huge-but-flowing responses are unaffected.
/// When the timeout elapses, an error is returned and parsing stops.
pub async fn parse_multistatus_stream_visit_with_timeout<F>(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
    idle_timeout: Duration,
    on_item: F,
) -> Result<Option<String>>
where
    F: FnMut(DavItem) -> Result<()> + Send,
{
    let result = parse_multistatus_stream_with(resp_body, encodings, on_item, idle_timeout).await?;
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
        Ok(s) => Ok(unescape(s)?.into_owned()),
        Err(_) => Ok(String::from_utf8_lossy(raw).into_owned()),
    }
}

pub(crate) fn parse_current_user_principal_bytes(body: &[u8]) -> Result<Option<String>> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    use std::io::Cursor;
    let cursor = Cursor::new(body);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::with_capacity(8 * 1024);
    let mut parser = CommonParser::new();
    let mut principal = None;

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => parser.on_start(e.name().as_ref()),
            Ok(Event::Empty(e)) => {
                parser.on_start(e.name().as_ref());
                parser.on_end(e.name().as_ref())?;
            }
            Ok(Event::Text(e)) => {
                let text = decode_text(e.as_ref())?;
                parser.on_text(&text);
            }
            Ok(Event::End(e)) => {
                parser.on_end(e.name().as_ref())?;
                let name = e.name();
                if local_name(name.as_ref()).eq_ignore_ascii_case(b"response") {
                    let common = parser.finish_response();
                    if principal.is_none() {
                        if let Some(found) = common
                            .current_user_principal
                            .into_iter()
                            .find(|href| !href.is_empty())
                        {
                            principal = Some(found);
                            break;
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(Error::from_quick_xml(error)),
            _ => {}
        }
        buf.clear();
    }

    Ok(principal)
}

/// Parse a `<D:error>` body (RFC 4918 §14.12) into [`WebDavError`].
///
/// Server error responses (4xx/5xx) may include a `<D:error>` body whose
/// child element identifies the precondition or postcondition that failed.
/// This function extracts the local name of the first child element as
/// `precondition_code`. Returns a [`WebDavError`] with `precondition_code:
/// None` when the body is empty, not valid XML, or has no `<D:error>`
/// element with a child.
///
/// ```
/// use fast_dav_rs::webdav::parse_error_body;
///
/// let xml = br#"<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
///   <C:no-uid-conflict/>
/// </D:error>"#;
/// let err = parse_error_body(xml).unwrap();
/// assert_eq!(err.precondition_code.as_deref(), Some("no-uid-conflict"));
/// ```
pub fn parse_error_body(body: &[u8]) -> Result<WebDavError> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    use std::io::Cursor;

    let mut err = WebDavError::default();
    let trimmed = body.trim_ascii();
    if trimmed.is_empty() {
        return Ok(err);
    }
    let cursor = Cursor::new(trimmed);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::with_capacity(4 * 1024);
    let mut in_error = false;
    let mut found = false;

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_vec();
                let local = local_name(&name);
                if local.eq_ignore_ascii_case(b"error") {
                    in_error = true;
                } else if in_error && !found {
                    err.precondition_code = Some(String::from_utf8_lossy(local).into_owned());
                    found = true;
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_vec();
                let local = local_name(&name);
                if local.eq_ignore_ascii_case(b"error") {
                    in_error = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                if matches!(
                    error,
                    quick_xml::Error::Syntax(_) | quick_xml::Error::IllFormed(_)
                ) {
                    return Ok(WebDavError::default());
                }
                return Err(Error::from_quick_xml(error));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(err)
}

fn local_name(raw: &[u8]) -> &[u8] {
    match raw.iter().position(|b| *b == b':') {
        Some(idx) => &raw[idx + 1..],
        None => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_end_mismatched_closing_tag() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        let err = parser.on_end(b"D:prop").unwrap_err();
        assert!(
            err.to_string().contains("does not match"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn on_end_without_opening_tag() {
        let mut parser = CommonParser::new();
        let err = parser.on_end(b"D:response").unwrap_err();
        assert!(
            err.to_string().contains("without a matching opening tag"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn on_text_sets_href() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:href");
        parser.on_text("/path/to/resource/");
        let resp = parser.finish_response();
        assert_eq!(resp.href, "/path/to/resource/");
    }

    #[test]
    fn on_text_sets_status_direct() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:status");
        parser.on_text("HTTP/1.1 200 OK");
        let resp = parser.finish_response();
        assert_eq!(resp.status.as_deref(), Some("HTTP/1.1 200 OK"));
    }

    #[test]
    fn on_text_sets_status_in_propstat() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:status");
        parser.on_text("HTTP/1.1 200 OK");
        let resp = parser.finish_response();
        assert_eq!(resp.status.as_deref(), Some("HTTP/1.1 200 OK"));
    }

    #[test]
    fn on_text_sets_displayname() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:prop");
        parser.on_start(b"D:displayname");
        parser.on_text("My Calendar");
        let resp = parser.finish_response();
        assert_eq!(resp.displayname.as_deref(), Some("My Calendar"));
    }

    #[test]
    fn on_text_sets_etag() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:prop");
        parser.on_start(b"D:getetag");
        parser.on_text("\"abc123\"");
        let resp = parser.finish_response();
        assert_eq!(resp.etag.as_deref(), Some("abc123"));
    }

    #[test]
    fn on_text_sets_sync_token() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:prop");
        parser.on_start(b"D:sync-token");
        parser.on_text("http://sync/123");
        let resp = parser.finish_response();
        assert_eq!(resp.sync_token.as_deref(), Some("http://sync/123"));
    }

    #[test]
    fn on_text_sets_current_user_principal() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:prop");
        parser.on_start(b"D:current-user-principal");
        parser.on_start(b"D:href");
        parser.on_text("/principals/me/");
        let resp = parser.finish_response();
        assert_eq!(
            resp.current_user_principal,
            vec!["/principals/me/".to_string()]
        );
    }

    #[test]
    fn on_text_sets_owner() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:prop");
        parser.on_start(b"D:owner");
        parser.on_start(b"D:href");
        parser.on_text("/owners/me/");
        let resp = parser.finish_response();
        assert_eq!(resp.owner.as_deref(), Some("/owners/me/"));
    }

    #[test]
    fn on_text_sets_content_type() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:prop");
        parser.on_start(b"D:getcontenttype");
        parser.on_text("text/calendar");
        let resp = parser.finish_response();
        assert_eq!(resp.content_type.as_deref(), Some("text/calendar"));
    }

    #[test]
    fn on_text_sets_last_modified() {
        let mut parser = CommonParser::new();
        parser.on_start(b"D:response");
        parser.on_start(b"D:propstat");
        parser.on_start(b"D:prop");
        parser.on_start(b"D:getlastmodified");
        parser.on_text("Mon, 01 Jan 2024 00:00:00 GMT");
        let resp = parser.finish_response();
        assert_eq!(
            resp.last_modified.as_deref(),
            Some("Mon, 01 Jan 2024 00:00:00 GMT")
        );
    }

    #[test]
    fn on_text_empty_is_noop() {
        let mut parser = CommonParser::new();
        parser.on_text("");
        let resp = parser.finish_response();
        assert!(resp.href.is_empty());
        assert!(resp.status.is_none());
    }

    #[test]
    fn on_text_whitespace_only_is_noop() {
        let mut parser = CommonParser::new();
        parser.on_text("   \n\t  ");
        let resp = parser.finish_response();
        assert!(resp.href.is_empty());
        assert!(resp.status.is_none());
    }

    #[test]
    fn on_text_no_context_is_noop() {
        let mut parser = CommonParser::new();
        parser.on_text("orphan text");
        let resp = parser.finish_response();
        assert!(resp.href.is_empty());
        assert!(resp.status.is_none());
    }

    fn cup_principal(href: &str) -> String {
        format!("<D:current-user-principal><D:href>{href}</D:href></D:current-user-principal>")
    }

    fn cup_response(prop: &str) -> String {
        format!(
            "<D:response><D:href>/</D:href><D:propstat><D:prop>{prop}</D:prop>\
             <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>"
        )
    }

    fn cup_doc(responses: &str) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
             <D:multistatus xmlns:D=\"DAV:\">{responses}</D:multistatus>"
        )
        .into_bytes()
    }

    #[test]
    fn parse_current_user_principal_bytes_valid() {
        let xml = cup_doc(&cup_response(&cup_principal("/principals/user/")));
        let result = parse_current_user_principal_bytes(&xml).unwrap();
        assert_eq!(result.as_deref(), Some("/principals/user/"));
    }

    #[test]
    fn parse_current_user_principal_bytes_no_principal() {
        let xml = cup_doc(&cup_response("<D:displayname>My Cal</D:displayname>"));
        let result = parse_current_user_principal_bytes(&xml).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_current_user_principal_bytes_empty_href_skipped() {
        let xml = cup_doc(&cup_response(&cup_principal("")));
        let result = parse_current_user_principal_bytes(&xml).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_current_user_principal_bytes_multi_response_picks_second() {
        let xml = cup_doc(&format!(
            "{}{}",
            cup_response(&cup_principal("")),
            cup_response(&cup_principal("/principals/second/"))
        ));
        let result = parse_current_user_principal_bytes(&xml).unwrap();
        assert_eq!(result.as_deref(), Some("/principals/second/"));
    }

    #[test]
    fn parse_current_user_principal_bytes_malformed_xml() {
        let xml = b"<D:multistatus><D:response><D:prop";
        let result = parse_current_user_principal_bytes(xml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_current_user_principal_bytes_first_match_wins() {
        let xml = cup_doc(&format!(
            "{}{}",
            cup_response(&cup_principal("/principals/first/")),
            cup_response(&cup_principal("/principals/second/"))
        ));
        let result = parse_current_user_principal_bytes(&xml).unwrap();
        assert_eq!(result.as_deref(), Some("/principals/first/"));
    }
}
