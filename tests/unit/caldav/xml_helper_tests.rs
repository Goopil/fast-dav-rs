// Tests for the public escape_xml function.
// XML injection protection tests for build_calendar_query_body are in
// src/caldav/client.rs (internal #[cfg(test)] module) since that function
// is now pub(crate).
use fast_dav_rs::client::escape_xml;

#[test]
fn test_escape_xml_comprehensive() {
    // Test all special characters together
    let input = "&<>'\"";
    let expected = "&amp;&lt;&gt;&apos;&quot;";
    assert_eq!(escape_xml(input), expected);

    // Test empty string
    assert_eq!(escape_xml(""), "");

    // Test string with no special characters
    assert_eq!(escape_xml("normal text"), "normal text");

    // Test string with only special characters
    assert_eq!(escape_xml("&<>\"'"), "&amp;&lt;&gt;&quot;&apos;");

    // Test repeated special characters
    assert_eq!(
        escape_xml("&&<<>>\"\"''"),
        "&amp;&amp;&lt;&lt;&gt;&gt;&quot;&quot;&apos;&apos;"
    );

    // Test unicode characters mixed with special characters
    assert_eq!(escape_xml("café & résumé"), "café &amp; résumé");

    // Test very long string with special characters
    let long_input = "&".repeat(1000);
    let long_expected = "&amp;".repeat(1000);
    assert_eq!(escape_xml(&long_input), long_expected);
}

#[test]
fn test_escape_xml_unicode_edge_cases() {
    // Test unicode characters that might be confused with XML entities
    assert_eq!(escape_xml("αβγ"), "αβγ"); // Greek letters
    assert_eq!(escape_xml("🙂"), "🙂"); // Emoji
    assert_eq!(escape_xml("café"), "café"); // Latin-1 supplement
    assert_eq!(escape_xml("Москва"), "Москва"); // Cyrillic
    assert_eq!(escape_xml("北京"), "北京"); // Chinese
}

#[test]
fn test_escape_xml_whitespace_preservation() {
    // Test that whitespace is preserved
    assert_eq!(escape_xml(" leading"), " leading");
    assert_eq!(escape_xml("trailing "), "trailing ");
    assert_eq!(escape_xml("  multiple  spaces  "), "  multiple  spaces  ");
    assert_eq!(escape_xml("\t\ttabs\t\t"), "\t\ttabs\t\t");
    assert_eq!(escape_xml("\n\nnewlines\n\n"), "\n\nnewlines\n\n");
}
