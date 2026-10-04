use crate::caldav::types::TimeRange;
use crate::webdav::types::{CalendarDataLimits, Collation, MatchType, SyncLevel};
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
///
/// `data_element` is an XML element name; it is escaped so a hostile value
/// cannot inject markup (a server will reject the resulting ill-formed name).
pub(crate) fn data_element_xml(data_element: &str, expand: Option<(&str, Option<&str>)>) -> String {
    data_element_xml_inner(data_element, expand, None)
}

/// Render a CalDAV/CardDAV data element with optional data-return limits
/// (RFC 4791 §9.6.4): `<C:limit-recurrence-set start="…" end="…"/>` and/or
/// `<C:limit-freebusy-set start="…" end="…"/>`, serialized inside the data
/// element **after** `<C:expand>` (RFC 4791 §9.6 DTD order:
/// `expand?, limit-recurrence-set?, limit-freebusy-set?`).
///
/// Like [`data_element_xml`], this is a pure renderer: the values are escaped
/// so untrusted input cannot inject markup, but the DTD constraints (both
/// `start` and `end` are `#REQUIRED` on the limit elements, `end` after
/// `start`) are enforced by the validating entry point
/// [`CalDavClient::calendar_query_options`](crate::CalDavClient::calendar_query_options),
/// not here. A limit range without `end` renders with the `end` attribute
/// omitted.
///
/// # Example
///
/// ```
/// use fast_dav_rs::{TimeRange, webdav::{CalendarDataLimits, xml::data_element_xml_with_limits}};
///
/// let limits = CalendarDataLimits::new()
///     .with_recurrence_set(TimeRange::new("20240101T000000Z").with_end("20241231T235959Z"));
/// let xml = data_element_xml_with_limits(
///     "calendar-data",
///     Some(("20240101T000000Z", "20240301T000000Z")),
///     Some(&limits),
/// );
/// assert!(xml.starts_with("<C:calendar-data><C:expand"));
/// assert!(xml.contains("<C:limit-recurrence-set start=\"20240101T000000Z\" end=\"20241231T235959Z\"/>"));
/// assert!(xml.ends_with("</C:calendar-data>"));
/// ```
pub fn data_element_xml_with_limits(
    data_element: &str,
    expand: Option<(&str, &str)>,
    limits: Option<&CalendarDataLimits>,
) -> String {
    data_element_xml_inner(data_element, expand.map(|(s, e)| (s, Some(e))), limits)
}

/// Shared core behind [`data_element_xml`] and
/// [`data_element_xml_with_limits`].
fn data_element_xml_inner(
    data_element: &str,
    expand: Option<(&str, Option<&str>)>,
    limits: Option<&CalendarDataLimits>,
) -> String {
    let data_element = escape_xml(data_element);
    let mut out = match expand {
        None => {
            let has_limits =
                limits.is_some_and(|l| l.recurrence_set.is_some() || l.freebusy_set.is_some());
            if !has_limits {
                // Bare element with nothing inside: keep the historical
                // self-closing form byte-identical.
                return format!("<C:{data_element}/>");
            }
            format!("<C:{data_element}>")
        }
        Some((start, end)) => {
            let mut out = format!(
                "<C:{data_element}><C:expand start=\"{}\"",
                escape_xml(start)
            );
            if let Some(e) = end {
                out.push_str(&format!(" end=\"{}\"", escape_xml(e)));
            }
            out.push_str("/>");
            out
        }
    };
    if let Some(limits) = limits {
        if let Some(range) = &limits.recurrence_set {
            out.push_str(&limit_element_xml("limit-recurrence-set", range));
        }
        if let Some(range) = &limits.freebusy_set {
            out.push_str(&limit_element_xml("limit-freebusy-set", range));
        }
    }
    out.push_str("</C:");
    out.push_str(&data_element);
    out.push('>');
    out
}

/// Render one `limit-*` element (RFC 4791 §9.6.4) from a time-range.
fn limit_element_xml(element: &str, range: &TimeRange) -> String {
    let mut out = format!("<C:{element} start=\"{}\"", escape_xml(&range.start));
    if let Some(end) = &range.end {
        out.push_str(&format!(" end=\"{}\"", escape_xml(end)));
    }
    out.push_str("/>");
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
    let mut body = format!(
        r#"<D:sync-collection xmlns:D="DAV:" xmlns:C="{}">"#,
        escape_xml(namespace)
    );
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

/// Render a `<C:text-match>` element.
///
/// CalDAV (RFC 4791 §9.7.5) has no `match-type` attribute and defaults the
/// collation to `i;ascii-casemap` (§7.5 only requires servers to support
/// `i;ascii-casemap` and `i;octet`), so with `caldav == true` neither
/// attribute is emitted: the enum default `i;unicode-casemap` is treated as
/// unset, and sending it could be rejected by minimally conforming servers
/// with a `400 valid-collation` (the enum cannot distinguish an explicitly
/// selected default). CardDAV (RFC 6352 §10.5.4) always carries both
/// attributes.
pub(crate) fn text_match_xml(
    value: &str,
    collation: Collation,
    match_type: MatchType,
    negate: bool,
    caldav: bool,
) -> String {
    let mut attrs = String::new();
    if !caldav {
        attrs.push_str(&format!(
            " collation=\"{}\"",
            escape_xml(collation.as_str())
        ));
        attrs.push_str(&format!(
            " match-type=\"{}\"",
            escape_xml(match_type.as_str())
        ));
    }
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
    const HREF_OPEN: &str = "<D:href>";
    const HREF_CLOSE: &str = "</D:href>";

    // Buffer the hrefs so the body can be sized exactly in one allocation:
    // growing it through repeated `push_str` (and a final bulk copy of the
    // `href_xml` scratch buffer) left `capacity` up to 2× above `len`,
    // depending on whether realloc had to relocate the buffer mid-growth —
    // which made the 1k-hrefs request-body cost bimodal. Metacharacter
    // escaping can still exceed the estimate (`escape_xml` expands up to 6×
    // per char); amortized growth handles that rare case.
    let hrefs: Vec<S> = hrefs.into_iter().collect();
    let href_capacity: usize = hrefs
        .iter()
        .filter(|href| !href.as_ref().is_empty())
        .map(|href| HREF_OPEN.len() + HREF_CLOSE.len() + href.as_ref().len())
        .sum();
    if href_capacity == 0 {
        return None;
    }

    let mut prop = format!(
        r#"<C:{} xmlns:D="DAV:" xmlns:C="{}"><D:prop><D:getetag/>"#,
        escape_xml(root_element),
        escape_xml(namespace)
    );
    if include_data || expand.is_some() {
        prop.push_str(&data_element_xml(data_element, expand));
    }
    prop.push_str("</D:prop>");

    let mut body = String::with_capacity(
        prop.len() + href_capacity + "</C:".len() + root_element.len() + ">".len(),
    );
    body.push_str(&prop);
    for href in &hrefs {
        let href = href.as_ref();
        if href.is_empty() {
            continue;
        }
        body.push_str(HREF_OPEN);
        body.push_str(&escape_xml(href));
        body.push_str(HREF_CLOSE);
    }
    body.push_str("</C:");
    body.push_str(root_element);
    body.push('>');
    Some(body)
}
