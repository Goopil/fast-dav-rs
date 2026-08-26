use crate::webdav::client::{normalize_etag, normalize_sync_token};
use crate::webdav::types::DavItemCommon;
use crate::{Error, Result};
use quick_xml::escape::unescape;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommonElement {
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
    SyncToken,
    CurrentUserPrincipal,
    Owner,
    Getcontenttype,
    Getlastmodified,
    Other,
}

pub(crate) fn common_element_from_bytes(raw: &[u8]) -> CommonElement {
    let local = match raw.iter().position(|b| *b == b':') {
        Some(idx) => &raw[idx + 1..],
        None => raw,
    };

    if local.eq_ignore_ascii_case(b"multistatus") {
        CommonElement::Multistatus
    } else if local.eq_ignore_ascii_case(b"response") {
        CommonElement::Response
    } else if local.eq_ignore_ascii_case(b"propstat") {
        CommonElement::Propstat
    } else if local.eq_ignore_ascii_case(b"prop") {
        CommonElement::Prop
    } else if local.eq_ignore_ascii_case(b"href") {
        CommonElement::Href
    } else if local.eq_ignore_ascii_case(b"status") {
        CommonElement::Status
    } else if local.eq_ignore_ascii_case(b"displayname") {
        CommonElement::Displayname
    } else if local.eq_ignore_ascii_case(b"getetag") {
        CommonElement::Getetag
    } else if local.eq_ignore_ascii_case(b"resourcetype") {
        CommonElement::Resourcetype
    } else if local.eq_ignore_ascii_case(b"collection") {
        CommonElement::Collection
    } else if local.eq_ignore_ascii_case(b"sync-token") {
        CommonElement::SyncToken
    } else if local.eq_ignore_ascii_case(b"current-user-principal") {
        CommonElement::CurrentUserPrincipal
    } else if local.eq_ignore_ascii_case(b"owner") {
        CommonElement::Owner
    } else if local.eq_ignore_ascii_case(b"getcontenttype") {
        CommonElement::Getcontenttype
    } else if local.eq_ignore_ascii_case(b"getlastmodified") {
        CommonElement::Getlastmodified
    } else {
        CommonElement::Other
    }
}

pub(crate) struct CommonParser {
    stack: Vec<CommonElement>,
    current: DavItemCommon,
}

pub(crate) fn path_ends_with<T: PartialEq>(stack: &[T], needle: &[T]) -> bool {
    stack.len() >= needle.len() && stack[stack.len() - needle.len()..] == needle[..]
}

impl CommonParser {
    pub(crate) fn new() -> Self {
        Self {
            stack: Vec::with_capacity(16),
            current: DavItemCommon::default(),
        }
    }

    pub(crate) fn on_start(&mut self, raw: &[u8]) {
        let element = common_element_from_bytes(raw);
        self.stack.push(element);

        match element {
            CommonElement::Response => {
                self.current = DavItemCommon::default();
            }
            CommonElement::Collection
                if self.path_ends_with(&[
                    CommonElement::Response,
                    CommonElement::Propstat,
                    CommonElement::Prop,
                    CommonElement::Resourcetype,
                    CommonElement::Collection,
                ]) =>
            {
                self.current.is_collection = true;
            }
            _ => {}
        }
    }

    pub(crate) fn on_end(&mut self, raw: &[u8]) -> Result<()> {
        let element = common_element_from_bytes(raw);
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

        if self.path_ends_with(&[CommonElement::Response, CommonElement::Href]) {
            self.current.href = trimmed.to_string();
        } else if self.path_ends_with(&[CommonElement::Response, CommonElement::Status])
            || self.path_ends_with(&[
                CommonElement::Response,
                CommonElement::Propstat,
                CommonElement::Status,
            ])
        {
            self.current.status = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::Displayname,
        ]) {
            self.current.displayname = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::Getetag,
        ]) {
            self.current.etag = Some(normalize_etag(trimmed));
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::SyncToken,
        ]) {
            self.current.sync_token = Some(normalize_sync_token(trimmed));
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::CurrentUserPrincipal,
            CommonElement::Href,
        ]) {
            self.current
                .current_user_principal
                .push(trimmed.to_string());
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::Owner,
            CommonElement::Href,
        ]) {
            self.current.owner = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::Getcontenttype,
        ]) {
            self.current.content_type = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::Getlastmodified,
        ]) {
            self.current.last_modified = Some(trimmed.to_string());
        }
    }

    pub(crate) fn finish_response(&mut self) -> DavItemCommon {
        std::mem::take(&mut self.current)
    }

    fn path_ends_with(&self, needle: &[CommonElement]) -> bool {
        path_ends_with(&self.stack, needle)
    }
}

#[macro_export]
macro_rules! impl_multistatus_on_end {
    ($self:ident, $name:expr, $elem:ty) => {{
        $self.common.on_end($name)?;
        if let Some(popped) = $self.stack.pop() {
            if popped == <$elem>::Response {
                let common = $self.common.finish_response();
                $self.current.apply_common(common);
                let finished = std::mem::take(&mut $self.current);
                $self.sink.consume(finished)?;
            }
        }
    }};
}

pub(crate) fn decode_text(raw: &[u8]) -> Result<String> {
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
                let local = match name.as_ref().iter().position(|b| *b == b':') {
                    Some(idx) => &name.as_ref()[idx + 1..],
                    None => name.as_ref(),
                };
                if local.eq_ignore_ascii_case(b"response") {
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

    #[test]
    fn parse_current_user_principal_bytes_valid() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href>/principals/user/</D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let result = parse_current_user_principal_bytes(xml).unwrap();
        assert_eq!(result.as_deref(), Some("/principals/user/"));
    }

    #[test]
    fn parse_current_user_principal_bytes_no_principal() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>My Cal</D:displayname>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let result = parse_current_user_principal_bytes(xml).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_current_user_principal_bytes_empty_href_skipped() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href></D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let result = parse_current_user_principal_bytes(xml).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_current_user_principal_bytes_multi_response_picks_second() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href></D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/other/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href>/principals/second/</D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let result = parse_current_user_principal_bytes(xml).unwrap();
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
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href>/principals/first/</D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/other/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href>/principals/second/</D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let result = parse_current_user_principal_bytes(xml).unwrap();
        assert_eq!(result.as_deref(), Some("/principals/first/"));
    }
}
