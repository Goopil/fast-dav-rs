//! E2E tests against the Nextcloud fixture (`nextcloud-test/`).
//!
//! Bring the fixture up first: `./nextcloud-test/setup.sh`. The site URL
//! defaults to `http://localhost:8083` (override with `NEXTCLOUD_URL`); all
//! DAV paths live under the Nextcloud standard `/remote.php/dav/` tree.

#[path = "e2e/util.rs"]
mod util;

use bytes::Bytes;
use fast_dav_rs::{CalDavClient, CardDavClient, Depth};

const NEXTCLOUD_USER: &str = "test";
const NEXTCLOUD_PASS: &str = "fixture-dav-password";

/// Nextcloud site root (base URL for well-known redirects).
fn nextcloud_url() -> String {
    let mut url = std::env::var("NEXTCLOUD_URL").unwrap_or_else(|_| "http://localhost:8083".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// Nextcloud DAV base — all test paths are relative to it.
fn dav_url() -> String {
    format!("{}remote.php/dav/", nextcloud_url())
}

fn caldav_client() -> CalDavClient {
    CalDavClient::new(&dav_url(), Some(NEXTCLOUD_USER), Some(NEXTCLOUD_PASS))
        .expect("CalDAV client construction")
}

fn carddav_client() -> CardDavClient {
    CardDavClient::new(&dav_url(), Some(NEXTCLOUD_USER), Some(NEXTCLOUD_PASS))
        .expect("CardDAV client construction")
}

/// Nextcloud principal path (note the `users/` segment — an Nextcloud
/// specific layout; see nextcloud-test/README.md).
fn principal_path() -> String {
    format!("principals/users/{NEXTCLOUD_USER}/")
}

#[tokio::test]
async fn test_discover_principal_and_home_sets() {
    let client = caldav_client();

    // Nextcloud serves `current-user-principal` on the DAV root PROPFIND.
    let principal = client
        .discover_current_user_principal()
        .await
        .expect("DAV root PROPFIND must succeed on Nextcloud")
        .expect("Nextcloud must advertise the current-user-principal on the DAV root");
    println!("current-user-principal: {principal}");
    assert!(
        principal.contains(&format!("principals/users/{NEXTCLOUD_USER}")),
        "principal href must point at the Nextcloud principals tree, got: {principal}"
    );

    let cal_home_sets = client
        .discover_calendar_home_set(&principal)
        .await
        .expect("calendar home-set discovery");
    assert!(
        !cal_home_sets.is_empty(),
        "Nextcloud must advertise a calendar home set, got: {cal_home_sets:?}"
    );
    println!("calendar home sets: {cal_home_sets:?}");

    let ab_home_sets = carddav_client()
        .discover_addressbook_home_set(&principal)
        .await
        .expect("addressbook home-set discovery");
    assert!(
        !ab_home_sets.is_empty(),
        "Nextcloud must advertise an addressbook home set, got: {ab_home_sets:?}"
    );
    println!("addressbook home sets: {ab_home_sets:?}");
}

#[tokio::test]
async fn test_calendar_crud_round_trip_with_vtodo() {
    let client = caldav_client();
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
        "GET must return the stored event containing UID {uid}, got: {event_body}"
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
    let client = carddav_client();
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

/// Nextcloud serves its DAV tree strictly under `/remote.php/dav/`; the
/// site root is not DAV-capable and well-known URIs redirect there.
#[tokio::test]
async fn test_dav_root_scoping() {
    let client = caldav_client();

    // PROPFIND of the DAV root must answer the current-user-principal.
    let resp = client
        .propfind(
            "",
            Depth::Zero,
            r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:current-user-principal/></D:prop></D:propfind>"#,
        )
        .await
        .expect("DAV root PROPFIND");
    assert!(
        resp.status().is_success(),
        "DAV root PROPFIND must succeed, got {}",
        resp.status()
    );
    let body = String::from_utf8_lossy(resp.body()).into_owned();
    assert!(
        body.contains("current-user-principal"),
        "DAV root must advertise the current-user-principal, got: {body}"
    );

    // The principal collection also answers home-set queries.
    let homes = client
        .discover_calendar_home_set(&principal_path())
        .await
        .expect("home-set discovery via the principal path");
    assert!(
        !homes.is_empty(),
        "home-set must be discoverable via the principal path"
    );
}

/// `SyncSession` happy path (issue #160) against Nextcloud: the initial
/// snapshot lists the event with its calendar data and a sync token, and the
/// follow-up incremental sync returns an empty delta (the fixture is static
/// after setup).
#[tokio::test]
async fn test_sync_session_initial_and_empty_incremental() {
    let client = caldav_client();
    let calendar_path = format!(
        "calendars/{NEXTCLOUD_USER}/{}/",
        util::unique_calendar_name("nc_session")
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

    let uid = util::unique_uid("nc-session");
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
SUMMARY:Nextcloud sync session event
END:VEVENT
END:VCALENDAR"#
    );
    let put = client
        .put(&event_path, Bytes::from(event_ics))
        .await
        .expect("event PUT");
    assert!(put.status().is_success(), "event PUT must succeed");

    let session = client.sync_session(&calendar_path);
    let snapshot = session.initial().await.expect("initial sync session");
    assert!(
        snapshot.items.iter().any(|entry| entry.href.contains(&uid)),
        "initial snapshot must list the event ({} items)",
        snapshot.items.len()
    );
    assert!(
        snapshot.sync_token.is_some(),
        "Nextcloud must issue a sync token for the session"
    );
    assert!(
        snapshot
            .items
            .iter()
            .all(|entry| entry.data.is_some() && entry.etag.is_some()),
        "the CalDAV session must fetch calendar data alongside the etags ({} items)",
        snapshot.items.len()
    );

    // The fixture is static after setup: the incremental sync is empty.
    let delta = session.incremental().await.expect("incremental sync");
    assert!(!delta.resynced);
    assert!(
        delta.added.is_empty() && delta.modified.is_empty() && delta.deleted.is_empty(),
        "no changes since the initial sync ({} added, {} modified, {} deleted)",
        delta.added.len(),
        delta.modified.len(),
        delta.deleted.len()
    );
    assert!(
        delta.sync_token.is_some(),
        "the incremental sync must hand out a token to persist"
    );

    // Cleanup.
    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}
