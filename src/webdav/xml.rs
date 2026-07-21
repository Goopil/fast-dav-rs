use anyhow::{Result, anyhow};

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
pub(crate) fn validate_component_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("component name must not be empty"));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
    {
        return Err(anyhow!(
            "component name {name:?} contains invalid character {bad:?}: \
             only ASCII letters, digits and '-' are allowed (e.g. VEVENT, X-CUSTOM)"
        ));
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
pub(crate) fn validate_utc_datetime(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let structurally_valid = bytes.len() == 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'Z';
    if !structurally_valid {
        return Err(anyhow!(
            "invalid UTC date-time {value:?}: expected iCalendar format \
             YYYYMMDDTHHMMSSZ (e.g. 20240101T000000Z)"
        ));
    }
    Ok(())
}

pub fn build_sync_collection_body(
    sync_token: Option<&str>,
    limit: Option<u32>,
    include_data: bool,
    namespace: &str,
    data_element: &str,
) -> String {
    let mut body = format!(r#"<D:sync-collection xmlns:D="DAV:" xmlns:C="{namespace}">"#);
    if let Some(token) = sync_token {
        body.push_str("<D:sync-token>");
        body.push_str(&escape_xml(token));
        body.push_str("</D:sync-token>");
    } else {
        body.push_str("<D:sync-token/>");
    }
    body.push_str("<D:sync-level>1</D:sync-level>");
    body.push_str("<D:prop><D:getetag/>");
    if include_data {
        body.push_str("<C:");
        body.push_str(data_element);
        body.push_str("/>");
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
