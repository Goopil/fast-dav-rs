use crate::util::unique_calendar_name;
use bytes::Bytes;
use fast_dav_rs::webdav::LockScope;
use fast_dav_rs::{CalDavClient, WebDavClient};

const SABREDAV_URL: &str = "http://localhost:8080/";
const TEST_USER: &str = "test";
const TEST_PASS: &str = "test";

fn create_test_event(uid: &str) -> String {
    format!(
        r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//fast-dav-rs//EN
BEGIN:VEVENT
UID:{uid}
DTSTAMP:20230101T000000Z
DTSTART:20231225T100000Z
DTEND:20231225T110000Z
SUMMARY:Lock lifecycle event
END:VEVENT
END:VCALENDAR"#
    )
}

/// Full WebDAV locking lifecycle (RFC 4918 class 2) against the live SabreDAV
/// fixture, which serves the `Locks` plugin with the PDO locks backend:
/// `LOCK` → `LockInfo` (opaquelocktoken shape, exclusive scope, granted
/// timeout), a token-less `PUT` rejected with `423 Locked`, `refresh_lock`
/// via the `If` header, `UNLOCK` (204), and a fresh re-lock with a new token
/// after release.
#[tokio::test]
async fn test_lock_refresh_unlock_relock_lifecycle() {
    // Setup: a real calendar collection to lock (CalDAV client), lock
    // operations driven through the WebDAV client API.
    let setup = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let calendar_name = unique_calendar_name("e2e_lock_calendar");
    let calendar_path = format!("calendars/test/{calendar_name}/");
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{calendar_name}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#
    );
    let mk = setup
        .mkcalendar(&calendar_path, &calendar_xml)
        .await
        .expect("MKCALENDAR request");
    assert!(
        mk.status().is_success(),
        "Expected successful calendar creation, got {}",
        mk.status()
    );

    // Create the event resource first; the lock then targets the resource
    // itself (`Depth: 0`, RFC 4918 §9.10.4 — a collection lock with the
    // explicit `Depth: 0` this client now sends does NOT cover children).
    let event_path = format!("{calendar_path}locked.ics");
    let created = setup
        .put(
            &event_path,
            Bytes::from(create_test_event("lock-423@example.com")),
        )
        .await
        .expect("PUT request must complete");
    assert!(
        created.status().is_success(),
        "Expected the event to be created before locking, got {}",
        created.status()
    );

    // LOCK: exclusive write lock with a requested timeout.
    let lock = client
        .lock(
            &event_path,
            LockScope::Exclusive,
            "<D:href>principals/test</D:href>",
            Some(60),
        )
        .await
        .expect("LOCK must succeed on the class-2 fixture");
    assert!(
        lock.token.starts_with("opaquelocktoken:"),
        "Expected an opaquelocktoken: URI, got {:?}",
        lock.token
    );
    assert_eq!(
        lock.scope,
        Some(LockScope::Exclusive),
        "Expected the exclusive scope to be echoed back"
    );
    assert!(
        matches!(lock.timeout_secs, Some(secs) if secs >= 60),
        "Expected a granted timeout of at least the requested 60s, got {:?}",
        lock.timeout_secs
    );

    // A PUT without the lock token must be rejected with 423 Locked.
    let denied = setup
        .put(
            &event_path,
            Bytes::from(create_test_event("lock-423@example.com")),
        )
        .await
        .expect("PUT request must complete");
    assert_eq!(
        denied.status().as_u16(),
        423,
        "Token-less write on an exclusively locked resource must be 423, got {}",
        denied.status()
    );

    // refresh_lock: re-issue LOCK with the token in an `If` header; the
    // server may rotate the token, so use the returned LockInfo afterwards.
    let refreshed = client
        .refresh_lock(&event_path, &lock.token, Some(120))
        .await
        .expect("Lock refresh must succeed while the lock is held");
    assert!(
        refreshed.token.starts_with("opaquelocktoken:"),
        "Refreshed token must keep the opaquelocktoken: shape, got {:?}",
        refreshed.token
    );

    // UNLOCK: release with the refreshed token (typical 204).
    client
        .unlock(&event_path, &refreshed.token)
        .await
        .expect("UNLOCK must succeed for a held lock");

    // Re-lock after release: must succeed with a fresh token.
    let relock = client
        .lock(
            &event_path,
            LockScope::Exclusive,
            "<D:href>principals/test</D:href>",
            Some(60),
        )
        .await
        .expect("Re-lock after UNLOCK must succeed");
    assert_ne!(
        relock.token, lock.token,
        "A fresh lock must carry a new token"
    );

    // Teardown.
    client
        .unlock(&event_path, &relock.token)
        .await
        .expect("Teardown UNLOCK must succeed");
    let del = setup
        .delete(&calendar_path)
        .await
        .expect("Teardown DELETE request");
    assert!(
        del.status().is_success(),
        "Expected successful calendar deletion, got {}",
        del.status()
    );
}

/// A lock actually enforces mutual exclusion: after `unlock`, the same PUT
/// that was rejected with 423 succeeds, proving the 423 came from the lock
/// and not from the resource state.
#[tokio::test]
async fn test_put_succeeds_after_unlock() {
    let setup = CalDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create CalDAV client");
    let client = WebDavClient::new(SABREDAV_URL, Some(TEST_USER), Some(TEST_PASS))
        .expect("Failed to create WebDAV client");

    let calendar_name = unique_calendar_name("e2e_lock_after_unlock");
    let calendar_path = format!("calendars/test/{calendar_name}/");
    let calendar_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>{calendar_name}</D:displayname>
    </D:prop>
  </D:set>
</C:mkcalendar>"#
    );
    let mk = setup
        .mkcalendar(&calendar_path, &calendar_xml)
        .await
        .expect("MKCALENDAR request");
    assert!(
        mk.status().is_success(),
        "Expected successful calendar creation, got {}",
        mk.status()
    );

    let uid = "lock-after-unlock@example.com";
    let event_path = format!("{calendar_path}{uid}.ics");

    // Create the resource first, then lock the resource itself (`Depth: 0`,
    // RFC 4918 §9.10.4 — a collection lock does NOT cover children).
    let created = setup
        .put(&event_path, Bytes::from(create_test_event(uid)))
        .await
        .expect("PUT request must complete");
    assert!(
        created.status().is_success(),
        "Expected the event to be created before locking, got {}",
        created.status()
    );

    let lock = client
        .lock(
            &event_path,
            LockScope::Exclusive,
            "<D:href>principals/test</D:href>",
            None,
        )
        .await
        .expect("LOCK must succeed on the class-2 fixture");

    // While locked: 423.
    let denied = setup
        .put(&event_path, Bytes::from(create_test_event(uid)))
        .await
        .expect("PUT request must complete");
    assert_eq!(
        denied.status().as_u16(),
        423,
        "Write under an exclusive lock must be 423, got {}",
        denied.status()
    );

    // After unlock: the same write succeeds.
    client
        .unlock(&event_path, &lock.token)
        .await
        .expect("UNLOCK must succeed");
    let put = setup
        .put(&event_path, Bytes::from(create_test_event(uid)))
        .await
        .expect("PUT request after unlock");
    assert!(
        put.status().is_success(),
        "Expected the write to succeed after unlocking, got {}",
        put.status()
    );

    // Teardown.
    let del = setup
        .delete(&calendar_path)
        .await
        .expect("Teardown DELETE request");
    assert!(
        del.status().is_success(),
        "Expected successful calendar deletion, got {}",
        del.status()
    );
}
