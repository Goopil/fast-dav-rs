//! E2E tests against the Radicale fixture (`radicale-test/`).
//!
//! Bring the fixture up first: `./radicale-test/setup.sh`. The base URL
//! defaults to `http://localhost:8081` and can be overridden with the
//! `RADICALE_URL` env var.

#[path = "e2e/util.rs"]
mod util;

use bytes::Bytes;
use fast_dav_rs::{CalDavClient, CardDavClient, Depth, Error, LockScope, WebDavClient};

const RADICALE_USER: &str = "test";
const RADICALE_PASS: &str = "test";

fn radicale_url() -> String {
    let mut url = std::env::var("RADICALE_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

fn caldav_client() -> CalDavClient {
    CalDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
        .expect("CalDAV client construction")
}

fn carddav_client() -> CardDavClient {
    CardDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
        .expect("CardDAV client construction")
}

fn principal_path() -> String {
    format!("{RADICALE_USER}/")
}

#[tokio::test]
async fn test_discover_principal_and_home_sets() {
    let client = caldav_client();

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

    let ab_home_sets = carddav_client()
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
    let client = caldav_client();
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
        "GET must return the stored event containing UID {uid}, got: {}",
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
    let client = carddav_client();
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

/// Records Radicale's observed behavior for a `sync-collection` REPORT with
/// an unknown token. Observed on Radicale 3.7.6: `403 Forbidden` with
/// `<D:error><valid-sync-token/></D:error>` (RFC 6578 §3.2 stale-token
/// signal). The assertion is deliberately loose (any non-success status);
/// the raw status and body are printed for the record.
#[tokio::test]
async fn test_sync_collection_unknown_token_records_observed_behavior() {
    let client = caldav_client();
    let raw = WebDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
        .expect("raw WebDAV client");

    let calendar_path = format!(
        "{}{}/",
        principal_path(),
        util::unique_calendar_name("radicale_sync")
    );
    let mkcalendar_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"/>"#;
    let created = client
        .mkcalendar(&calendar_path, mkcalendar_xml)
        .await
        .expect("MKCALENDAR");
    assert!(created.status().is_success(), "MKCALENDAR must succeed");

    let uid = util::unique_uid("radicale-sync");
    let event_path = format!("{calendar_path}{uid}.ics");
    let put = client
        .put(&event_path, Bytes::from(format!("BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//fast-dav-rs//Radicale E2E//EN\nBEGIN:VEVENT\nUID:{uid}\nDTSTAMP:20260101T000000Z\nDTSTART:20260601T100000Z\nEND:VEVENT\nEND:VCALENDAR")))
        .await
        .expect("event PUT");
    assert!(put.status().is_success(), "PUT must succeed");

    // The initial sync hands out a real token; keep it out of the request to
    // prove the *server* rejects the fabricated one.
    let initial = client
        .sync_collection(&calendar_path, None, Some(100), true, None)
        .await
        .expect("initial sync_collection must succeed");
    assert!(
        initial.sync_token.is_some(),
        "Radicale must issue a sync token"
    );
    assert_eq!(initial.items.len(), 1, "initial sync must list the event");

    let bogus_report = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:sync-collection xmlns:D="DAV:">
  <D:sync-token>http://radicale.example/ns/sync/DOES-NOT-EXIST</D:sync-token>
  <D:prop><D:getetag/></D:prop>
</D:sync-collection>"#;
    let resp = raw
        .report(&calendar_path, Depth::Zero, bogus_report)
        .await
        .expect("REPORT with unknown token must complete (no transport error)");
    println!(
        "Radicale sync-collection REPORT with unknown token -> status {} body: {}",
        resp.status(),
        String::from_utf8_lossy(resp.body())
    );
    assert!(
        !resp.status().is_success(),
        "Radicale must reject an unknown sync token (observed 403 + valid-sync-token), got {}",
        resp.status()
    );

    // Client-level contract: `sync_collection_resilient` treats the observed
    // stale-token signal (403 + valid-sync-token, or 410) as "resync" and
    // transparently re-issues an initial sync (the DAVx⁵ rule).
    let (_, items, token, resynced) = raw
        .sync_collection_resilient(
            &calendar_path,
            Some("http://radicale.example/ns/sync/DOES-NOT-EXIST"),
            None,
            false,
            "urn:ietf:params:xml:ns:caldav",
            "getetag",
        )
        .await
        .expect("resilient sync must transparently fall back to an initial sync");
    assert!(resynced, "unknown token must surface as resynced=true");
    assert!(!items.is_empty(), "the resync must return the live items");
    assert!(token.is_some(), "the resync must hand out a fresh token");
    println!(
        "resilient resync returned {} item(s) with token {token:?}",
        items.len()
    );

    // The server must still be healthy after the rejected REPORT.
    let alive = client
        .propfind(
            &calendar_path,
            Depth::Zero,
            r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#,
        )
        .await
        .expect("PROPFIND after rejected REPORT");
    assert!(
        alive.status().is_success(),
        "server must stay healthy, got {}",
        alive.status()
    );

    let _ = client.delete(&event_path).await;
    let _ = client.delete(&calendar_path).await;
}

/// Records Radicale's observed behavior for WebDAV LOCK. Observed on
/// Radicale 3.7.6: `OPTIONS /` advertises `DAV: 1, 2, 3` but LOCK answers
/// `405 Method Not Allowed`. The assertion is deliberately loose (any
/// client/server error); the status is printed for the record.
#[tokio::test]
async fn test_lock_unsupported_records_observed_behavior() {
    let client = caldav_client();

    let fixture_calendar = format!("{RADICALE_USER}/fixture-calendar/");
    let err = client
        .lock(
            &fixture_calendar,
            LockScope::Exclusive,
            "<D:href>fast-dav-rs-e2e</D:href>",
            Some(60),
        )
        .await
        .expect_err("Radicale must not support LOCK (observed 405)");
    match err {
        Error::UnexpectedStatus { status, .. } => {
            println!("Radicale LOCK -> UnexpectedStatus {}", status);
            assert!(status.is_client_error() || status.is_server_error());
        }
        Error::UnexpectedStatusWithDav { status, .. } => {
            println!("Radicale LOCK -> UnexpectedStatusWithDav {status}");
            assert!(status.is_client_error() || status.is_server_error());
        }
        other => panic!("expected an UnexpectedStatus error for LOCK, got: {other:?}"),
    }

    // Robustness: normal operations keep working after the rejected LOCK.
    let alive = client
        .propfind(
            &fixture_calendar,
            Depth::Zero,
            r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#,
        )
        .await
        .expect("PROPFIND after rejected LOCK");
    assert!(
        alive.status().is_success(),
        "server must stay healthy, got {}",
        alive.status()
    );
}

/// Documents the boundary of Radicale's auto-create quirk: the principal
/// collection tree is created on first authenticated access, but arbitrary
/// nonexistent paths stay nonexistent (404) — no phantom collections.
#[tokio::test]
async fn test_no_auto_create_for_arbitrary_paths() {
    let client = caldav_client();

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
