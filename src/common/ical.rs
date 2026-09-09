//! Shared iCalendar text helpers (RFC 5545 §3.1).

use std::borrow::Cow;

/// Unfold an iCalendar text into its logical content lines (RFC 5545 §3.1).
///
/// A physical line beginning with a single space or tab continues the
/// previous line: the preceding line break and that one leading whitespace
/// character are removed. Both CRLF and LF endings are handled, and a
/// leading UTF-8 BOM (`U+FEFF`) is dropped so line-based scanners see
/// `BEGIN:VCALENDAR` first. Returned lines borrow from `text` unless
/// folding joined them.
pub(crate) fn unfold_ical_lines(text: &str) -> Vec<Cow<'_, str>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Mirror `str::lines` trailing-newline behavior: split on LF (with a
    // trailing CR stripped per line), without an empty final element.
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut lines: Vec<Cow<'_, str>> = Vec::new();
    for line in body.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with([' ', '\t']) {
            if let Some(prev) = lines.last_mut() {
                prev.to_mut().push_str(&line[1..]);
                continue;
            }
        }
        lines.push(Cow::Borrowed(line));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::unfold_ical_lines;

    #[test]
    fn unfolds_folded_prodid_roundtrip() {
        let lines = unfold_ical_lines(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//X//Y\r\n Z//EN\r\nEND:VCALENDAR",
        );
        assert_eq!(
            lines,
            [
                "BEGIN:VCALENDAR",
                "VERSION:2.0",
                "PRODID:-//X//YZ//EN",
                "END:VCALENDAR"
            ]
        );
    }

    #[test]
    fn tab_continuation_is_unfolded() {
        assert_eq!(unfold_ical_lines("SUMMARY:a\r\n\tb"), ["SUMMARY:ab"]);
    }

    #[test]
    fn lf_only_input_unfolds_the_same() {
        assert_eq!(unfold_ical_lines("A:1\n B\nC:2\n"), ["A:1B", "C:2"]);
    }

    #[test]
    fn utf8_bom_is_stripped() {
        assert_eq!(
            unfold_ical_lines("\u{feff}BEGIN:VCALENDAR"),
            ["BEGIN:VCALENDAR"]
        );
    }
}
