/// S4.1 — `data_element_xml_with_limits` (RFC 4791 §9.6.4): both children
/// serialize inside `<C:calendar-data>`, with `<C:expand>` before the limits
/// (RFC 4791 §9.6 DTD order).
#[test]
fn data_element_xml_with_limits_serializes_both_children() {
    let limits = fast_dav_rs::webdav::CalendarDataLimits::new().with_recurrence_set(
        fast_dav_rs::TimeRange::new("20240101T000000Z").with_end("20241231T235959Z"),
    );
    let xml = fast_dav_rs::webdav::xml::data_element_xml_with_limits(
        "calendar-data",
        Some(("20240101T000000Z", "20240301T000000Z")),
        Some(&limits),
    );
    assert!(xml.contains("<C:calendar-data>"));
    assert!(xml.contains("<C:expand start=\"20240101T000000Z\" end=\"20240301T000000Z\"/>"));
    assert!(
        xml.contains(
            "<C:limit-recurrence-set start=\"20240101T000000Z\" end=\"20241231T235959Z\"/>"
        )
    );
    // expand must precede limits (RFC 4791 §9.6 DTD order)
    assert!(xml.find("<C:expand").unwrap() < xml.find("<C:limit-recurrence-set").unwrap());
}

/// S4.1 — `limit-freebusy-set` serializes on its own when only the freebusy
/// limit is set.
#[test]
fn data_element_xml_with_limits_serializes_freebusy_limit() {
    let limits = fast_dav_rs::webdav::CalendarDataLimits::new().with_freebusy_set(
        fast_dav_rs::TimeRange::new("20240101T000000Z").with_end("20240201T000000Z"),
    );
    let xml = fast_dav_rs::webdav::xml::data_element_xml_with_limits(
        "calendar-data",
        Some(("20240101T000000Z", "20240301T000000Z")),
        Some(&limits),
    );
    assert!(
        xml.contains("<C:limit-freebusy-set start=\"20240101T000000Z\" end=\"20240201T000000Z\"/>")
    );
    assert!(!xml.contains("limit-recurrence-set"));
    assert!(xml.find("<C:expand").unwrap() < xml.find("<C:limit-freebusy-set").unwrap());
}

/// S4.1 — both limits serialize in DTD order
/// (`limit-recurrence-set` then `limit-freebusy-set`).
#[test]
fn data_element_xml_with_limits_orders_recurrence_before_freebusy() {
    let limits = fast_dav_rs::webdav::CalendarDataLimits::new()
        .with_recurrence_set(
            fast_dav_rs::TimeRange::new("20240101T000000Z").with_end("20241231T235959Z"),
        )
        .with_freebusy_set(
            fast_dav_rs::TimeRange::new("20240101T000000Z").with_end("20240201T000000Z"),
        );
    let xml = fast_dav_rs::webdav::xml::data_element_xml_with_limits(
        "calendar-data",
        Some(("20240101T000000Z", "20240301T000000Z")),
        Some(&limits),
    );
    let recurrence = xml.find("<C:limit-recurrence-set").unwrap();
    let freebusy = xml.find("<C:limit-freebusy-set").unwrap();
    assert!(recurrence < freebusy, "DTD order violated: {xml}");
}

/// S4.1 — `None` limits keep the bare expanded form (no limit elements).
#[test]
fn data_element_xml_with_limits_none_omits_limits() {
    let xml = fast_dav_rs::webdav::xml::data_element_xml_with_limits(
        "calendar-data",
        Some(("20240101T000000Z", "20240301T000000Z")),
        None,
    );
    assert_eq!(
        xml,
        "<C:calendar-data><C:expand start=\"20240101T000000Z\" end=\"20240301T000000Z\"/></C:calendar-data>"
    );
}

/// S4.1 — the existing `data_element_xml` behavior is unchanged through the
/// delegation: `build_sync_collection_body` (which delegates to it) still
/// renders the bare element, expand without end, and expand with end.
#[test]
fn data_element_xml_delegate_keeps_existing_shapes() {
    use fast_dav_rs::{SyncLevel, webdav::build_sync_collection_body};
    assert_eq!(
        build_sync_collection_body(
            Some("token"),
            None,
            true,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
            None,
            SyncLevel::Infinite,
        ),
        "<D:sync-collection xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\
<D:sync-token>token</D:sync-token><D:sync-level>infinite</D:sync-level>\
<D:prop><D:getetag/><C:calendar-data/></D:prop></D:sync-collection>"
    );
    assert!(
        build_sync_collection_body(
            None,
            None,
            true,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
            Some(("20240101T000000Z", None)),
            SyncLevel::One,
        )
        .contains("<C:expand start=\"20240101T000000Z\"/>")
    );
    assert!(
        build_sync_collection_body(
            None,
            None,
            true,
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
            Some(("20240101T000000Z", Some("20240301T000000Z"))),
            SyncLevel::One,
        )
        .contains("<C:expand start=\"20240101T000000Z\" end=\"20240301T000000Z\"/>")
    );
}

/// S4.1 — metacharacters in the data element name and limit values are
/// escaped (untrusted values must not inject markup).
#[test]
fn data_element_xml_with_limits_escapes_values() {
    let limits = fast_dav_rs::webdav::CalendarDataLimits::new().with_recurrence_set(
        fast_dav_rs::TimeRange::new("20240101T000000Z").with_end("<injected/>"),
    );
    let xml = fast_dav_rs::webdav::xml::data_element_xml_with_limits(
        "calendar-data",
        None,
        Some(&limits),
    );
    assert!(
        xml.contains("end=\"&lt;injected/&gt;\""),
        "unescaped: {xml}"
    );
    assert!(!xml.contains("<injected/>"));
}

/// S4.3 — `build_propfind_allprop` produces the RFC 4918 §9.1 allprop body.
#[test]
fn build_propfind_allprop_full_body() {
    assert_eq!(
        fast_dav_rs::webdav::build_propfind_allprop(),
        "<D:propfind xmlns:D=\"DAV:\"><D:allprop/></D:propfind>"
    );
}

/// S4.3 — `build_propfind_propname` produces the RFC 4918 §9.1 propname body.
#[test]
fn build_propfind_propname_full_body() {
    assert_eq!(
        fast_dav_rs::webdav::build_propfind_propname(),
        "<D:propfind xmlns:D=\"DAV:\"><D:propname/></D:propfind>"
    );
}

/// S4.3 — `build_propfind_props` groups the namespace declarations on the
/// root and renders `<D:prop>` children as `<ns-prefix:name/>` (for `DAV:`
/// the conventional `D:` prefix).
#[test]
fn build_propfind_props_groups_namespaces_on_root() {
    let body = fast_dav_rs::webdav::build_propfind_props(&[
        ("DAV:", "displayname"),
        ("urn:ietf:params:xml:ns:caldav", "calendar-description"),
    ]);
    assert_eq!(
        body,
        "<D:propfind xmlns:D=\"DAV:\" xmlns:ns1=\"urn:ietf:params:xml:ns:caldav\">\
<D:prop><D:displayname/><ns1:calendar-description/></D:prop></D:propfind>"
    );
}

/// S4.3 — repeated namespaces share one declaration and keep their prefix;
/// distinct namespaces get sequential `ns1`, `ns2`, … prefixes.
#[test]
fn build_propfind_props_dedupes_namespace_declarations() {
    let body = fast_dav_rs::webdav::build_propfind_props(&[
        ("urn:ietf:params:xml:ns:caldav", "calendar-data"),
        ("DAV:", "getetag"),
        ("urn:ietf:params:xml:ns:caldav", "calendar-data"), // hmm, duplicate pair
        ("http://calendarserver.org/ns/", "getctag"),
    ]);
    assert_eq!(
        body,
        "<D:propfind xmlns:D=\"DAV:\" xmlns:ns1=\"urn:ietf:params:xml:ns:caldav\" \
xmlns:ns2=\"http://calendarserver.org/ns/\"><D:prop>\
<ns1:calendar-data/><D:getetag/><ns1:calendar-data/><ns2:getctag/>\
</D:prop></D:propfind>"
    );
}

/// S4.3 — namespace URIs and local names are escaped so untrusted values
/// cannot inject markup.
#[test]
fn build_propfind_props_escapes_names_and_namespaces() {
    let body = fast_dav_rs::webdav::build_propfind_props(&[
        ("DAV:", "display\"name"),
        ("<evil>/", "<x/>"),
    ]);
    assert!(body.contains("<D:display&quot;name/>"), "unescaped: {body}");
    assert!(
        body.contains("xmlns:ns1=\"&lt;evil&gt;/\""),
        "unescaped namespace: {body}"
    );
    assert!(
        body.contains("<ns1:&lt;x/&gt;/>"),
        "unescaped local name: {body}"
    );
    assert!(!body.contains("<x/>"), "injection possible: {body}");
}

/// S4.4 — `build_mkcalendar_body` renders the RFC 4791 §9.5 skeleton with
/// all typed properties.
#[test]
fn build_mkcalendar_body_serializes_all_props() {
    let props = fast_dav_rs::webdav::MkCalendarProps::new()
        .with_displayname("My Calendar")
        .with_description("Work events")
        .with_supported_components(["VEVENT", "VTODO"]);
    let body = fast_dav_rs::webdav::build_mkcalendar_body(&props).unwrap();
    assert_eq!(
        body,
        "<C:mkcalendar xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\
<D:set><D:prop><D:displayname>My Calendar</D:displayname>\
<C:calendar-description>Work events</C:calendar-description>\
<C:supported-calendar-component-set><C:comp name=\"VEVENT\"/><C:comp name=\"VTODO\"/>\
</C:supported-calendar-component-set></D:prop></D:set></C:mkcalendar>"
    );
}

/// S4.4 — empty props produce the minimal §9.5 skeleton (no
/// `supported-calendar-component-set` element).
#[test]
fn build_mkcalendar_body_empty_props_minimal_skeleton() {
    let body =
        fast_dav_rs::webdav::build_mkcalendar_body(&fast_dav_rs::webdav::MkCalendarProps::new())
            .unwrap();
    assert_eq!(
        body,
        "<C:mkcalendar xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\
<D:set><D:prop></D:prop></D:set></C:mkcalendar>"
    );
}

/// S4.4 — component names in the supported-component set are validated
/// (ASCII alphanumeric + `-`, non-empty) before any XML is produced.
#[test]
fn build_mkcalendar_body_rejects_invalid_component_names() {
    for bad in ["VE EVENT", "VE;EVENT", "VE<EVENT", "", "ÉVÉNEMENT"] {
        let props = fast_dav_rs::webdav::MkCalendarProps::new().with_supported_components([bad]);
        let err = fast_dav_rs::webdav::build_mkcalendar_body(&props).unwrap_err();
        assert!(
            matches!(err, fast_dav_rs::Error::InvalidComponentName { .. }),
            "expected InvalidComponentName for {bad:?}, got: {err:?}"
        );
    }
}

/// S4.4 — displayname and description are escaped so untrusted values
/// cannot inject markup.
#[test]
fn build_mkcalendar_body_escapes_text_values() {
    let props = fast_dav_rs::webdav::MkCalendarProps::new()
        .with_displayname("<script>")
        .with_description("a & b");
    let body = fast_dav_rs::webdav::build_mkcalendar_body(&props).unwrap();
    assert!(body.contains("<D:displayname>&lt;script&gt;</D:displayname>"));
    assert!(body.contains("<C:calendar-description>a &amp; b</C:calendar-description>"));
    assert!(!body.contains("<script>"));
}
