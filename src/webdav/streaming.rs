use crate::common::compression::ContentEncoding;
use crate::webdav::client::{normalize_etag, normalize_sync_token};
use crate::webdav::types::{DavItemCommon, Depth, LockInfo, LockScope, PropStat, WebDavError};
use crate::{Error, Result};
use quick_xml::escape::{EscapeError, unescape};
use std::time::Duration;

/// Compact dispatch of a single XML event to a [`MultistatusParser`], shared by
/// the streaming (async) and aggregated (sync) parse loops.
///
/// Text content is accumulated across [`Event::Text`], [`Event::GeneralRef`]
/// (entity references, which quick-xml splits out of text runs) and
/// [`Event::CData`] events, and flushed to the parser when the enclosing
/// element boundary is reached.
///
/// Returns `true` when `EOF` was reached.
fn dispatch_event<C: ItemConsumer>(
    parser: &mut MultistatusParser<C>,
    decoder: Decoder,
    event: quick_xml::Result<Event<'_>>,
) -> Result<bool> {
    match event {
        Ok(Event::Start(e)) => {
            parser.flush_text()?;
            parser.on_start(&e, decoder)?;
        }
        Ok(Event::Empty(e)) => {
            parser.flush_text()?;
            parser.on_start(&e, decoder)?;
            parser.on_end(e.name().as_ref())?;
        }
        Ok(Event::Text(e)) => parser.push_text(&decode_text(e.as_ref())?)?,
        Ok(Event::GeneralRef(e)) => parser.push_ref(e.as_ref())?,
        Ok(Event::CData(e)) => {
            parser.push_cdata(String::from_utf8_lossy(e.as_ref()).into_owned())?;
        }
        Ok(Event::End(e)) => {
            parser.flush_text()?;
            parser.on_end(e.name().as_ref())?;
        }
        Ok(Event::Eof) => {
            parser.flush_text()?;
            return Ok(true);
        }
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

use crate::common::compression::MAX_DECOMPRESSED_SIZE;
use crate::common::compression::{body_stream_reader, stack_decoders};
use crate::webdav::types::{DavItem, MediaType, Privilege};
use futures::StreamExt;
use hyper::body::Incoming;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Decoder, Reader, XmlVersion};
use std::io::{BufRead, Cursor};
use std::pin::Pin;
use tokio::io::AsyncBufRead;

/// Default **idle** timeout for streaming multistatus reads.
///
/// This bounds the time the parser waits for the next XML event to become available,
/// i.e. the maximum period of inactivity between two reads making progress. It is not
/// a cap on the total parse duration: arbitrarily large responses are fine as long as
/// data keeps flowing.
pub const STREAM_READ_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum bytes of text accumulated for a single parsed item (text runs plus
/// `calendar-data` / `address-data` / `calendar-timezone` payloads). Matches the
/// aggregate [`MAX_DECOMPRESSED_SIZE`] cap enforced on response bodies.
const MAX_ITEM_TEXT_BYTES: usize = MAX_DECOMPRESSED_SIZE as usize;

/// Append `text` to `target` unless that would push it past `limit`; aborts
/// with [`Error::BodyTooLarge`] (leaving `target` unchanged) instead of
/// growing an unbounded buffer.
fn append_capped(target: &mut String, text: &str, limit: usize) -> Result<()> {
    if target.len() + text.len() > limit {
        return Err(Error::BodyTooLarge { limit });
    }
    target.push_str(text);
    Ok(())
}

/// Element names inside a `207 Multi-Status` body — union of the DAV core,
/// CalDAV, and CardDAV element sets. Domain variants only appear in responses
/// from the matching server type.
///
/// The first 24 variants keep the discriminant order of the pre-unification
/// `caldav::streaming::ElementName`; CardDAV-only variants are appended after
/// [`ElementName::Other`] for 0.9 compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    Addressbook,
    SupportedAddressData,
    AddressDataType,
    AddressData,
    AddressbookDescription,
    AddressbookColor,
    AddressbookHomeSet,
    MaxResourceSize,
    SupportedCalendarData,
    CalendarDataType,
    MaxAttendeesPerInstance,
    ScheduleInboxUrl,
    ScheduleOutboxUrl,
    CalendarUserAddressSet,
    ManagedId,
    CurrentUserPrivilegeSet,
    Privilege,
}

/// Map a raw XML element name (`prefix:local` or `local`) to an
/// [`ElementName`].
///
/// Matching is performed on the local name with any prefix stripped, and is
/// ASCII-case-insensitive: `<D:HREF>`, `<d:Href>` and `<href>` all map to
/// [`ElementName::Href`]. This documented tolerance accepts servers that
/// emit non-canonical element-name casing in multistatus bodies.
///
/// Namespace resolution happens in the multistatus parser: a recognized
/// local name is only honored in the DAV:, CalDAV and CardDAV namespaces
/// (plus Apple's iCal namespace for the two color elements); colliding
/// foreign-namespace elements are rewritten to [`ElementName::Other`].
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
    } else if local.eq_ignore_ascii_case(b"max-resource-size") {
        ElementName::MaxResourceSize
    } else if local.eq_ignore_ascii_case(b"supported-calendar-data") {
        ElementName::SupportedCalendarData
    } else if local.eq_ignore_ascii_case(b"calendar-data-type") {
        ElementName::CalendarDataType
    } else if local.eq_ignore_ascii_case(b"max-attendees-per-instance") {
        ElementName::MaxAttendeesPerInstance
    } else if local.eq_ignore_ascii_case(b"current-user-principal") {
        ElementName::CurrentUserPrincipal
    } else if local.eq_ignore_ascii_case(b"schedule-inbox-url") {
        ElementName::ScheduleInboxUrl
    } else if local.eq_ignore_ascii_case(b"schedule-outbox-url") {
        ElementName::ScheduleOutboxUrl
    } else if local.eq_ignore_ascii_case(b"calendar-user-address-set") {
        ElementName::CalendarUserAddressSet
    } else if local.eq_ignore_ascii_case(b"managed-id") {
        ElementName::ManagedId
    } else if local.eq_ignore_ascii_case(b"current-user-privilege-set") {
        ElementName::CurrentUserPrivilegeSet
    } else if local.eq_ignore_ascii_case(b"privilege") {
        ElementName::Privilege
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

/// Map a privilege element's raw name to the typed [`Privilege`] (RFC 3744
/// §3, plus the CalDAV `read-free-busy` extension, RFC 4791 §6.1.1).
/// Unknown local names fall back to [`Privilege::Other`].
fn privilege_from_local_name(raw: &[u8]) -> Privilege {
    let local = local_name(raw);
    if local.eq_ignore_ascii_case(b"read") {
        Privilege::Read
    } else if local.eq_ignore_ascii_case(b"write") {
        Privilege::Write
    } else if local.eq_ignore_ascii_case(b"write-properties") {
        Privilege::WriteProperties
    } else if local.eq_ignore_ascii_case(b"write-content") {
        Privilege::WriteContent
    } else if local.eq_ignore_ascii_case(b"bind") {
        Privilege::Bind
    } else if local.eq_ignore_ascii_case(b"unbind") {
        Privilege::Unbind
    } else if local.eq_ignore_ascii_case(b"unlock") {
        Privilege::Unlock
    } else if local.eq_ignore_ascii_case(b"read-free-busy") {
        Privilege::ReadFreeBusy
    } else {
        Privilege::Other(String::from_utf8_lossy(local).into_owned())
    }
}

/// Parse the `content-type` (required) and `version` (optional) attributes
/// shared by `address-data-type` (RFC 6352 §6.2.2) and `calendar-data-type`
/// (RFC 4791 §5.2.4) elements. Attribute values are trimmed; empty or
/// whitespace-only values are treated as absent.
fn parse_type_attributes(
    event: &BytesStart<'_>,
    decoder: Decoder,
) -> Result<(Option<String>, Option<String>)> {
    let mut content_type = None;
    let mut version = None;
    for attr in event.attributes().with_checks(true) {
        let attr = attr?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
        if key == "content-type" {
            let value = attr
                .decoded_and_normalized_value(XmlVersion::default(), decoder)?
                .into_owned();
            let value = value.trim();
            if !value.is_empty() {
                content_type = Some(value.to_string());
            }
        } else if key == "version" {
            let value = attr
                .decoded_and_normalized_value(XmlVersion::default(), decoder)?
                .into_owned();
            let value = value.trim();
            if !value.is_empty() {
                version = Some(value.to_string());
            }
        }
    }
    Ok((content_type, version))
}

/// Namespace URIs in which multistatus element names are recognized.
const DAV_XML_NS: &[u8] = b"DAV:";
const CALDAV_XML_NS: &[u8] = b"urn:ietf:params:xml:ns:caldav";
const CARDDAV_XML_NS: &[u8] = b"urn:ietf:params:xml:ns:carddav";
/// Apple's iCal extension namespace; the CalDAV and CardDAV clients request
/// `calendar-color` / `addressbook-color` in it, so those two elements keep
/// being recognized there.
const APPLE_ICAL_XML_NS: &[u8] = b"http://apple.com/ns/ical/";

/// Marker prepended to the local name of a foreign-namespace element whose
/// local name collides with a recognized one (`(` cannot start an XML name,
/// so the rewritten form can never re-match a real element).
const FOREIGN_ELEMENT_MARKER: &[u8] = b"(foreign-ns)";

/// `true` when a recognized element may keep its typed name under namespace
/// `ns` (`None` = no declaration in scope).
fn ns_allows(ns: Option<&[u8]>, element: ElementName) -> bool {
    match ns {
        // Undeclared namespaces stay tolerated: some servers emit multistatus
        // bodies without any xmlns declaration, and prefix-stripped matching
        // has always accepted those.
        None => true,
        Some(ns) => {
            ns == DAV_XML_NS
                || ns == CALDAV_XML_NS
                || ns == CARDDAV_XML_NS
                || (ns == APPLE_ICAL_XML_NS
                    && matches!(
                        element,
                        ElementName::CalendarColor | ElementName::AddressbookColor
                    ))
        }
    }
}

/// In-scope XML namespace declarations during a multistatus parse.
///
/// Tracks `xmlns` / `xmlns:prefix` attributes with an undo log so closing an
/// element restores exactly the declarations it introduced.
struct NsScopes {
    /// Prefix → namespace URI; the empty prefix is the default namespace.
    bindings: std::collections::HashMap<Vec<u8>, Vec<u8>>,
    /// Reverse log of [`NsScopes::declare`] calls for scope restoration.
    undo: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    /// Undo-log length at each open element (one mark per start event).
    marks: Vec<usize>,
}

impl NsScopes {
    fn new() -> Self {
        Self {
            bindings: std::collections::HashMap::new(),
            undo: Vec::new(),
            marks: Vec::new(),
        }
    }

    fn declare(&mut self, prefix: &[u8], uri: Vec<u8>) {
        let previous = self.bindings.insert(prefix.to_vec(), uri);
        self.undo.push((prefix.to_vec(), previous));
    }

    /// Namespace URI declared for `raw`'s prefix, `None` when undeclared.
    /// An element without a prefix resolves through the default namespace.
    fn resolve(&self, raw: &[u8]) -> Option<&[u8]> {
        self.bindings.get(namespace_prefix(raw)).map(Vec::as_slice)
    }

    fn mark(&mut self) {
        self.marks.push(self.undo.len());
    }

    fn pop_mark(&mut self) {
        if let Some(mark) = self.marks.pop() {
            while self.undo.len() > mark {
                let (prefix, previous) = self.undo.pop().expect("undo log aligned with marks");
                match previous {
                    Some(uri) => {
                        self.bindings.insert(prefix, uri);
                    }
                    None => {
                        self.bindings.remove(&prefix);
                    }
                }
            }
        }
    }
}

impl ItemConsumer for Vec<DavItem> {
    fn consume(&mut self, item: DavItem) -> Result<()> {
        self.push(item);
        Ok(())
    }
}

/// Sink queueing parsed items as [`DavStreamEvent`]s for the streaming engine.
#[derive(Default)]
struct EventQueue {
    events: std::collections::VecDeque<DavStreamEvent>,
}

impl ItemConsumer for EventQueue {
    fn consume(&mut self, item: DavItem) -> Result<()> {
        self.events.push_back(DavStreamEvent::Item(item));
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

/// One incrementally parsed piece of a multistatus response, yielded by
/// [`multistatus_events`] as soon as it is complete.
#[derive(Debug)]
#[non_exhaustive]
// Events are consumed one at a time, so the inline size difference between
// variants costs nothing; boxing `Item` would add a heap allocation per
// entry in the hot streaming path.
#[allow(clippy::large_enum_variant)]
pub enum DavStreamEvent {
    /// A complete `<D:response>` entry.
    Item(DavItem),
    /// The multistatus `<D:sync-token>` content (RFC 6578). Emitted once,
    /// wherever the server places the element (commonly first or last child
    /// of `<D:multistatus>`).
    SyncToken(String),
}

/// Concrete stream returned by the item-by-item APIs.
///
/// Implements [`futures::Stream`] and is `Unpin`, so it can be driven
/// directly — no pinning ceremony:
///
/// ```ignore
/// let mut stream = client.calendar_query_stream(..).await?;
/// while let Some(object) = stream.next().await { … }
/// ```
pub struct ItemStream<T> {
    inner: Pin<Box<dyn futures::Stream<Item = Result<T>> + Send>>,
}

impl<T> std::fmt::Debug for ItemStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemStream").finish_non_exhaustive()
    }
}

impl<T> futures::Stream for ItemStream<T> {
    type Item = Result<T>;
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl<T> ItemStream<T> {
    pub(crate) fn new(inner: impl futures::Stream<Item = Result<T>> + Send + 'static) -> Self {
        Self {
            inner: Box::pin(inner),
        }
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
    /// Text run accumulated across [`Event::Text`], [`Event::GeneralRef`] and
    /// [`Event::CData`] events; flushed on element boundaries.
    text_buf: String,
    common: CommonParser,
    ns: NsScopes,
    sink: C,
}

impl<C: ItemConsumer> MultistatusParser<C> {
    pub fn new(sink: C) -> Self {
        Self {
            stack: Vec::with_capacity(16),
            current: DavItem::new(),
            sync_token: None,
            text_buf: String::new(),
            common: CommonParser::new(),
            ns: NsScopes::new(),
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
        // Namespace declarations carried by the element itself apply to it
        // (XML 1.0 §3.3), so declare before resolving.
        for attr in event.attributes().flatten() {
            let key = attr.key.as_ref();
            if key == b"xmlns" {
                self.ns.declare(b"", attr.value.into_owned());
            } else if let Some(prefix) = key.strip_prefix(b"xmlns:") {
                self.ns.declare(prefix, attr.value.into_owned());
            }
        }
        let binding = event.name();
        let raw = self.ns_qualified_name(binding.as_ref());
        self.common.on_start(&raw);
        let element = element_from_bytes(&raw);
        self.stack.push(element);
        self.ns.mark();

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
                let (content_type, version) = parse_type_attributes(event, decoder)?;
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
            ElementName::CalendarDataType
                if self.path_ends_with(&[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::SupportedCalendarData,
                    ElementName::CalendarDataType,
                ]) =>
            {
                let (content_type, version) = parse_type_attributes(event, decoder)?;
                if let Some(content_type) = content_type {
                    let media = MediaType {
                        content_type,
                        version,
                    };
                    if !self.current.supported_calendar_data.contains(&media) {
                        self.current.supported_calendar_data.push(media);
                    }
                }
            }
            _ => {}
        }

        // `current-user-privilege-set` (RFC 3744 §5.4): the property wraps
        // one or more `<D:privilege>` containers whose empty children name
        // the granted privileges. Capture each direct child of a container.
        let stack_len = self.stack.len();
        if stack_len > 5
            && path_ends_with(
                &self.stack[..stack_len - 1],
                &[
                    ElementName::Response,
                    ElementName::Propstat,
                    ElementName::Prop,
                    ElementName::CurrentUserPrivilegeSet,
                    ElementName::Privilege,
                ],
            )
        {
            let binding = event.name();
            let privilege_raw = binding.as_ref();
            let privilege = if ns_allows(self.ns.resolve(privilege_raw), ElementName::Other) {
                privilege_from_local_name(privilege_raw)
            } else {
                Privilege::Other(String::from_utf8_lossy(local_name(privilege_raw)).into_owned())
            };
            self.current.current_user_privileges.push(privilege);
        }

        Ok(())
    }

    /// Resolve `raw` against the in-scope namespace declarations and return
    /// the name bytes to parse with.
    ///
    /// Recognized local names are only honored in the accepted namespaces
    /// (see [`ns_allows`]); a colliding name from a foreign namespace is
    /// rewritten to a non-XML-name form so it parses as [`ElementName::Other`]
    /// while keeping the original local name visible in property listings.
    fn ns_qualified_name<'a>(&self, raw: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        let element = element_from_bytes(raw);
        if element == ElementName::Other || ns_allows(self.ns.resolve(raw), element) {
            std::borrow::Cow::Borrowed(raw)
        } else {
            let mut rewritten = FOREIGN_ELEMENT_MARKER.to_vec();
            rewritten.extend_from_slice(local_name(raw));
            std::borrow::Cow::Owned(rewritten)
        }
    }

    fn on_end(&mut self, name: &[u8]) -> Result<()> {
        let name = self.ns_qualified_name(name);
        self.common.on_end(&name)?;
        self.ns.pop_mark();
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

    fn push_text(&mut self, text: &str) -> Result<()> {
        append_capped(&mut self.text_buf, text, MAX_ITEM_TEXT_BYTES)
    }

    fn push_ref(&mut self, name: &[u8]) -> Result<()> {
        let name = String::from_utf8_lossy(name);
        let token = decode_entity_token(&format!("&{name};"))?;
        append_capped(&mut self.text_buf, &token, MAX_ITEM_TEXT_BYTES)
    }

    fn push_cdata(&mut self, text: String) -> Result<()> {
        append_capped(&mut self.text_buf, &text, MAX_ITEM_TEXT_BYTES)
    }

    fn flush_text(&mut self) -> Result<()> {
        if self.text_buf.is_empty() {
            return Ok(());
        }
        let text = std::mem::take(&mut self.text_buf);
        self.handle_text(text)
    }

    fn handle_text(&mut self, text: String) -> Result<()> {
        if text.is_empty() {
            return Ok(());
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
            let existing = self.current.calendar_data.get_or_insert_with(String::new);
            return append_capped(existing, &text, MAX_ITEM_TEXT_BYTES);
        }
        if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::AddressData,
        ]) {
            let existing = self.current.address_data.get_or_insert_with(String::new);
            return append_capped(existing, &text, MAX_ITEM_TEXT_BYTES);
        }

        // calendar-timezone can also contain multi-line iCalendar content; preserve it.
        if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarTimezone,
        ]) {
            let existing = self
                .current
                .calendar_timezone
                .get_or_insert_with(String::new);
            return append_capped(existing, &text, MAX_ITEM_TEXT_BYTES);
        }

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
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
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::MaxResourceSize,
        ]) {
            // Malformed or overflowing values are skipped (stay `None`).
            self.current.max_resource_size = trimmed.parse::<u64>().ok();
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::MaxAttendeesPerInstance,
        ]) {
            self.current.max_attendees_per_instance = trimmed.parse::<u32>().ok();
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
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::ScheduleInboxUrl,
            ElementName::Href,
        ]) {
            self.current.schedule_inbox = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::ScheduleOutboxUrl,
            ElementName::Href,
        ]) {
            self.current.schedule_outbox = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarUserAddressSet,
            ElementName::Href,
        ]) {
            self.current
                .calendar_user_addresses
                .push(trimmed.to_string());
        } else if self.path_ends_with(&[ElementName::ManagedId]) {
            // `managed-id` elements live inside the `managed-ids` property
            // (CalendarServer extension; RFC 8607 §4.3 defines MANAGED-ID
            // only as an iCalendar ATTACH parameter); a suffix match on the
            // element name keeps this independent of the parent nesting.
            self.current.managed_ids.push(trimmed.to_string());
        }
        Ok(())
    }
}

/// Yield every parsed piece of a multistatus body as it completes, as a
/// [`futures::Stream`].
///
/// The XML is read and decompressed **incrementally** from `resp_body`
/// (br/gzip/zstd negotiated by the caller's request), so memory stays bounded
/// by the current item plus the reader buffers regardless of collection size.
/// Each complete `<D:response>` becomes a [`DavStreamEvent::Item`]; the
/// `<D:sync-token>` (when present) becomes a [`DavStreamEvent::SyncToken`]
/// emitted in document order. Errors (transport, timeout, XML) yield one
/// [`Result::Err`] and end the stream.
///
/// Reads are bounded by the default idle timeout ([`STREAM_READ_IDLE_TIMEOUT`]);
/// use [`multistatus_events_with_timeout`] to customize it. **Dropping the
/// stream aborts the download**: the underlying response body is dropped
/// before completion, which also frees (rather than re-pools) the connection.
///
/// # Example
/// ```no_run
/// use fast_dav_rs::webdav::streaming::{DavStreamEvent, multistatus_events};
/// use futures::StreamExt;
/// use hyper::body::Incoming;
///
/// # async fn example(body: Incoming) -> fast_dav_rs::Result<()> {
/// let mut stream = multistatus_events(body, &[]);
/// while let Some(event) = stream.next().await {
///     match event? {
///         DavStreamEvent::Item(item) => println!("item: {}", item.href),
///         DavStreamEvent::SyncToken(token) => println!("token: {token}"),
///         other => println!("other: {other:?}"),
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub fn multistatus_events(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
) -> ItemStream<DavStreamEvent> {
    multistatus_events_with_timeout(resp_body, encodings, STREAM_READ_IDLE_TIMEOUT)
}

/// Variant of [`multistatus_events`] with a caller-provided **idle** timeout.
///
/// `idle_timeout` is the maximum time allowed between two reads making progress
/// (i.e. waiting for the next XML event to arrive from the network). It is **not**
/// a cap on the total stream duration, so huge-but-flowing responses are unaffected.
/// When the timeout elapses, the stream yields a [`Error::Timeout`](crate::Error)
/// and ends.
pub fn multistatus_events_with_timeout(
    resp_body: Incoming,
    encodings: &[ContentEncoding],
    idle_timeout: Duration,
) -> ItemStream<DavStreamEvent> {
    struct State {
        xml: Reader<Box<dyn AsyncBufRead + Unpin + Send>>,
        buf: Vec<u8>,
        parser: MultistatusParser<EventQueue>,
        idle_timeout: Duration,
        done: bool,
    }

    let state = State {
        xml: Reader::from_reader(stack_decoders(body_stream_reader(resp_body), encodings)),
        buf: Vec::with_capacity(8 * 1024),
        parser: MultistatusParser::new(EventQueue::default()),
        idle_timeout,
        done: false,
    };

    ItemStream::new(futures::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.parser.sink.events.pop_front() {
                return Some((Ok(event), state));
            }
            if state.done {
                return None;
            }
            match tokio::time::timeout(
                state.idle_timeout,
                state.xml.read_event_into_async(&mut state.buf),
            )
            .await
            {
                Err(_) => {
                    state.done = true;
                    return Some((
                        Err(Error::Timeout {
                            limit: state.idle_timeout,
                        }),
                        state,
                    ));
                }
                Ok(Err(error)) => {
                    state.done = true;
                    return Some((Err(Error::from_quick_xml(error)), state));
                }
                Ok(Ok(event)) => {
                    let is_eof =
                        match dispatch_event(&mut state.parser, state.xml.decoder(), Ok(event)) {
                            Ok(done) => done,
                            Err(error) => {
                                state.done = true;
                                return Some((Err(error), state));
                            }
                        };
                    state.buf.clear();
                    if let Some(token) = state.parser.sync_token.take() {
                        state
                            .parser
                            .sink
                            .events
                            .push_back(DavStreamEvent::SyncToken(token));
                    }
                    if is_eof {
                        state.done = true;
                        if let Some(unclosed) = state.parser.stack.last() {
                            return Some((
                                Err(Error::XmlStructure(format!(
                                    "unexpected end of input with unclosed element {unclosed:?}"
                                ))),
                                state,
                            ));
                        }
                    }
                }
            }
        }
    }))
}

/// Parse a WebDAV `207 Multi-Status` XML body in **streaming mode**, with optional
/// decompression (br, gzip, zstd).
///
/// This function avoids loading the entire response into memory, making it suitable
/// for very large CalDAV/WebDAV collections. It collects
/// [`multistatus_events`] into a [`Vec`]; for true item-by-item consumption,
/// use [`multistatus_events`] directly.
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
    let mut items = Vec::new();
    let mut sync_token = None;
    let stream = multistatus_events_with_timeout(resp_body, encodings, idle_timeout);
    futures::pin_mut!(stream);
    while let Some(event) = stream.next().await {
        match event? {
            DavStreamEvent::Item(item) => items.push(item),
            DavStreamEvent::SyncToken(token) => sync_token = Some(token),
        }
    }
    Ok(ParseResult { items, sync_token })
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
    mut on_item: F,
) -> Result<Option<String>>
where
    F: FnMut(DavItem) -> Result<()> + Send,
{
    let mut sync_token = None;
    let stream = multistatus_events_with_timeout(resp_body, encodings, idle_timeout);
    futures::pin_mut!(stream);
    while let Some(event) = stream.next().await {
        match event? {
            DavStreamEvent::Item(item) => on_item(item)?,
            DavStreamEvent::SyncToken(token) => sync_token = Some(token),
        }
    }
    Ok(sync_token)
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

/// Decode raw XML text content, resolving entity references.
///
/// Predefined entities (`&amp;`, `&lt;`, `&gt;`, `&apos;`, `&quot;`) and
/// numeric character references are always resolved. Named entities outside
/// that predefined set (e.g. `&nbsp;` emitted by lenient servers) are kept as
/// literal text instead of failing the whole parse. Malformed numeric
/// character references still return an error.
pub fn decode_text(raw: &[u8]) -> Result<String> {
    match std::str::from_utf8(raw) {
        Ok(s) => match unescape(s) {
            Ok(text) => Ok(text.into_owned()),
            Err(EscapeError::UnrecognizedEntity(..)) => unescape_lenient(s),
            Err(error) => Err(error.into()),
        },
        Err(_) => Ok(String::from_utf8_lossy(raw).into_owned()),
    }
}

/// Decode a single entity reference token of the form `&name;`.
///
/// Mirrors [`decode_text`]: predefined entities and numeric character
/// references resolve, unknown named entities pass through as literal text,
/// malformed numeric references error.
fn decode_entity_token(token: &str) -> Result<String> {
    match unescape(token) {
        Ok(text) => Ok(text.into_owned()),
        Err(EscapeError::UnrecognizedEntity(..)) => Ok(token.to_string()),
        Err(error) => Err(error.into()),
    }
}

/// Lenient second pass used when [`unescape`] hit an entity it does not know.
///
/// Resolves predefined and numeric character references entity-by-entity and
/// copies unrecognized named entities through verbatim. Unterminated `&`
/// sequences and malformed numeric references keep failing with the same
/// [`EscapeError`] the strict pass produced.
fn unescape_lenient(s: &str) -> Result<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        match tail.find(';') {
            Some(end) => {
                let token = &tail[..end + 1];
                match unescape(token) {
                    Ok(decoded) => out.push_str(&decoded),
                    Err(EscapeError::UnrecognizedEntity(..)) => out.push_str(token),
                    Err(error) => return Err(error.into()),
                }
                rest = &tail[end + 1..];
            }
            None => return Err(EscapeError::UnterminatedEntity(amp..tail.len()).into()),
        }
    }
    out.push_str(rest);
    Ok(out)
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

/// Parse a `Timeout` value (RFC 4918 §10.7): `Second-N` (a comma-separated
/// list is tolerated, the first `Second-N` wins). `Infinite` or anything
/// unparseable yields `None`.
pub(crate) fn parse_lock_timeout(value: &str) -> Option<u64> {
    value.split(',').find_map(|part| {
        let part = part.trim();
        let seconds = part
            .strip_prefix("Second-")
            .or_else(|| part.strip_prefix("second-"))?;
        seconds.parse().ok()
    })
}

/// Parse a `<D:depth>` value (RFC 4918 §14.3): `"0"`, `"1"`, or `"infinity"`.
/// Anything else (including absent) yields `None`.
fn parse_lock_depth(value: &str) -> Option<Depth> {
    match value {
        "0" => Some(Depth::Zero),
        "1" => Some(Depth::One),
        "infinity" => Some(Depth::Infinity),
        _ => None,
    }
}

/// Parse the first `<D:activelock>` of a `<D:lockdiscovery>` body (RFC 4918
/// §14.1) into a [`LockInfo`].
///
/// Lenient by design: missing `<D:locktoken>`, `<D:timeout>`,
/// `<D:lockscope>`, `<D:owner>`, `<D:lockroot>`, and `<D:depth>` elements
/// leave the corresponding [`LockInfo`] fields empty or `None` rather than
/// failing. A body without any `<D:activelock>` (or an empty body) yields a
/// default (empty) [`LockInfo`]. Useful on `LOCK` responses and on `PROPFIND`
/// responses that request the `lockdiscovery` property (RFC 4918 §8.10.9).
///
/// ```
/// use fast_dav_rs::webdav::{Depth, LockScope, parse_lock_discovery_bytes};
///
/// let xml = br#"<D:prop xmlns:D="DAV:">
///   <D:lockdiscovery>
///     <D:activelock>
///       <D:lockscope><D:exclusive/></D:lockscope>
///       <D:owner><D:href>https://example.com/alice</D:href></D:owner>
///       <D:timeout>Second-300</D:timeout>
///       <D:locktoken><D:href>opaquelocktoken:abc</D:href></D:locktoken>
///       <D:lockroot><D:href>https://example.com/docs/plan.txt</D:href></D:lockroot>
///       <D:depth>0</D:depth>
///     </D:activelock>
///   </D:lockdiscovery>
/// </D:prop>"#;
/// let lock = parse_lock_discovery_bytes(xml).unwrap();
/// assert_eq!(lock.token, "opaquelocktoken:abc");
/// assert_eq!(lock.timeout_secs, Some(300));
/// assert_eq!(lock.scope, Some(LockScope::Exclusive));
/// assert_eq!(lock.owner.as_deref(), Some("https://example.com/alice"));
/// assert_eq!(
///     lock.lockroot.as_deref(),
///     Some("https://example.com/docs/plan.txt")
/// );
/// assert_eq!(lock.depth, Some(Depth::Zero));
/// ```
pub fn parse_lock_discovery_bytes(body: &[u8]) -> Result<LockInfo> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    use std::io::Cursor;

    let trimmed = body.trim_ascii();
    if trimmed.is_empty() {
        return Ok(LockInfo::default());
    }
    let cursor = Cursor::new(trimmed);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(false);

    let mut info = LockInfo::default();
    let mut buf = Vec::with_capacity(4 * 1024);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut text = String::new();

    let in_activelock = |stack: &[Vec<u8>]| {
        stack
            .iter()
            .any(|name| name.eq_ignore_ascii_case(b"activelock"))
    };

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref()).to_vec();
                if in_activelock(&stack) {
                    if local.eq_ignore_ascii_case(b"exclusive") {
                        info.scope = Some(LockScope::Exclusive);
                    } else if local.eq_ignore_ascii_case(b"shared") {
                        info.scope = Some(LockScope::Shared);
                    }
                }
                stack.push(local);
                text.clear();
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref()).to_vec();
                if in_activelock(&stack) {
                    if local.eq_ignore_ascii_case(b"exclusive") {
                        info.scope = Some(LockScope::Exclusive);
                    } else if local.eq_ignore_ascii_case(b"shared") {
                        info.scope = Some(LockScope::Shared);
                    }
                }
            }
            Ok(Event::Text(e)) => text.push_str(&decode_text(e.as_ref())?),
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref()).to_vec();
                if local.eq_ignore_ascii_case(b"activelock") {
                    break; // first activelock wins
                }
                if in_activelock(&stack) {
                    if local.eq_ignore_ascii_case(b"timeout") {
                        info.timeout_secs = parse_lock_timeout(text.trim());
                    } else if local.eq_ignore_ascii_case(b"depth") {
                        info.depth = parse_lock_depth(text.trim());
                    } else if local.eq_ignore_ascii_case(b"href") {
                        if stack
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(b"locktoken"))
                        {
                            info.token = text.trim().to_string();
                        } else if stack.iter().any(|name| name.eq_ignore_ascii_case(b"owner")) {
                            let owner = text.trim();
                            info.owner = if owner.is_empty() {
                                None
                            } else {
                                Some(owner.to_string())
                            };
                        } else if stack
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(b"lockroot"))
                        {
                            let lockroot = text.trim();
                            info.lockroot = if lockroot.is_empty() {
                                None
                            } else {
                                Some(lockroot.to_string())
                            };
                        }
                    } else if local.eq_ignore_ascii_case(b"locktoken") && info.token.is_empty() {
                        info.token = text.trim().to_string();
                    } else if local.eq_ignore_ascii_case(b"owner")
                        && info.owner.is_none()
                        && !text.trim().is_empty()
                    {
                        info.owner = Some(text.trim().to_string());
                    }
                }
                stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(Error::from_quick_xml(error)),
            _ => {}
        }
        buf.clear();
    }

    Ok(info)
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
/// A malformed (unparsable) error body is reported via
/// [`WebDavError::parse_failed`] (`true`): a hostile server cannot silently
/// suppress precondition diagnostics by sending garbage markup.
///
/// ```
/// use fast_dav_rs::webdav::parse_error_body;
///
/// let xml = br#"<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
///   <C:no-uid-conflict/>
/// </D:error>"#;
/// let err = parse_error_body(xml).unwrap();
/// assert_eq!(err.precondition_code.as_deref(), Some("no-uid-conflict"));
/// assert!(!err.parse_failed);
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
    // Namespace declarations in scope on `<D:error>`; precondition codes are
    // the DAV:-namespaced child elements (RFC 4918 §16). Servers interleave
    // vendor extension elements (e.g. SabreDAV's `<s:sabredav-version>`,
    // `<s:exception>`, `<s:message>`) before the precondition, so the first
    // child is not necessarily the code — prefer a DAV: child, fall back to
    // the first child of any namespace.
    let mut xmlns: std::collections::HashMap<Vec<u8>, Vec<u8>> = Default::default();
    let mut default_ns: Option<Vec<u8>> = None;
    let mut first_any: Option<Vec<u8>> = None;

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_vec();
                let local = local_name(&name);
                if local.eq_ignore_ascii_case(b"error") {
                    in_error = true;
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        if key == b"xmlns" {
                            default_ns = Some(attr.value.into_owned());
                        } else if let Some(prefix) = key.strip_prefix(b"xmlns:") {
                            xmlns.insert(prefix.to_vec(), attr.value.into_owned());
                        }
                    }
                } else if in_error && !found {
                    if first_any.is_none() {
                        first_any = Some(local.to_vec());
                    }
                    let prefix = namespace_prefix(&name);
                    let ns = if prefix.is_empty() {
                        default_ns.as_deref()
                    } else {
                        xmlns.get(prefix).map(Vec::as_slice)
                    };
                    if ns == Some(b"DAV:".as_slice()) {
                        err.precondition_code = Some(String::from_utf8_lossy(local).into_owned());
                        found = true;
                    }
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
                    return Ok(WebDavError {
                        precondition_code: None,
                        parse_failed: true,
                    });
                }
                return Err(Error::from_quick_xml(error));
            }
            _ => {}
        }
        buf.clear();
    }

    if err.precondition_code.is_none() {
        err.precondition_code = first_any.map(|c| String::from_utf8_lossy(&c).into_owned());
    }

    Ok(err)
}

fn local_name(raw: &[u8]) -> &[u8] {
    match raw.iter().position(|b| *b == b':') {
        Some(idx) => &raw[idx + 1..],
        None => raw,
    }
}

fn namespace_prefix(raw: &[u8]) -> &[u8] {
    match raw.iter().position(|b| *b == b':') {
        Some(idx) => &raw[..idx],
        None => &[],
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

    #[test]
    fn append_capped_rejects_text_past_limit() {
        let mut buf = String::new();
        append_capped(&mut buf, "abc", 4).unwrap();
        let err = append_capped(&mut buf, "de", 4).unwrap_err();
        assert!(matches!(err, Error::BodyTooLarge { limit: 4 }), "{err:?}");
        assert_eq!(buf, "abc", "aborted append must leave the buffer unchanged");
    }

    #[test]
    fn per_item_text_accumulation_is_capped() {
        let mut parser = MultistatusParser::new(Vec::<DavItem>::new());
        parser.stack = vec![
            ElementName::Response,
            ElementName::Propstat,
            ElementName::Prop,
            ElementName::CalendarData,
        ];
        let chunk = "x".repeat(64 * 1024);
        let chunks = MAX_ITEM_TEXT_BYTES / chunk.len() + 2;
        let mut result = Ok(());
        for _ in 0..chunks {
            result = parser.handle_text(chunk.clone());
            if result.is_err() {
                break;
            }
        }
        let err = result.expect_err("per-item cap must abort accumulation");
        assert!(
            matches!(err, Error::BodyTooLarge { limit } if limit == MAX_ITEM_TEXT_BYTES),
            "{err:?}"
        );
    }
}
