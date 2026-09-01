//! Structural iCalendar validation for CalDAV `PUT` bodies (RFC 5545).
//!
//! The checks are a deliberately cheap, dependency-free line scan over the
//! unfolded raw text: they catch the common breakage modes (truncated
//! bodies, missing required properties, unbalanced components) before any
//! bytes hit the wire. Full RFC 5545 parsing (line folding, `VTIMEZONE`,
//! iTIP/`METHOD` semantics) is out of scope.

use crate::error::ICalendarViolation;
use crate::{Error, Result};

/// How strictly CalDAV `PUT` bodies are validated client-side before they
/// are sent.
///
/// The default is [`Structural`](Self::Structural): a structurally invalid
/// body fails with [`Error::InvalidICalendar`] **before any network I/O**.
/// Use [`None`](Self::None) to restore the pre-validation behavior.
///
/// # Example
///
/// ```
/// use fast_dav_rs::caldav::ValidationLevel;
/// use fast_dav_rs::CalDavClient;
///
/// let client = CalDavClient::builder("https://cal.example.com/dav/")
///     .validation_level(ValidationLevel::Strict) // also require UID in VEVENT/VTODO
///     .build()?;
/// # Ok::<(), fast_dav_rs::Error>(())
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationLevel {
    /// Send bodies unvalidated (the behavior before this feature existed).
    None,
    /// Structural checks: valid UTF-8, a `BEGIN:VCALENDAR`/`END:VCALENDAR`
    /// envelope, a `VERSION:2.0` property, a `PRODID` property, and balanced
    /// `BEGIN`/`END` component pairs. This is the default.
    #[default]
    Structural,
    /// The structural checks plus: every `VEVENT`/`VTODO` component must
    /// carry a `UID` property.
    Strict,
}

/// Validate an iCalendar body, running **all** structural checks (the
/// equivalent of [`ValidationLevel::Strict`]).
///
/// The seven checks, reported in this order:
///
/// 1. the body is valid UTF-8;
/// 2. it starts with a `BEGIN:VCALENDAR` line;
/// 3. it ends with an `END:VCALENDAR` line;
/// 4. it declares a `VERSION` property with value `2.0`;
/// 5. it declares a `PRODID` property (value not validated);
/// 6. every `BEGIN:x` has a matching `END:x` (case-insensitive names);
/// 7. every `VEVENT`/`VTODO` component carries a `UID` property.
///
/// Parsing is a line-based scan on the unfolded raw text (split on CRLF/LF);
/// property and component names are matched case-insensitively per
/// RFC 5545. No full iCalendar parsing is performed.
///
/// # Errors
///
/// Returns [`Error::InvalidICalendar`] with the first violated check.
///
/// # Example
///
/// ```
/// use fast_dav_rs::caldav::{ICalendarViolation, validate_icalendar};
/// use fast_dav_rs::Error;
///
/// let ok = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//t//EN\r\n\
///            BEGIN:VEVENT\r\nUID:1@example\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
/// assert!(validate_icalendar(ok).is_ok());
///
/// let bad = b"BEGIN:VCALENDAR\r\nVERSION:3.0\r\nPRODID:-//t//EN\r\nEND:VCALENDAR\r\n";
/// assert!(matches!(
///     validate_icalendar(bad),
///     Err(Error::InvalidICalendar {
///         violation: ICalendarViolation::UnsupportedVersion,
///         ..
///     })
/// ));
/// ```
pub fn validate_icalendar(data: &[u8]) -> Result<()> {
    validate_icalendar_level(data, ValidationLevel::Strict).map(|_| ())
}

/// Level-gated validation behind the CalDAV `PUT` methods.
///
/// `level` must not be [`ValidationLevel::None`] (callers short-circuit
/// before reaching here). On success returns the `VERSION` value declared by
/// the body — guaranteed `2.0` after a passing
/// [`Structural`](ValidationLevel::Structural) or
/// [`Strict`](ValidationLevel::Strict) run — for the wire `Content-Type`
/// version parameter.
pub(crate) fn validate_icalendar_level(data: &[u8], level: ValidationLevel) -> Result<&str> {
    debug_assert_ne!(level, ValidationLevel::None);

    let text = std::str::from_utf8(data).map_err(|_| Error::InvalidICalendar {
        violation: ICalendarViolation::NotUtf8,
    })?;

    // Empty lines carry no information; trimming each line makes the scan
    // CRLF/LF tolerant (`str::lines` already strips a trailing `\r`).
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    if !lines
        .first()
        .is_some_and(|first| first.eq_ignore_ascii_case("BEGIN:VCALENDAR"))
    {
        return Err(Error::InvalidICalendar {
            violation: ICalendarViolation::MissingBegin,
        });
    }
    if !lines
        .last()
        .is_some_and(|last| last.eq_ignore_ascii_case("END:VCALENDAR"))
    {
        return Err(Error::InvalidICalendar {
            violation: ICalendarViolation::MissingEnd,
        });
    }

    let mut version: Option<&str> = None;
    let mut prodid = false;
    let mut stack: Vec<&str> = Vec::new();
    // "UID seen" flag per open component; pushed/popped together with `stack`.
    let mut uid_seen: Vec<bool> = Vec::new();

    for line in &lines {
        if let Some(name) = strip_prefix_ci(line, "BEGIN:") {
            stack.push(name.trim());
            uid_seen.push(false);
            continue;
        }
        if let Some(name) = strip_prefix_ci(line, "END:") {
            let name = name.trim();
            match stack.pop() {
                Some(open) if open.eq_ignore_ascii_case(name) => {
                    let seen_uid = uid_seen.pop().unwrap_or(false);
                    if level == ValidationLevel::Strict
                        && !seen_uid
                        && (name.eq_ignore_ascii_case("VEVENT")
                            || name.eq_ignore_ascii_case("VTODO"))
                    {
                        return Err(Error::InvalidICalendar {
                            violation: ICalendarViolation::MissingUid,
                        });
                    }
                }
                _ => {
                    return Err(Error::InvalidICalendar {
                        violation: ICalendarViolation::UnbalancedComponents,
                    });
                }
            }
            continue;
        }

        // Property line: name runs to the first `:` or `;` (parameters first).
        let Some(colon) = line.find(':') else {
            continue;
        };
        let name_end = line[..colon].find(';').unwrap_or(colon);
        let name = &line[..name_end];
        if name.eq_ignore_ascii_case("VERSION") {
            if version.is_none() {
                version = Some(line[colon + 1..].trim());
            }
        } else if name.eq_ignore_ascii_case("PRODID") {
            prodid = true;
        } else if name.eq_ignore_ascii_case("UID") {
            if let Some(seen) = uid_seen.last_mut() {
                *seen = true;
            }
        }
    }

    // The envelope checks (first line `BEGIN:VCALENDAR`, last line
    // `END:VCALENDAR`) plus the matching pop above guarantee a balanced scan.

    let Some(version) = version else {
        return Err(Error::InvalidICalendar {
            violation: ICalendarViolation::MissingVersion,
        });
    };
    if !version.eq_ignore_ascii_case("2.0") {
        return Err(Error::InvalidICalendar {
            violation: ICalendarViolation::UnsupportedVersion,
        });
    }
    if !prodid {
        return Err(Error::InvalidICalendar {
            violation: ICalendarViolation::MissingProdId,
        });
    }
    Ok(version)
}

/// Case-insensitive `strip_prefix`.
fn strip_prefix_ci<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&line[prefix.len()..])
    } else {
        None
    }
}
