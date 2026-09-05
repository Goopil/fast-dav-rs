//! A small Nextcloud DAV client: Bearer-token auth, VTODO (task) creation
//! and fetching.
//!
//! Target fixture: **Nextcloud** (`nextcloud-test/`, Basic auth `test` /
//! `fixture-dav-password`, DAV root `/remote.php/dav/`). First boot is slow:
//! run `./nextcloud-test/setup.sh` and wait for it to finish.
//!
//! ```sh
//! ./nextcloud-test/setup.sh       # http://localhost:8083 (first boot: minutes)
//! cargo run --example nextcloud_client
//! ```
//!
//! Auth: the fixture ships without OIDC, so the default path uses Basic auth
//! (an app password works the same way on hardened instances). When
//! `NEXTCLOUD_BEARER_TOKEN` is set — e.g. against an OIDC-enabled deployment —
//! the builder attaches it as a `Bearer` token instead.

#[path = "common/mod.rs"]
mod common;

use bytes::Bytes;

use common::{nextcloud_client, todo_ics};

const COLLECTION: &str = "calendars/test/example-nextcloud-todos/";

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    let client = nextcloud_client()?;
    println!("dav root OPTIONS: {}", client.options("").await?.status());

    // Nextcloud calendars need an explicit supported-component set to accept
    // VTODOs (see nextcloud-test/README.md).
    let mk = r#"<?xml version="1.0" encoding="UTF-8"?>
<C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set><D:prop>
    <D:displayname>nextcloud example (VTODO)</D:displayname>
    <C:supported-calendar-component-set>
      <C:comp name="VEVENT"/><C:comp name="VTODO"/>
    </C:supported-calendar-component-set>
  </D:prop></D:set>
</C:mkcalendar>"#;
    let resp = client.mkcalendar(COLLECTION, mk).await?;
    println!("mkcalendar: {}", resp.status());

    // Create two tasks.
    for (uid, summary) in [
        ("todo-1@example.com", "water the plants"),
        ("todo-2@example.com", "ship 0.12"),
    ] {
        let resp = client
            .put(
                &format!("{COLLECTION}{uid}.ics"),
                Bytes::from(todo_ics(uid, summary)),
            )
            .await?;
        println!("PUT {summary}: {}", resp.status());
    }

    // Fetch them back with a calendar-query restricted to VTODO components.
    let todos = client
        .calendar_query(
            COLLECTION,
            &fast_dav_rs::caldav::CalendarQueryFilter::new("VTODO"),
            true,
        )
        .await?;
    println!("calendar-query VTODO: {} tasks", todos.len());
    for todo in &todos {
        println!(
            "  {} -> {}",
            todo.href,
            todo.calendar_data.as_deref().unwrap_or("(no data)")
        );
    }

    // Cleanup so re-runs start from a known state.
    client.delete(COLLECTION).await?;
    println!("deleted calendar — done");
    Ok(())
}
