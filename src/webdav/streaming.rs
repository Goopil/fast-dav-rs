use crate::webdav::types::DavItemCommon;

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
            CommonElement::Collection => {
                if self.path_ends_with(&[
                    CommonElement::Response,
                    CommonElement::Propstat,
                    CommonElement::Prop,
                    CommonElement::Resourcetype,
                    CommonElement::Collection,
                ]) {
                    self.current.is_collection = true;
                }
            }
            _ => {}
        }
    }

    pub(crate) fn on_end(&mut self, raw: &[u8]) {
        let element = common_element_from_bytes(raw);
        if let Some(popped) = self.stack.pop()
            && popped != element
        {
            // Ignore mismatches silently; the XML is assumed well-formed.
        }
        if element == CommonElement::Response && !self.stack.is_empty() {
            while let Some(last) = self.stack.last() {
                if *last == CommonElement::Response {
                    self.stack.pop();
                } else {
                    break;
                }
            }
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
            self.current.etag = Some(trimmed.to_string());
        } else if self.path_ends_with(&[
            CommonElement::Response,
            CommonElement::Propstat,
            CommonElement::Prop,
            CommonElement::SyncToken,
        ]) {
            self.current.sync_token = Some(trimmed.to_string());
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
