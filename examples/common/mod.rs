#![allow(dead_code)] // every example includes the whole module; each uses a subset

//! Shared fixture helpers for the runnable examples: fixture endpoint
//! detection (env vars), client constructors, and iCalendar body builders.
//!
//! Each example includes this module with:
//!
//! ```ignore
//! #[path = "common/mod.rs"]
//! mod common;
//! ```

use fast_dav_rs::{CalDavClient, Result, WebDavClient};

// --- Fixture credentials ----------------------------------------------------

pub const RADICALE_USER: &str = "test";
pub const RADICALE_PASS: &str = "test";
pub const SABREDAV_USER: &str = "test";
pub const SABREDAV_PASS: &str = "test";
pub const NEXTCLOUD_USER: &str = "test";
pub const NEXTCLOUD_PASS: &str = "fixture-dav-password";

// --- Fixture endpoints ------------------------------------------------------

/// Radicale fixture root (`radicale-test/setup.sh`, http://localhost:8081).
pub fn radicale_url() -> String {
    let mut url = std::env::var("RADICALE_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// SabreDAV fixture root (`sabredav-test/setup.sh`, http://localhost:8080).
pub fn sabredav_url() -> String {
    let mut url = std::env::var("SABREDAV_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// Nextcloud DAV base (`nextcloud-test/setup.sh`, http://localhost:8083) —
/// every Nextcloud DAV path is relative to `/remote.php/dav/`.
pub fn nextcloud_dav_url() -> String {
    let mut url = std::env::var("NEXTCLOUD_URL").unwrap_or_else(|_| "http://localhost:8083".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url.push_str("remote.php/dav/");
    url
}

// --- Client constructors ----------------------------------------------------

pub fn radicale_client() -> Result<CalDavClient> {
    CalDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
}

pub fn radicale_webdav_client() -> Result<WebDavClient> {
    WebDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
}

pub fn sabredav_client() -> Result<CalDavClient> {
    CalDavClient::new(&sabredav_url(), Some(SABREDAV_USER), Some(SABREDAV_PASS))
}

pub fn sabredav_webdav_client() -> Result<WebDavClient> {
    WebDavClient::new(&sabredav_url(), Some(SABREDAV_USER), Some(SABREDAV_PASS))
}

/// Nextcloud client. When `NEXTCLOUD_BEARER_TOKEN` is set — e.g. against an
/// OIDC-enabled deployment — the builder attaches it as a `Bearer` token;
/// the fixture default is Basic auth with the account password.
pub fn nextcloud_client() -> Result<CalDavClient> {
    match std::env::var("NEXTCLOUD_BEARER_TOKEN") {
        // Bearer token (app password or OIDC access token) on a token-capable
        // deployment: the client attaches `Authorization: Bearer …` itself.
        Ok(token) if !token.is_empty() => CalDavClient::builder(nextcloud_dav_url())
            .bearer_token(token)
            .build(),
        // Fixture default: Basic auth with the account password.
        _ => CalDavClient::new(
            &nextcloud_dav_url(),
            Some(NEXTCLOUD_USER),
            Some(NEXTCLOUD_PASS),
        ),
    }
}

// --- iCalendar body builders --------------------------------------------------

/// A minimal VEVENT calendar object.
pub fn event_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//example//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260910T100000Z\r\n\
         DTEND:20260910T110000Z\r\n\
         SUMMARY:{summary}\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR"
    )
}

/// A minimal VTODO calendar object.
pub fn todo_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//example//EN\r\n\
         BEGIN:VTODO\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         SUMMARY:{summary}\r\n\
         STATUS:NEEDS-ACTION\r\n\
         END:VTODO\r\n\
         END:VCALENDAR"
    )
}
