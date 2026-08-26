use fast_dav_rs::carddav::client::{
    VCARD_CONTENT_TYPE, build_addressbook_query_filter, build_addressbook_query_filter_email,
    build_addressbook_query_filter_fn, build_addressbook_query_filter_uid,
};
use fast_dav_rs::carddav::types::{CardDavFilter, Collation, MatchType, ParamFilter, TextMatch};

#[test]
fn vcard_content_type_includes_version_4() {
    assert!(VCARD_CONTENT_TYPE.contains("text/vcard"));
    assert!(VCARD_CONTENT_TYPE.contains("charset=utf-8"));
    assert!(VCARD_CONTENT_TYPE.contains("version=4.0"));
}

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
fn build_filter_default_collation_and_match_type() {
    let xml = build_addressbook_query_filter(
        "UID",
        "user-123",
        Collation::default(),
        MatchType::default(),
        false,
    );
    assert!(xml.contains("collation=\"i;unicode-casemap\""));
    assert!(xml.contains("match-type=\"equals\""));
    assert!(xml.contains(">user-123</C:text-match>"));
    assert!(!xml.contains("negate-condition"));
}

#[test]
fn build_filter_ascii_casemap() {
    let xml = build_addressbook_query_filter(
        "EMAIL",
        "test@example.com",
        Collation::AsciiCasemap,
        MatchType::Equals,
        false,
    );
    assert!(xml.contains("collation=\"i;ascii-casemap\""));
}

#[test]
fn build_filter_contains_match_type() {
    let xml = build_addressbook_query_filter(
        "FN",
        "Jane",
        Collation::default(),
        MatchType::Contains,
        false,
    );
    assert!(xml.contains("match-type=\"contains\""));
}

#[test]
fn build_filter_starts_with_match_type() {
    let xml = build_addressbook_query_filter(
        "FN",
        "Jane",
        Collation::default(),
        MatchType::StartsWith,
        false,
    );
    assert!(xml.contains("match-type=\"starts-with\""));
}

#[test]
fn build_filter_ends_with_match_type() {
    let xml = build_addressbook_query_filter(
        "FN",
        "Doe",
        Collation::default(),
        MatchType::EndsWith,
        false,
    );
    assert!(xml.contains("match-type=\"ends-with\""));
}

#[test]
fn build_filter_negate_true_adds_attribute() {
    let xml = build_addressbook_query_filter(
        "EMAIL",
        "spam@example.com",
        Collation::default(),
        MatchType::Equals,
        true,
    );
    assert!(xml.contains("negate-condition=\"yes\""));
}

#[test]
fn build_filter_negate_false_omits_attribute() {
    let xml = build_addressbook_query_filter(
        "EMAIL",
        "test@example.com",
        Collation::default(),
        MatchType::Equals,
        false,
    );
    assert!(!xml.contains("negate-condition"));
}

#[test]
fn build_filter_escapes_special_chars() {
    let xml = build_addressbook_query_filter(
        "FN",
        "Tom & Jerry <script>",
        Collation::default(),
        MatchType::default(),
        false,
    );
    assert!(xml.contains("Tom &amp; Jerry &lt;script&gt;"));
    assert!(!xml.contains("Tom & Jerry <script>"));
}

#[test]
fn build_filter_uid_uses_defaults() {
    let xml = build_addressbook_query_filter_uid("user-123");
    assert!(xml.contains("prop-filter name=\"UID\""));
    assert!(xml.contains("collation=\"i;unicode-casemap\""));
    assert!(xml.contains("match-type=\"equals\""));
    assert!(xml.contains(">user-123</C:text-match>"));
    assert!(!xml.contains("negate-condition"));
}

#[test]
fn build_filter_email_uses_defaults() {
    let xml = build_addressbook_query_filter_email("test@example.com");
    assert!(xml.contains("prop-filter name=\"EMAIL\""));
    assert!(xml.contains("collation=\"i;unicode-casemap\""));
    assert!(xml.contains("match-type=\"equals\""));
    assert!(xml.contains("test@example.com"));
}

#[test]
fn build_filter_fn_uses_defaults() {
    let xml = build_addressbook_query_filter_fn("Ada Lovelace");
    assert!(xml.contains("prop-filter name=\"FN\""));
    assert!(xml.contains("collation=\"i;unicode-casemap\""));
    assert!(xml.contains("match-type=\"equals\""));
    assert!(xml.contains("Ada Lovelace"));
}

#[test]
fn carddav_filter_simple_text_match() {
    let filter = CardDavFilter::new("EMAIL", "test@example.com");
    let xml = filter.to_filter_xml();
    assert!(xml.contains("<C:filter>"));
    assert!(xml.contains("</C:filter>"));
    assert!(xml.contains("prop-filter name=\"EMAIL\""));
    assert!(xml.contains("text-match"));
    assert!(xml.contains("collation=\"i;unicode-casemap\""));
    assert!(xml.contains("match-type=\"equals\""));
    assert!(xml.contains(">test@example.com</C:text-match>"));
    assert!(!xml.contains("negate-condition"));
    assert!(!xml.contains("is-not-defined"));
    assert!(!xml.contains("param-filter"));
}

#[test]
fn carddav_filter_with_param_filter() {
    let filter = CardDavFilter::new("EMAIL", "test@example.com")
        .with_param_filters(vec![ParamFilter::new("TYPE", TextMatch::new("work"))]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("param-filter name=\"TYPE\""));
    assert!(xml.contains(">work</C:text-match>"));
    assert!(xml.contains("prop-filter name=\"EMAIL\""));
    assert!(xml.contains(">test@example.com</C:text-match>"));
}

#[test]
fn carddav_filter_is_not_defined() {
    let filter = CardDavFilter::new("NICKNAME", "").with_is_not_defined(true);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("prop-filter name=\"NICKNAME\""));
    assert!(xml.contains("<C:is-not-defined/>"));
    assert!(!xml.contains("text-match"));
}

#[test]
fn carddav_filter_negate() {
    let filter = CardDavFilter::new("EMAIL", "spam@example.com").with_negate(true);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("negate-condition=\"yes\""));
}

#[test]
fn carddav_filter_param_filter_is_not_defined() {
    let filter = CardDavFilter::new("EMAIL", "test@example.com")
        .with_param_filters(vec![ParamFilter::not_defined("TYPE")]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("param-filter name=\"TYPE\""));
    assert!(xml.contains("<C:is-not-defined/>"));
}

#[test]
fn carddav_filter_param_filter_negate() {
    let mut tm = TextMatch::new("home");
    tm.negate = true;
    let filter = CardDavFilter::new("EMAIL", "test@example.com")
        .with_param_filters(vec![ParamFilter::new("TYPE", tm)]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("param-filter name=\"TYPE\""));
    assert!(xml.contains("negate-condition=\"yes\""));
    assert!(xml.contains(">home</C:text-match>"));
}

#[test]
fn carddav_filter_multiple_param_filters() {
    let filter = CardDavFilter::new("EMAIL", "test@example.com").with_param_filters(vec![
        ParamFilter::new("TYPE", TextMatch::new("work")),
        ParamFilter::new("PREF", TextMatch::new("1")),
    ]);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("param-filter name=\"TYPE\""));
    assert!(xml.contains("param-filter name=\"PREF\""));
    assert!(xml.contains(">work</C:text-match>"));
    assert!(xml.contains(">1</C:text-match>"));
}

#[test]
fn carddav_filter_escapes_values() {
    let filter = CardDavFilter::new("FN", "Tom & Jerry");
    let xml = filter.to_filter_xml();
    assert!(xml.contains("Tom &amp; Jerry"));
    assert!(!xml.contains("Tom & Jerry"));
}

#[test]
fn carddav_filter_ascii_casemap() {
    let filter =
        CardDavFilter::new("EMAIL", "test@example.com").with_collation(Collation::AsciiCasemap);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("collation=\"i;ascii-casemap\""));
}

#[test]
fn carddav_filter_contains_match_type() {
    let filter = CardDavFilter::new("FN", "Jane").with_match_type(MatchType::Contains);
    let xml = filter.to_filter_xml();
    assert!(xml.contains("match-type=\"contains\""));
}

#[test]
fn text_match_to_xml_defaults() {
    let tm = TextMatch::new("work");
    let xml = tm.to_xml();
    assert!(xml.contains("collation=\"i;unicode-casemap\""));
    assert!(xml.contains("match-type=\"equals\""));
    assert!(xml.contains(">work</C:text-match>"));
    assert!(!xml.contains("negate-condition"));
}

#[test]
fn text_match_to_xml_custom_collation() {
    let mut tm = TextMatch::new("work");
    tm.collation = Collation::AsciiCasemap;
    let xml = tm.to_xml();
    assert!(xml.contains("collation=\"i;ascii-casemap\""));
}

#[test]
fn text_match_to_xml_negate() {
    let mut tm = TextMatch::new("work");
    tm.negate = true;
    let xml = tm.to_xml();
    assert!(xml.contains("negate-condition=\"yes\""));
}

#[test]
fn text_match_to_xml_escapes_value() {
    let tm = TextMatch::new("Tom & Jerry");
    let xml = tm.to_xml();
    assert!(xml.contains("Tom &amp; Jerry"));
    assert!(!xml.contains("Tom & Jerry"));
}

#[test]
fn param_filter_to_xml_with_text_match() {
    let pf = ParamFilter::new("TYPE", TextMatch::new("work"));
    let xml = pf.to_xml();
    assert!(xml.contains("param-filter name=\"TYPE\""));
    assert!(xml.contains(">work</C:text-match>"));
}

#[test]
fn param_filter_to_xml_is_not_defined() {
    let pf = ParamFilter::not_defined("TYPE");
    let xml = pf.to_xml();
    assert!(xml.contains("param-filter name=\"TYPE\""));
    assert!(xml.contains("<C:is-not-defined/>"));
    assert!(!xml.contains("text-match"));
}

#[test]
fn param_filter_to_xml_escapes_name() {
    let pf = ParamFilter::new("TYPE&NAME", TextMatch::new("work"));
    let xml = pf.to_xml();
    assert!(xml.contains("TYPE&amp;NAME"));
}
