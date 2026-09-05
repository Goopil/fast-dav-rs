//! Radicale core behaviors: principal/home-set discovery, calendar and
//! addressbook CRUD round-trips (VEVENT + VTODO), and the auto-create
//! boundary for arbitrary paths.

use super::util;
use super::util::{RADICALE_USER, radicale_caldav_client, radicale_carddav_client};
use bytes::Bytes;

fn principal_path() -> String {
    format!("{RADICALE_USER}/")
}

#[tokio::test]
async fn test_discover_principal_and_home_sets() {
    let client = radicale_caldav_client();

    let principal = client
        .discover_current_user_principal()
        .await
        .expect("root PROPFIND must succeed on Radicale")
        .expect("Radicale must advertise the current-user-principal on the root");
    println!("current-user-principal: {principal}");
    assert!(!principal.is_empty(), "principal href must not be empty");

    // Radicale answers the principal href as a path ("/test/"); build_uri
    // accepts absolute-path and absolute-URL hrefs alike.
    let cal_home_sets = client
        .discover_calendar_home_set(&principal)
        .await
        .expect("calendar home-set discovery");
    assert!(
        !cal_home_sets.is_empty(),
        "Radicale must advertise a calendar home set, got: {cal_home_sets:?}"
    );
    println!("calendar home sets: {cal_home_sets:?}");

    let ab_home_sets = radicale_carddav_client()
        .discover_addressbook_home_set(&principal)
        .await
        .expect("addressbook home-set discovery");
    assert!(
        !ab_home_sets.is_empty(),
        "Radicale must advertise an addressbook home set, got: {ab_home_sets:?}"
    );
    println!("addressbook home sets: {ab_home_sets:?}");
}

#[tokio::test]
async fn test_calendar_crud_round_trip() {
    let client = radicale_caldav_client();
    let calendar_path = format!(
        "{}{}/",
        principal_path(),
        util::unique_calendar_name("radicale_e2e")
    );

    let mkcalendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#,
        calendar_path
    );
    let created = client
        .mkcalendar(&calendar_path, &mkcalendar_xml)
        .await
        .expect("MKCALENDAR request");
    assert!(
        created.status().is_success(),
        "MKCALENDAR must succeed, got {}",
        created.status()
    );

    let uid = util::unique_uid("radicale-e2e");
    let event_path = format!("{calendar_path}{uid}.ics");
    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//Radicale E2E//EN
BEGIN:VEVENT
UID:{uid}
DTSTAMP:20260101T000000Z
DTSTART:20260601T100000Z
DTEND:20260601T110000Z
SUMMARY:Radicale e2e event
END:VEVENT
END:VCALENDAR"#
    );
    let put = client
        .put(&event_path, Bytes::from(event_ics))
        .await
        .expect("event PUT");
    assert!(
        put.status().is_success(),
        "PUT must succeed, got {}",
        put.status()
    );

    let get = client.get(&event_path).await.expect("event GET");
    assert!(
        get.status().is_success(),
        "GET must succeed, got {}",
        get.status()
    );
    let body = get.into_body();
    assert!(
        String::from_utf8_lossy(&body).contains(&uid),
        "GET must return the stored event containing the event UID, got: {}",
        String::from_utf8_lossy(&body)
    );

    let delete_event = client.delete(&event_path).await.expect("event DELETE");
    assert!(
        delete_event.status().is_success(),
        "event DELETE must succeed, got {}",
        delete_event.status()
    );
    let delete_calendar = client
        .delete(&calendar_path)
        .await
        .expect("calendar DELETE");
    assert!(
        delete_calendar.status().is_success(),
        "calendar DELETE must succeed, got {}",
        delete_calendar.status()
    );
    // Radicale must not resurrect the collection (no auto-create on GET for
    // arbitrary paths — auto-create is limited to the principal tree).
    let gone = client.get(&calendar_path).await.expect("GET after delete");
    assert_eq!(
        gone.status().as_u16(),
        404,
        "deleted calendar must stay deleted (Radicale does not auto-create arbitrary collections)"
    );
}

#[tokio::test]
async fn test_addressbook_crud_round_trip() {
    let client = radicale_carddav_client();
    let book_path = format!(
        "{}{}/",
        principal_path(),
        util::unique_addressbook_name("radicale_e2e")
    );

    let mkaddressbook_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkaddressbook xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
    </D:prop>
  </D:set>
</C:mkaddressbook>"#,
        book_path
    );
    // Radicale has no MKADDRESSBOOK method; the client must transparently
    // fall back to extended MKCOL.
    let created = client
        .mkaddressbook(&book_path, &mkaddressbook_xml)
        .await
        .expect("MKADDRESSBOOK/MKCOL request");
    assert!(
        created.status().is_success(),
        "address book creation must succeed (direct or MKCOL fallback), got {}",
        created.status()
    );

    let contact_path = format!("{book_path}{}", util::unique_contact_uri("radicale-e2e"));
    let vcard = r#"BEGIN:VCARD
VERSION:4.0
UID:radicale-e2e-contact@example.com
FN:Radicale E2E Contact
N:Contact;Radicale E2E;;;
END:VCARD"#;
    let put = client
        .put(&contact_path, Bytes::from(vcard))
        .await
        .expect("vCard PUT");
    assert!(
        put.status().is_success(),
        "PUT must succeed, got {}",
        put.status()
    );

    let get = client.get(&contact_path).await.expect("vCard GET");
    assert!(
        get.status().is_success(),
        "GET must succeed, got {}",
        get.status()
    );
    let body = String::from_utf8_lossy(&get.into_body()).into_owned();
    assert!(
        body.contains("FN:Radicale E2E Contact"),
        "GET must return the stored vCard, got: {body}"
    );

    let delete_contact = client.delete(&contact_path).await.expect("vCard DELETE");
    assert!(
        delete_contact.status().is_success(),
        "vCard DELETE must succeed, got {}",
        delete_contact.status()
    );
    let delete_book = client.delete(&book_path).await.expect("addressbook DELETE");
    assert!(
        delete_book.status().is_success(),
        "addressbook DELETE must succeed, got {}",
        delete_book.status()
    );
}

/// Gap closure (0.14 H6): VTODO round-trip on Radicale — previously only
/// exercised against Nextcloud. Radicale stores VTODO components on regular
/// calendars; the task is PUT, fetched back verbatim via GET, and deleted.
#[tokio::test]
async fn test_vtodo_round_trip_on_radicale() {
    let client = radicale_caldav_client();
    let calendar_path = format!(
        "{}{}/",
        principal_path(),
        util::unique_calendar_name("radicale_vtodo")
    );

    let mkcalendar_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"/>"#;
    let created = client
        .mkcalendar(&calendar_path, mkcalendar_xml)
        .await
        .expect("MKCALENDAR request");
    assert!(
        created.status().is_success(),
        "MKCALENDAR must succeed, got {}",
        created.status()
    );

    let uid = util::unique_uid("radicale-vtodo");
    let todo_path = format!("{calendar_path}{uid}.ics");
    let todo_ics = util::vtodo_ics(&uid, "Radicale e2e task");
    let put = client
        .put(&todo_path, Bytes::from(todo_ics))
        .await
        .expect("VTODO PUT");
    assert!(
        put.status().is_success(),
        "VTODO PUT must succeed, got {}",
        put.status()
    );

    let get = client.get(&todo_path).await.expect("VTODO GET");
    assert!(
        get.status().is_success(),
        "VTODO GET must succeed, got {}",
        get.status()
    );
    let body = String::from_utf8_lossy(&get.into_body()).into_owned();
    assert!(
        body.contains("VTODO") && body.contains(&uid),
        "GET must return the stored VTODO, got: {body}"
    );
    assert!(
        body.contains("Radicale e2e task"),
        "the VTODO summary must round-trip, got: {body}"
    );

    let delete_todo = client.delete(&todo_path).await.expect("VTODO DELETE");
    assert!(
        delete_todo.status().is_success(),
        "VTODO DELETE must succeed, got {}",
        delete_todo.status()
    );
    let delete_calendar = client
        .delete(&calendar_path)
        .await
        .expect("calendar DELETE");
    assert!(
        delete_calendar.status().is_success(),
        "calendar DELETE must succeed, got {}",
        delete_calendar.status()
    );
}

/// Documents the boundary of Radicale's auto-create quirk: the principal
/// collection tree is created on first authenticated access, but arbitrary
/// nonexistent paths stay nonexistent (404) — no phantom collections.
#[tokio::test]
async fn test_no_auto_create_for_arbitrary_paths() {
    let client = radicale_caldav_client();

    let missing_collection = format!(
        "{}{}/",
        principal_path(),
        util::unique_calendar_name("radicale_ghost")
    );
    let get = client
        .get(&missing_collection)
        .await
        .expect("GET on missing collection");
    assert_eq!(
        get.status().as_u16(),
        404,
        "GET must not auto-create a collection"
    );

    let missing_item = format!(
        "{}fixture-calendar/{}.ics",
        principal_path(),
        util::unique_uid("radicale-ghost")
    );
    let get_item = client
        .get(&missing_item)
        .await
        .expect("GET on missing item");
    assert_eq!(
        get_item.status().as_u16(),
        404,
        "GET must not auto-create an item"
    );
}
