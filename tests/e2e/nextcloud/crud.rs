//! Nextcloud CRUD round-trips: calendar (VEVENT + VTODO) and addressbook.

use super::util;
use super::util::{NEXTCLOUD_USER, nextcloud_caldav_client, nextcloud_carddav_client};
use bytes::Bytes;

#[tokio::test]
async fn test_calendar_crud_round_trip_with_vtodo() {
    let client = nextcloud_caldav_client();
    let calendar_path = format!(
        "calendars/{NEXTCLOUD_USER}/{}/",
        util::unique_calendar_name("nc_e2e")
    );

    let mkcalendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{}</D:displayname>
      <C:supported-calendar-component-set>
        <C:comp name="VEVENT"/>
        <C:comp name="VTODO"/>
      </C:supported-calendar-component-set>
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

    // VEVENT round-trip.
    let uid = util::unique_uid("nc-e2e-event");
    let event_path = format!("{calendar_path}{uid}.ics");
    let event_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//Nextcloud E2E//EN
BEGIN:VEVENT
UID:{uid}
DTSTAMP:20260101T000000Z
DTSTART:20260701T100000Z
DTEND:20260701T110000Z
SUMMARY:Nextcloud e2e event
END:VEVENT
END:VCALENDAR"#
    );
    let put = client
        .put(&event_path, Bytes::from(event_ics))
        .await
        .expect("event PUT");
    assert!(
        put.status().is_success(),
        "event PUT must succeed, got {}, body: {}",
        put.status(),
        String::from_utf8_lossy(put.body())
    );

    let get = client.get(&event_path).await.expect("event GET");
    assert!(
        get.status().is_success(),
        "event GET must succeed, got {}",
        get.status()
    );
    let event_body = String::from_utf8_lossy(&get.into_body()).into_owned();
    assert!(
        event_body.contains(&uid),
        "GET must return the stored event containing the event UID, got: {event_body}"
    );

    // VTODO round-trip (task coverage).
    let todo_uid = util::unique_uid("nc-e2e-todo");
    let todo_path = format!("{calendar_path}{todo_uid}.ics");
    let todo_ics = format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//Nextcloud E2E//EN
BEGIN:VTODO
UID:{todo_uid}
DTSTAMP:20260101T000000Z
SUMMARY:Nextcloud e2e task
STATUS:NEEDS-ACTION
END:VTODO
END:VCALENDAR"#
    );
    let todo_put = client
        .put(&todo_path, Bytes::from(todo_ics))
        .await
        .expect("VTODO PUT");
    assert!(
        todo_put.status().is_success(),
        "VTODO PUT must succeed, got {}, body: {}",
        todo_put.status(),
        String::from_utf8_lossy(todo_put.body())
    );
    let todo_get = client.get(&todo_path).await.expect("VTODO GET");
    assert!(todo_get.status().is_success(), "VTODO GET must succeed");
    let todo_body = String::from_utf8_lossy(&todo_get.into_body()).into_owned();
    assert!(
        todo_body.contains("VTODO") && todo_body.contains(&todo_uid),
        "GET must return the stored VTODO, got: {todo_body}"
    );

    // Cleanup.
    let delete_event = client.delete(&event_path).await.expect("event DELETE");
    assert!(
        delete_event.status().is_success(),
        "event DELETE must succeed"
    );
    let delete_todo = client.delete(&todo_path).await.expect("VTODO DELETE");
    assert!(
        delete_todo.status().is_success(),
        "VTODO DELETE must succeed"
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

#[tokio::test]
async fn test_addressbook_crud_round_trip() {
    let client = nextcloud_carddav_client();
    let book_path = format!(
        "addressbooks/users/{NEXTCLOUD_USER}/{}/",
        util::unique_addressbook_name("nc_e2e")
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
    // Nextcloud has no MKADDRESSBOOK method; the client must transparently
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

    let contact_path = format!("{book_path}{}", util::unique_contact_uri("nc-e2e"));
    let vcard = r#"BEGIN:VCARD
VERSION:4.0
UID:nc-e2e-contact@example.com
FN:Nextcloud E2E Contact
N:Contact;Nextcloud E2E;;;
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
        body.contains("FN:Nextcloud E2E Contact"),
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
