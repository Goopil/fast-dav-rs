//! Nextcloud discovery: principal/home-sets on the DAV root and the strict
//! `/remote.php/dav/` scoping.

use super::util::{NEXTCLOUD_USER, nextcloud_caldav_client, nextcloud_carddav_client};
use fast_dav_rs::Depth;

/// Nextcloud principal path (note the `users/` segment — an Nextcloud
/// specific layout; see nextcloud-test/README.md).
fn principal_path() -> String {
    format!("principals/users/{NEXTCLOUD_USER}/")
}

#[tokio::test]
async fn test_discover_principal_and_home_sets() {
    let client = nextcloud_caldav_client();

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

    let ab_home_sets = nextcloud_carddav_client()
        .discover_addressbook_home_set(&principal)
        .await
        .expect("addressbook home-set discovery");
    assert!(
        !ab_home_sets.is_empty(),
        "Nextcloud must advertise an addressbook home set, got: {ab_home_sets:?}"
    );
    println!("addressbook home sets: {ab_home_sets:?}");
}

/// Nextcloud serves its DAV tree strictly under `/remote.php/dav/`; the
/// site root is not DAV-capable and well-known URIs redirect there.
#[tokio::test]
async fn test_dav_root_scoping() {
    let client = nextcloud_caldav_client();

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
