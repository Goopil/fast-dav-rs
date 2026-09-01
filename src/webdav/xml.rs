use crate::webdav::types::SyncLevel;
use crate::{Error, Result};

pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Validate an iCalendar component name (e.g. `VEVENT`, `VTODO`, `X-CUSTOM`).
///
/// Accepts non-empty names made exclusively of ASCII alphanumeric characters
/// or `-`, matching the iCalendar component-name grammar. Anything else
/// (whitespace, quotes, XML metacharacters, non-ASCII, …) is rejected so
/// untrusted values cannot alter the structure of generated request XML.
///
/// # Errors
///
/// Returns an error when `name` is empty or contains a character outside
/// `[A-Za-z0-9-]`.
pub(crate) fn validate_component_name(name: &str, context: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::InvalidComponentName {
            context: context.to_owned(),
            name: name.to_owned(),
            reason: "component name must not be empty",
            bad_char: None,
        });
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
    {
        return Err(Error::InvalidComponentName {
            context: context.to_owned(),
            name: name.to_owned(),
            reason: "only ASCII letters, digits and '-' are allowed (e.g. VEVENT, X-CUSTOM)",
            bad_char: Some(bad),
        });
    }
    Ok(())
}

/// Validate the structure of an iCalendar UTC date-time (RFC 5545 `DATE-TIME`
/// form 2), e.g. `20240101T000000Z`.
///
/// This is a purely structural check — exactly 8 ASCII digits, a literal `T`,
/// 6 ASCII digits, and a literal `Z` — used to keep untrusted values out of
/// generated request XML. It deliberately does not validate calendar
/// semantics (month/day ranges, leap years, …).
///
/// # Errors
///
/// Returns an error when `value` does not match `YYYYMMDDTHHMMSSZ`.
pub(crate) fn validate_utc_datetime(value: &str, context: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let structurally_valid = bytes.len() == 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'Z';
    if !structurally_valid {
        return Err(Error::InvalidDateTime {
            context: context.to_owned(),
            value: value.to_owned(),
            reason: "expected iCalendar format YYYYMMDDTHHMMSSZ (e.g. 20240101T000000Z)",
        });
    }
    Ok(())
}

/// Render a CalDAV/CardDAV data element (`calendar-data` / `address-data`):
/// bare when `expand` is `None`, or wrapping an `<C:expand>` element
/// (RFC 4791 §9.6) when server-side expansion is requested.
pub(crate) fn data_element_xml(data_element: &str, expand: Option<(&str, Option<&str>)>) -> String {
    let Some((start, end)) = expand else {
        return format!("<C:{data_element}/>");
    };
    let mut out = format!(
        "<C:{data_element}><C:expand start=\"{}\"",
        escape_xml(start)
    );
    if let Some(e) = end {
        out.push_str(&format!(" end=\"{}\"", escape_xml(e)));
    }
    out.push_str("/></C:");
    out.push_str(data_element);
    out.push('>');
    out
}

/// Build a `sync-collection` REPORT body (RFC 6578 §3.3).
///
/// `sync_level` controls the `<D:sync-level>` element: [`SyncLevel::One`]
/// restricts the sync to the collection members, [`SyncLevel::Infinite`]
/// includes all descendants.
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::{SyncLevel, build_sync_collection_body};
///
/// let body = build_sync_collection_body(
///     Some("http://example.com/sync/7"),
///     None,
///     true,
///     "urn:ietf:params:xml:ns:caldav",
///     "calendar-data",
///     None,
///     SyncLevel::Infinite,
/// );
/// assert!(body.contains("<D:sync-token>http://example.com/sync/7</D:sync-token>"));
/// assert!(body.contains("<D:sync-level>infinite</D:sync-level>"));
/// ```
pub fn build_sync_collection_body(
    sync_token: Option<&str>,
    limit: Option<u32>,
    include_data: bool,
    namespace: &str,
    data_element: &str,
    expand: Option<(&str, Option<&str>)>,
    sync_level: SyncLevel,
) -> String {
    let mut body = format!(r#"<D:sync-collection xmlns:D="DAV:" xmlns:C="{namespace}">"#);
    if let Some(token) = sync_token {
        body.push_str("<D:sync-token>");
        body.push_str(&escape_xml(token));
        body.push_str("</D:sync-token>");
    } else {
        body.push_str("<D:sync-token/>");
    }
    body.push_str("<D:sync-level>");
    body.push_str(sync_level.as_str());
    body.push_str("</D:sync-level>");
    body.push_str("<D:prop><D:getetag/>");
    if include_data || expand.is_some() {
        body.push_str(&data_element_xml(data_element, expand));
    }
    body.push_str("</D:prop>");
    if let Some(limit) = limit {
        body.push_str("<D:limit><D:nresults>");
        body.push_str(&limit.to_string());
        body.push_str("</D:nresults></D:limit>");
    }
    body.push_str("</D:sync-collection>");
    body
}

pub(crate) fn text_match_xml(
    value: &str,
    collation: &str,
    match_type: &str,
    negate: bool,
) -> String {
    let mut attrs = String::new();
    attrs.push_str(&format!(" collation=\"{}\"", escape_xml(collation)));
    attrs.push_str(&format!(" match-type=\"{}\"", escape_xml(match_type)));
    if negate {
        attrs.push_str(" negate-condition=\"yes\"");
    }
    format!("<C:text-match{attrs}>{}</C:text-match>", escape_xml(value))
}

pub(crate) fn param_filter_xml(name: &str, inner: &str) -> String {
    format!(
        "<C:param-filter name=\"{}\">{inner}</C:param-filter>",
        escape_xml(name)
    )
}

pub(crate) fn prop_filter_xml(name: &str, inner: &str) -> String {
    format!(
        "<C:prop-filter name=\"{}\">{inner}</C:prop-filter>",
        escape_xml(name)
    )
}

pub(crate) fn comp_filter_xml(name: &str, inner: &str) -> String {
    format!(
        "<C:comp-filter name=\"{}\">{inner}</C:comp-filter>",
        escape_xml(name)
    )
}

pub(crate) fn time_range_xml(start: &str, end: Option<&str>) -> String {
    let mut attrs = format!(" start=\"{}\"", escape_xml(start));
    if let Some(e) = end {
        attrs.push_str(&format!(" end=\"{}\"", escape_xml(e)));
    }
    format!("<C:time-range{attrs}/>")
}

pub(crate) const IS_NOT_DEFINED_XML: &str = "<C:is-not-defined/>";

pub(crate) fn build_multiget_body<I, S>(
    hrefs: I,
    include_data: bool,
    namespace: &str,
    root_element: &str,
    data_element: &str,
    expand: Option<(&str, Option<&str>)>,
) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut href_xml = String::new();
    let mut total = 0usize;
    for href in hrefs {
        let href = href.as_ref();
        if href.is_empty() {
            continue;
        }
        total += 1;
        href_xml.push_str("<D:href>");
        href_xml.push_str(&escape_xml(href));
        href_xml.push_str("</D:href>");
    }
    if total == 0 {
        return None;
    }

    let mut body =
        format!(r#"<C:{root_element} xmlns:D="DAV:" xmlns:C="{namespace}"><D:prop><D:getetag/>"#);
    if include_data || expand.is_some() {
        body.push_str(&data_element_xml(data_element, expand));
    }
    body.push_str("</D:prop>");
    body.push_str(&href_xml);
    body.push_str("</C:");
    body.push_str(root_element);
    body.push('>');
    Some(body)
}
