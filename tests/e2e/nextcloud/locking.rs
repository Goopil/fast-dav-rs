//! Locking round-trip (RFC 4918 class 2) on the Nextcloud fixture, which
//! serves the sabre `Locks` plugin with a PDO backend.
//!
//! Observed fixture behavior (probed live on the 0.14 fixture): lock
//! enforcement is scoped to the files tree (`/remote.php/dav/files/`). On
//! calendar objects `LOCK` answers `200` with an opaquelocktoken but the
//! lock is not registered (`lockdiscovery` stays empty) and a token-less
//! `PUT` succeeds — Nextcloud accepts, but does not enforce, locks on the
//! CalDAV tree. The round-trip below therefore locks a file resource, the
//! surface where the fixture enforces RFC 4918, and asserts the full
//! contract: `LOCK` → token, token-less write → `423`, `UNLOCK` frees.

use super::util;
use super::util::{NEXTCLOUD_USER, nextcloud_webdav_client};
use bytes::Bytes;
use fast_dav_rs::webdav::LockScope;
use hyper::Method;

/// Full locking contract on the enforced surface (files tree): `LOCK`
/// succeeds and returns a token, a token-less `PUT` on the locked resource
/// is rejected with `423 Locked`, and `UNLOCK` with the acquired token
/// frees the resource again.
#[tokio::test]
async fn test_lock_unlock_round_trip_on_nextcloud() {
    let client = nextcloud_webdav_client();
    let path = format!(
        "files/{}/{}",
        NEXTCLOUD_USER,
        util::unique_contact_uri("e2e-nc-lock").replace(".vcf", ".txt")
    );

    let put = |body: &str| {
        let client = client.clone();
        let path = path.clone();
        let body = body.to_owned();
        async move {
            let mut h = hyper::HeaderMap::new();
            h.insert(
                hyper::header::CONTENT_TYPE,
                hyper::header::HeaderValue::from_static("text/plain"),
            );
            client
                .send(Method::PUT, &path, h, Some(Bytes::from(body)), None)
                .await
        }
    };

    let created = put("lock me").await.expect("PUT must complete");
    assert!(
        created.status().is_success(),
        "the resource must exist before locking, got {}",
        created.status()
    );

    // LOCK: exclusive write lock with a requested timeout.
    let lock = client
        .lock(
            &path,
            LockScope::Exclusive,
            "<D:href>principals/users/test</D:href>",
            Some(60),
        )
        .await
        .expect("LOCK must succeed on nextcloud (sabre Locks plugin)");
    assert!(
        !lock.token.is_empty(),
        "the server must hand out a non-empty lock token"
    );

    // A token-less PUT while the exclusive lock is held must be 423 Locked.
    let denied = put("second write").await.expect("PUT must complete");
    println!("token-less PUT while locked -> {}", denied.status());
    assert_eq!(
        denied.status().as_u16(),
        423,
        "unlocked PUT while locked must be 423, got {}",
        denied.status()
    );

    // UNLOCK with the acquired token frees the resource.
    client
        .unlock(&path, &lock.token)
        .await
        .expect("UNLOCK with the acquired token must succeed");
    let after = put("after unlock").await.expect("PUT must complete");
    assert!(
        after.status().is_success(),
        "PUT after UNLOCK must succeed, got {}",
        after.status()
    );

    // Cleanup.
    let _ = client.delete(&path).await;
}
