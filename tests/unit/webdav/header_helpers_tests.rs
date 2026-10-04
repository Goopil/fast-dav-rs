//! Header helper tests: `schedule_tag_from_headers` and the
//! `schedule-tag` property parse into [`fast_dav_rs::webdav::DavItem`]
//! (RFC 6638 §10.1).

#[test]
fn schedule_tag_from_headers_quoted_and_bare() {
    let mut h = hyper::HeaderMap::new();
    h.insert("Schedule-Tag", r#""tag-1""#.parse().unwrap());
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&h).as_deref(),
        Some("tag-1")
    );
    h.clear();
    h.insert("Schedule-Tag", "tag-2".parse().unwrap());
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&h).as_deref(),
        Some("tag-2")
    );
    assert_eq!(
        fast_dav_rs::webdav::schedule_tag_from_headers(&hyper::HeaderMap::new()),
        None
    );
}
