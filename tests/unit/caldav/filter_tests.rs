use fast_dav_rs::caldav::types::{
    CalendarQueryFilter, Collation, MatchType, ParamFilter, PropFilter, TextMatch, TimeRange,
};

#[test]
fn collation_unicode_casemap_as_str() {
    assert_eq!(Collation::UnicodeCasemap.as_str(), "i;unicode-casemap");
}

#[test]
fn collation_ascii_casemap_as_str() {
    assert_eq!(Collation::AsciiCasemap.as_str(), "i;ascii-casemap");
}

#[test]
fn collation_default_is_unicode_casemap() {
    assert_eq!(Collation::default(), Collation::UnicodeCasemap);
}

#[test]
fn match_type_equals_as_str() {
    assert_eq!(MatchType::Equals.as_str(), "equals");
}

#[test]
fn match_type_contains_as_str() {
    assert_eq!(MatchType::Contains.as_str(), "contains");
}

#[test]
fn match_type_starts_with_as_str() {
    assert_eq!(MatchType::StartsWith.as_str(), "starts-with");
}

#[test]
fn match_type_ends_with_as_str() {
    assert_eq!(MatchType::EndsWith.as_str(), "ends-with");
}

#[test]
fn match_type_default_is_equals() {
    assert_eq!(MatchType::default(), MatchType::Equals);
}

#[test]
fn text_match_to_xml_defaults() {
    let tm = TextMatch::new("meeting");
    let xml = tm.to_xml();
    assert!(xml.contains("collation=\"i;unicode-casemap\""));
    assert!(xml.contains("match-type=\"equals\""));
    assert!(xml.contains(">meeting</C:text-match>"));
    assert!(!xml.contains("negate-condition"));
}

#[test]
fn text_match_to_xml_ascii_casemap() {
    let mut tm = TextMatch::new("meeting");
    tm.collation = Collation::AsciiCasemap;
    let xml = tm.to_xml();
    assert!(xml.contains("collation=\"i;ascii-casemap\""));
}

#[test]
fn text_match_to_xml_contains() {
    let mut tm = TextMatch::new("meeting");
    tm.match_type = MatchType::Contains;
    let xml = tm.to_xml();
    assert!(xml.contains("match-type=\"contains\""));
}

#[test]
fn text_match_to_xml_starts_with() {
    let mut tm = TextMatch::new("meet");
    tm.match_type = MatchType::StartsWith;
    let xml = tm.to_xml();
    assert!(xml.contains("match-type=\"starts-with\""));
}

#[test]
fn text_match_to_xml_ends_with() {
    let mut tm = TextMatch::new("ing");
    tm.match_type = MatchType::EndsWith;
    let xml = tm.to_xml();
    assert!(xml.contains("match-type=\"ends-with\""));
}

#[test]
fn text_match_to_xml_negate() {
    let mut tm = TextMatch::new("meeting");
    tm.negate = true;
    let xml = tm.to_xml();
    assert!(xml.contains("negate-condition=\"yes\""));
}

#[test]
fn text_match_to_xml_negate_false_omits_attribute() {
    let tm = TextMatch::new("meeting");
    let xml = tm.to_xml();
    assert!(!xml.contains("negate-condition"));
}

#[test]
fn text_match_to_xml_escapes_value() {
    let tm = TextMatch::new("Tom & Jerry <script>");
    let xml = tm.to_xml();
    assert!(xml.contains("Tom &amp; Jerry &lt;script&gt;"));
    assert!(!xml.contains("Tom & Jerry <script>"));
}

#[test]
fn text_match_to_xml_escapes_quotes_and_apostrophes() {
    let tm = TextMatch::new(r#"He said "hi" & 'bye'"#);
    let xml = tm.to_xml();
    assert!(xml.contains("&quot;"));
    assert!(xml.contains("&apos;"));
    assert!(xml.contains("&amp;"));
}

#[test]
fn text_match_new_defaults() {
    let tm = TextMatch::new("value");
    assert_eq!(tm.value, "value");
    assert_eq!(tm.collation, Collation::UnicodeCasemap);
    assert_eq!(tm.match_type, MatchType::Equals);
    assert!(!tm.negate);
}

#[test]
fn text_match_with_collation_builder() {
    let tm = TextMatch::new("value").with_collation(Collation::AsciiCasemap);
    assert_eq!(tm.collation, Collation::AsciiCasemap);
}

#[test]
fn text_match_with_match_type_builder() {
    let tm = TextMatch::new("value").with_match_type(MatchType::Contains);
    assert_eq!(tm.match_type, MatchType::Contains);
}

#[test]
fn text_match_with_negate_builder() {
    let tm = TextMatch::new("value").with_negate(true);
    assert!(tm.negate);
}

#[test]
fn param_filter_to_xml_with_text_match() {
    let pf = ParamFilter::new("PARTSTAT", TextMatch::new("ACCEPTED"));
    let xml = pf.to_xml();
    assert!(xml.contains("param-filter name=\"PARTSTAT\""));
    assert!(xml.contains(">ACCEPTED</C:text-match>"));
}

#[test]
fn param_filter_to_xml_is_not_defined() {
    let pf = ParamFilter::not_defined("PARTSTAT");
    let xml = pf.to_xml();
    assert!(xml.contains("param-filter name=\"PARTSTAT\""));
    assert!(xml.contains("<C:is-not-defined/>"));
    assert!(!xml.contains("text-match"));
}

#[test]
fn param_filter_to_xml_empty_inner_when_unset() {
    let mut pf = ParamFilter::new("PARTSTAT", TextMatch::new("x"));
    pf.text_match = None;
    let xml = pf.to_xml();
    assert!(xml.contains("param-filter name=\"PARTSTAT\""));
    assert!(!xml.contains("text-match"));
    assert!(!xml.contains("is-not-defined"));
}

#[test]
fn param_filter_to_xml_escapes_name() {
    let pf = ParamFilter::new("PARTSTAT&X", TextMatch::new("ACCEPTED"));
    let xml = pf.to_xml();
    assert!(xml.contains("PARTSTAT&amp;X"));
}

#[test]
fn param_filter_new_defaults() {
    let pf = ParamFilter::new("PARTSTAT", TextMatch::new("ACCEPTED"));
    assert_eq!(pf.name, "PARTSTAT");
    assert!(pf.text_match.is_some());
    assert!(!pf.is_not_defined);
}

#[test]
fn param_filter_not_defined_defaults() {
    let pf = ParamFilter::not_defined("PARTSTAT");
    assert_eq!(pf.name, "PARTSTAT");
    assert!(pf.text_match.is_none());
    assert!(pf.is_not_defined);
}

#[test]
fn prop_filter_to_xml_with_text_match() {
    let pf = PropFilter::new("SUMMARY", TextMatch::new("meeting"));
    let xml = pf.to_xml();
    assert!(xml.contains("prop-filter name=\"SUMMARY\""));
    assert!(xml.contains(">meeting</C:text-match>"));
}

#[test]
fn prop_filter_to_xml_with_time_range() {
    let pf = PropFilter::new("DTSTART", TextMatch::new("20240101T000000Z"))
        .with_time_range(TimeRange::new("20240101T000000Z"));
    let xml = pf.to_xml();
    assert!(xml.contains("prop-filter name=\"DTSTART\""));
    assert!(xml.contains("<C:time-range start=\"20240101T000000Z\""));
    assert!(xml.contains("text-match"));
}

#[test]
fn prop_filter_to_xml_with_param_filters() {
    let pf =
        PropFilter::new("ATTENDEE", TextMatch::new("user@example.com")).with_param_filters(vec![
            ParamFilter::new("PARTSTAT", TextMatch::new("ACCEPTED")),
        ]);
    let xml = pf.to_xml();
    assert!(xml.contains("prop-filter name=\"ATTENDEE\""));
    assert!(xml.contains("param-filter name=\"PARTSTAT\""));
    assert!(xml.contains(">ACCEPTED</C:text-match>"));
    assert!(xml.contains(">user@example.com</C:text-match>"));
}

#[test]
fn prop_filter_to_xml_is_not_defined() {
    let pf = PropFilter::not_defined("LOCATION");
    let xml = pf.to_xml();
    assert!(xml.contains("prop-filter name=\"LOCATION\""));
    assert!(xml.contains("<C:is-not-defined/>"));
    assert!(!xml.contains("text-match"));
}

#[test]
fn prop_filter_to_xml_multiple_children() {
    let pf = PropFilter::new("SUMMARY", TextMatch::new("meeting"))
        .with_time_range(TimeRange::new("20240101T000000Z").with_end("20240201T000000Z"))
        .with_param_filters(vec![
            ParamFilter::new("PARTSTAT", TextMatch::new("ACCEPTED")),
            ParamFilter::not_defined("ROLE"),
        ]);
    let xml = pf.to_xml();
    assert!(xml.contains("prop-filter name=\"SUMMARY\""));
    assert!(xml.contains("text-match"));
    assert!(xml.contains("time-range"));
    assert!(xml.contains("param-filter name=\"PARTSTAT\""));
    assert!(xml.contains("param-filter name=\"ROLE\""));
    assert!(xml.contains("<C:is-not-defined/>"));
}

#[test]
fn prop_filter_to_xml_escapes_name() {
    let pf = PropFilter::new("SUMMARY&X", TextMatch::new("meeting"));
    let xml = pf.to_xml();
    assert!(xml.contains("SUMMARY&amp;X"));
}

#[test]
fn time_range_to_xml_start_only() {
    let tr = TimeRange::new("20240101T000000Z");
    let xml = tr.to_xml();
    assert!(xml.contains("start=\"20240101T000000Z\""));
    assert!(!xml.contains("end="));
}

#[test]
fn time_range_to_xml_start_and_end() {
    let tr = TimeRange::new("20240101T000000Z").with_end("20240201T000000Z");
    let xml = tr.to_xml();
    assert!(xml.contains("start=\"20240101T000000Z\""));
    assert!(xml.contains("end=\"20240201T000000Z\""));
}

#[test]
fn calendar_query_filter_to_xml_simple() {
    let filter = CalendarQueryFilter::new("VEVENT");
    let xml = filter.to_filter_xml();
    assert!(xml.contains("<C:filter>"));
    assert!(xml.contains("</C:filter>"));
    assert!(xml.contains("comp-filter name=\"VCALENDAR\""));
    assert!(xml.contains("comp-filter name=\"VEVENT\""));
    assert!(!xml.contains("time-range"));
    assert!(!xml.contains("prop-filter"));
    assert!(!xml.contains("is-not-defined"));
}

#[test]
fn calendar_query_filter_to_xml_with_time_range() {
    let filter = CalendarQueryFilter::new("VEVENT")
        .with_time_range(TimeRange::new("20240101T000000Z").with_end("20240201T000000Z"));
    let xml = filter.to_filter_xml();
    assert!(xml.contains("comp-filter name=\"VEVENT\""));
    assert!(xml.contains("time-range"));
    assert!(xml.contains("start=\"20240101T000000Z\""));
    assert!(xml.contains("end=\"20240201T000000Z\""));
}

#[test]
fn calendar_query_filter_to_xml_with_prop_filters() {
    let filter = CalendarQueryFilter::new("VEVENT")
        .with_prop_filters(vec![PropFilter::new("SUMMARY", TextMatch::new("meeting"))]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("comp-filter name=\"VEVENT\""));
    assert!(xml.contains("prop-filter name=\"SUMMARY\""));
    assert!(xml.contains(">meeting</C:text-match>"));
}

#[test]
fn calendar_query_filter_to_xml_is_not_defined() {
    let filter = CalendarQueryFilter::not_defined("VEVENT");
    let xml = filter.to_filter_xml();
    assert!(xml.contains("comp-filter name=\"VEVENT\""));
    assert!(xml.contains("<C:is-not-defined/>"));
    assert!(!xml.contains("time-range"));
    assert!(!xml.contains("prop-filter name=\"SUMMARY\""));
}

#[test]
fn calendar_query_filter_to_xml_multiple_prop_filters() {
    let filter = CalendarQueryFilter::new("VEVENT").with_prop_filters(vec![
        PropFilter::new("SUMMARY", TextMatch::new("meeting")),
        PropFilter::new(
            "LOCATION",
            TextMatch::new("office").with_match_type(MatchType::Contains),
        ),
    ]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("prop-filter name=\"SUMMARY\""));
    assert!(xml.contains("prop-filter name=\"LOCATION\""));
    assert!(xml.contains(">meeting</C:text-match>"));
    assert!(xml.contains("match-type=\"contains\""));
}

#[test]
fn calendar_query_filter_to_xml_combined_time_range_and_prop_filters() {
    let filter = CalendarQueryFilter::new("VEVENT")
        .with_time_range(TimeRange::new("20240101T000000Z"))
        .with_prop_filters(vec![PropFilter::new("SUMMARY", TextMatch::new("meeting"))]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("time-range"));
    assert!(xml.contains("prop-filter name=\"SUMMARY\""));
    assert!(xml.contains(">meeting</C:text-match>"));
}

#[test]
fn calendar_query_filter_to_query_body_without_data() {
    let filter = CalendarQueryFilter::new("VEVENT");
    let body = filter.to_query_body(false);
    assert!(body.contains("<C:calendar-query"));
    assert!(body.contains("xmlns:D=\"DAV:\""));
    assert!(body.contains("xmlns:C=\"urn:ietf:params:xml:ns:caldav\""));
    assert!(body.contains("<D:getetag/>"));
    assert!(!body.contains("<C:calendar-data/>"));
    assert!(body.contains("<C:filter>"));
}

#[test]
fn calendar_query_filter_to_query_body_with_data() {
    let filter = CalendarQueryFilter::new("VEVENT");
    let body = filter.to_query_body(true);
    assert!(body.contains("<C:calendar-data/>"));
    assert!(body.contains("<D:getetag/>"));
}

#[test]
fn calendar_query_filter_to_query_body_full_structure() {
    let filter = CalendarQueryFilter::new("VEVENT")
        .with_time_range(TimeRange::new("20240101T000000Z").with_end("20240201T000000Z"))
        .with_prop_filters(vec![
            PropFilter::new(
                "SUMMARY",
                TextMatch::new("meeting").with_match_type(MatchType::Contains),
            ),
            PropFilter::new("ATTENDEE", TextMatch::new("user@example.com")).with_param_filters(
                vec![ParamFilter::new("PARTSTAT", TextMatch::new("ACCEPTED"))],
            ),
        ]);
    let body = filter.to_query_body(true);
    assert!(body.contains("<C:calendar-query"));
    assert!(body.contains("comp-filter name=\"VCALENDAR\""));
    assert!(body.contains("comp-filter name=\"VEVENT\""));
    assert!(body.contains("time-range"));
    assert!(body.contains("prop-filter name=\"SUMMARY\""));
    assert!(xml_contains(&body, "match-type=\"contains\""));
    assert!(body.contains("prop-filter name=\"ATTENDEE\""));
    assert!(body.contains("param-filter name=\"PARTSTAT\""));
    assert!(body.contains(">ACCEPTED</C:text-match>"));
    assert!(body.contains("<C:calendar-data/>"));
}

#[test]
fn calendar_query_filter_to_query_body_is_not_defined() {
    let filter = CalendarQueryFilter::not_defined("VTODO");
    let body = filter.to_query_body(false);
    assert!(body.contains("comp-filter name=\"VTODO\""));
    assert!(body.contains("<C:is-not-defined/>"));
}

#[test]
fn calendar_query_filter_escapes_component_name() {
    let filter = CalendarQueryFilter::new("VEVENT\"><evil/>");
    let xml = filter.to_filter_xml();
    assert!(!xml.contains("<evil/>"));
    assert!(xml.contains("VEVENT&quot;&gt;&lt;evil/&gt;"));
}

#[test]
fn calendar_query_filter_escapes_prop_filter_names() {
    let filter = CalendarQueryFilter::new("VEVENT").with_prop_filters(vec![PropFilter::new(
        "SUMMARY\"><x/>",
        TextMatch::new("meeting"),
    )]);
    let xml = filter.to_filter_xml();
    assert!(!xml.contains("<x/>"));
    assert!(xml.contains("SUMMARY&quot;&gt;&lt;x/&gt;"));
}

#[test]
fn calendar_query_filter_escapes_text_match_values() {
    let filter = CalendarQueryFilter::new("VEVENT").with_prop_filters(vec![PropFilter::new(
        "SUMMARY",
        TextMatch::new("Tom & Jerry <b>"),
    )]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("Tom &amp; Jerry &lt;b&gt;"));
    assert!(!xml.contains("Tom & Jerry <b>"));
}

#[test]
fn calendar_query_filter_escapes_time_range_values() {
    let filter = CalendarQueryFilter::new("VEVENT")
        .with_time_range(TimeRange::new("2024\"><x/>").with_end("20240201T000000Z'"));
    let xml = filter.to_filter_xml();
    assert!(!xml.contains("<x/>"));
    assert!(xml.contains("2024&quot;&gt;&lt;x/&gt;"));
    assert!(xml.contains("20240201T000000Z&apos;"));
}

fn xml_contains(xml: &str, fragment: &str) -> bool {
    xml.contains(fragment)
}
