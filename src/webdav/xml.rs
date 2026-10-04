use crate::caldav::types::TimeRange;
use crate::webdav::types::{
    CalendarDataLimits, CalendarQueryOptions, Collation, MatchType, SyncLevel,
};
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

/// Render the RFC 4918 `<D:limit><D:nresults>N</D:nresults></D:limit>`
/// result-truncation element, shared by the `sync-collection` REPORT
/// (RFC 6578 §3.3) and the `addressbook-query` REPORT (RFC 6352 §10.6).
pub(crate) fn limit_nresults_xml(limit: u32) -> String {
    format!("<D:limit><D:nresults>{limit}</D:nresults></D:limit>")
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
        body.push_str(&limit_nresults_xml(limit));
    }
    body.push_str("</D:sync-collection>");
    body
}

/// Build a `calendar-query` REPORT body (RFC 4791 §7.8) from structured
/// options, with optional data-return limits (RFC 4791 §9.6.4).
///
/// Pure XML renderer: no validation happens here. The validating entry point
/// is [`CalDavClient::calendar_query_options`](crate::CalDavClient::calendar_query_options),
/// which rejects invalid component names, UTC date-times, and limit windows
/// before any network I/O.
///
/// The `<C:calendar-data>` element is included when `include_data` is set —
/// and implied when `expand` or `limits` is set (a server cannot expand or
/// limit data it does not return).
///
/// # Example
///
/// ```
/// use fast_dav_rs::{TimeRange, caldav::CalendarQueryOptions, webdav::CalendarDataLimits};
///
/// let options = CalendarQueryOptions::new("VEVENT")
///     .with_start("20240101T000000Z")
///     .with_end("20240201T000000Z")
///     .with_limits(
///         CalendarDataLimits::new()
///             .with_recurrence_set(TimeRange::new("20240101T000000Z").with_end("20241231T235959Z")),
///     );
/// let body = fast_dav_rs::caldav::build_calendar_query_body_with_limits(&options);
/// assert!(body.contains("<C:limit-recurrence-set start=\"20240101T000000Z\" end=\"20241231T235959Z\"/>"));
/// assert!(body.contains("<C:calendar-data><C:limit-recurrence-set"));
/// ```
pub fn build_calendar_query_body_with_limits(options: &CalendarQueryOptions) -> String {
    let mut prop = String::from("<D:prop><D:getetag/>");
    if options.include_data || options.expand.is_some() || options.limits.is_some() {
        prop.push_str(&data_element_xml_inner(
            "calendar-data",
            options
                .expand
                .as_ref()
                .map(|tr| (tr.start.as_str(), tr.end.as_deref())),
            options.limits.as_ref(),
        ));
    }
    prop.push_str("</D:prop>");

    let mut filter = format!(
        "<C:filter>\
           <C:comp-filter name=\"VCALENDAR\">\
             <C:comp-filter name=\"{}\">",
        escape_xml(&options.component)
    );
    if options.start.is_some() || options.end.is_some() {
        filter.push_str("<C:time-range");
        if let Some(s) = &options.start {
            filter.push_str(&format!(" start=\"{}\"", escape_xml(s)));
        }
        if let Some(e) = &options.end {
            filter.push_str(&format!(" end=\"{}\"", escape_xml(e)));
        }
        filter.push_str("/>");
    }
    filter.push_str("</C:comp-filter></C:comp-filter></C:filter>");

    format!(
        r#"<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">{prop}{filter}</C:calendar-query>"#
    )
}

/// Build a PROPFIND request body requesting **all** properties
/// (RFC 4918 §9.1, `<D:allprop/>`).
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::build_propfind_allprop;
///
/// assert_eq!(
///     build_propfind_allprop(),
///     "<D:propfind xmlns:D=\"DAV:\"><D:allprop/></D:propfind>"
/// );
/// ```
pub fn build_propfind_allprop() -> String {
    "<D:propfind xmlns:D=\"DAV:\"><D:allprop/></D:propfind>".to_owned()
}

/// Build a PROPFIND request body requesting property **names** only
/// (RFC 4918 §9.1, `<D:propname/>`) — the server returns the list of
/// properties defined on each resource without their values.
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::build_propfind_propname;
///
/// assert_eq!(
///     build_propfind_propname(),
///     "<D:propfind xmlns:D=\"DAV:\"><D:propname/></D:propfind>"
/// );
/// ```
pub fn build_propfind_propname() -> String {
    "<D:propfind xmlns:D=\"DAV:\"><D:propname/></D:propfind>".to_owned()
}

/// Build a PROPFIND request body requesting a specific property list
/// (RFC 4918 §9.1, `<D:prop>`).
///
/// `props` carries `(namespace, local-name)` pairs. Namespace declarations
/// are grouped on the `<D:propfind>` root: `DAV:` uses the conventional
/// `D:` prefix, any other namespace gets a sequential `ns1`, `ns2`, … prefix
/// bound once on first appearance. Namespace URIs and local names are
/// escaped so untrusted values cannot inject markup. An empty namespace
/// renders an unprefixed child (a property in no namespace).
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::build_propfind_props;
///
/// let body = build_propfind_props(&[
///     ("DAV:", "displayname"),
///     ("urn:ietf:params:xml:ns:caldav", "calendar-description"),
/// ]);
/// assert!(body.starts_with(
///     "<D:propfind xmlns:D=\"DAV:\" xmlns:ns1=\"urn:ietf:params:xml:ns:caldav\">"
/// ));
/// assert!(body.contains("<D:displayname/>"));
/// assert!(body.contains("<ns1:calendar-description/>"));
/// ```
pub fn build_propfind_props(props: &[(&str, &str)]) -> String {
    // Namespace-prefix bindings in first-appearance order; `DAV:` is bound
    // to the conventional `D` prefix up front, further namespaces get `ns1`,
    // `ns2`, … and are declared once on the root element.
    let mut bindings: Vec<(&str, String)> = vec![("DAV:", "D".to_owned())];
    let mut declarations = String::from("xmlns:D=\"DAV:\"");
    let mut children = String::with_capacity(props.len() * 16);
    for (namespace, name) in props {
        let prefix = match bindings.iter().position(|(ns, _)| ns == namespace) {
            Some(index) => bindings[index].1.as_str(),
            None => {
                let prefix = if namespace.is_empty() {
                    String::new()
                } else {
                    format!("ns{}", bindings.len())
                };
                if !namespace.is_empty() {
                    declarations
                        .push_str(&format!(" xmlns:{prefix}=\"{}\"", escape_xml(namespace)));
                }
                bindings.push((namespace, prefix));
                bindings.last().unwrap().1.as_str()
            }
        };
        if prefix.is_empty() {
            children.push_str(&format!("<{}/>", escape_xml(name)));
        } else {
            children.push_str(&format!("<{prefix}:{}/>", escape_xml(name)));
        }
    }
    format!("<D:propfind {declarations}><D:prop>{children}</D:prop></D:propfind>")
}

/// Typed properties for a MKCALENDAR request body (RFC 4791 §9.5),
/// rendered by [`build_mkcalendar_body`] and usable with the existing
/// [`CalDavClient::mkcalendar`](crate::CalDavClient::mkcalendar) /
/// [`WebDavClient::mkcol`](crate::WebDavClient::mkcol) methods.
///
/// Build with [`MkCalendarProps::new`] plus the `with_*` constructors:
///
/// ```
/// use fast_dav_rs::webdav::MkCalendarProps;
///
/// let props = MkCalendarProps::new()
///     .with_displayname("Work")
///     .with_description("Work events")
///     .with_supported_components(["VEVENT", "VTODO"]);
/// assert_eq!(props.supported_components, ["VEVENT", "VTODO"]);
/// ```
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct MkCalendarProps {
    /// `displayname` (RFC 4918 §5.2) of the created collection.
    pub displayname: Option<String>,
    /// `calendar-description` (RFC 4791 §5.2.1) of the created calendar.
    pub description: Option<String>,
    /// Component names advertised in
    /// `supported-calendar-component-set` (RFC 4791 §5.2.3), e.g.
    /// `VEVENT`, `VTODO`. Validated (ASCII alphanumeric + `-`, non-empty)
    /// by [`build_mkcalendar_body`]; an empty list omits the element
    /// entirely (the server default applies).
    pub supported_components: Vec<String>,
}

impl MkCalendarProps {
    /// Create empty properties (the minimal §9.5 skeleton is rendered).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the `displayname` of the created collection.
    pub fn with_displayname(mut self, displayname: impl Into<String>) -> Self {
        self.displayname = Some(displayname.into());
        self
    }

    /// Set the `calendar-description` of the created calendar.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the advertised component names (replaces any previous list).
    pub fn with_supported_components<I, S>(mut self, components: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.supported_components = components.into_iter().map(Into::into).collect();
        self
    }
}

/// Build a MKCALENDAR request body (RFC 4791 §9.5):
/// `<C:mkcalendar><D:set><D:prop>…</D:prop></D:set></C:mkcalendar>` with the
/// [`MkCalendarProps`] as `<D:prop>` children — `displayname`
/// (`<D:displayname>`), `description` (`<C:calendar-description>`) and
/// `supported_components` (`<C:supported-calendar-component-set>` with one
/// `<C:comp name="…"/>` per name).
///
/// `displayname` and `description` are escaped; component names are
/// validated (ASCII alphanumeric + `-`, non-empty) so untrusted values
/// cannot alter the request structure. An empty property set renders the
/// minimal skeleton (empty `<D:prop>`).
///
/// # Errors
///
/// Returns [`Error::InvalidComponentName`](crate::Error::InvalidComponentName)
/// when a supported-component name is empty or contains a character outside
/// ASCII alphanumerics and `-`.
///
/// # Example
///
/// ```
/// use fast_dav_rs::webdav::{MkCalendarProps, build_mkcalendar_body};
///
/// let props = MkCalendarProps::new()
///     .with_displayname("Work")
///     .with_supported_components(["VEVENT"]);
/// let body = build_mkcalendar_body(&props)?;
/// assert!(body.starts_with(
///     "<C:mkcalendar xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">"
/// ));
/// assert!(body.contains("<D:displayname>Work</D:displayname>"));
/// assert!(body.contains("<C:comp name=\"VEVENT\"/>"));
/// # Ok::<(), fast_dav_rs::Error>(())
/// ```
pub fn build_mkcalendar_body(props: &MkCalendarProps) -> Result<String> {
    let mut body = String::from(
        "<C:mkcalendar xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\
<D:set><D:prop>",
    );
    if let Some(displayname) = &props.displayname {
        body.push_str(&format!(
            "<D:displayname>{}</D:displayname>",
            escape_xml(displayname)
        ));
    }
    if let Some(description) = &props.description {
        body.push_str(&format!(
            "<C:calendar-description>{}</C:calendar-description>",
            escape_xml(description)
        ));
    }
    if !props.supported_components.is_empty() {
        body.push_str("<C:supported-calendar-component-set>");
        for name in &props.supported_components {
            validate_component_name(
                name,
                "invalid mkcalendar supported-calendar-component-set component",
            )?;
            body.push_str(&format!("<C:comp name=\"{}\"/>", escape_xml(name)));
        }
        body.push_str("</C:supported-calendar-component-set>");
    }
    body.push_str("</D:prop></D:set></C:mkcalendar>");
    Ok(body)
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
