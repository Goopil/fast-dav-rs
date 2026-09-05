#![allow(dead_code)] // each test crate includes the whole module via #[path]

use std::sync::atomic::{AtomicU64, Ordering};

use fast_dav_rs::{CalDavClient, CardDavClient, WebDavClient};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn unique_calendar_name(prefix: &str) -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}_{}_{}",
        prefix,
        chrono::Utc::now().timestamp_micros(),
        counter
    )
}

pub fn unique_addressbook_name(prefix: &str) -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}_{}_{}",
        prefix,
        chrono::Utc::now().timestamp_micros(),
        counter
    )
}

pub fn unique_uid(prefix: &str) -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{}@example.com",
        prefix,
        chrono::Utc::now().timestamp_micros(),
        counter
    )
}

pub fn unique_contact_uri(prefix: &str) -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}_{}_{}.vcf",
        prefix,
        chrono::Utc::now().timestamp_micros(),
        counter
    )
}

// --- Fixture credentials ----------------------------------------------------

pub const RADICALE_USER: &str = "test";
pub const RADICALE_PASS: &str = "test";
pub const NEXTCLOUD_USER: &str = "test";
pub const NEXTCLOUD_PASS: &str = "fixture-dav-password";
pub const SABREDAV_USER: &str = "test";
pub const SABREDAV_PASS: &str = "test";

// --- Fixture endpoints ------------------------------------------------------

/// Radicale fixture root (`radicale-test/setup.sh`, http://localhost:8081).
pub fn radicale_url() -> String {
    let mut url = std::env::var("RADICALE_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// Nextcloud fixture site root (`nextcloud-test/setup.sh`, http://localhost:8083).
pub fn nextcloud_url() -> String {
    let mut url = std::env::var("NEXTCLOUD_URL").unwrap_or_else(|_| "http://localhost:8083".into());
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// Nextcloud DAV base — every Nextcloud DAV path is relative to it.
pub fn nextcloud_dav_url() -> String {
    format!("{}remote.php/dav/", nextcloud_url())
}

/// SabreDAV fixture root (`sabredav-test/setup.sh`, http://localhost:8080).
pub fn sabredav_url() -> String {
    std::env::var("SABREDAV_URL").unwrap_or_else(|_| "http://localhost:8080/".into())
}

// --- Per-fixture client constructors ----------------------------------------

pub fn radicale_caldav_client() -> CalDavClient {
    CalDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
        .expect("CalDAV client construction")
}

pub fn radicale_carddav_client() -> CardDavClient {
    CardDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
        .expect("CardDAV client construction")
}

pub fn radicale_webdav_client() -> WebDavClient {
    WebDavClient::new(&radicale_url(), Some(RADICALE_USER), Some(RADICALE_PASS))
        .expect("WebDAV client construction")
}

pub fn nextcloud_caldav_client() -> CalDavClient {
    CalDavClient::new(
        &nextcloud_dav_url(),
        Some(NEXTCLOUD_USER),
        Some(NEXTCLOUD_PASS),
    )
    .expect("CalDAV client construction")
}

pub fn nextcloud_carddav_client() -> CardDavClient {
    CardDavClient::new(
        &nextcloud_dav_url(),
        Some(NEXTCLOUD_USER),
        Some(NEXTCLOUD_PASS),
    )
    .expect("CardDAV client construction")
}

pub fn nextcloud_webdav_client() -> WebDavClient {
    WebDavClient::new(
        &nextcloud_dav_url(),
        Some(NEXTCLOUD_USER),
        Some(NEXTCLOUD_PASS),
    )
    .expect("WebDAV client construction")
}

pub fn sabredav_caldav_client() -> CalDavClient {
    CalDavClient::new(&sabredav_url(), Some(SABREDAV_USER), Some(SABREDAV_PASS))
        .expect("CalDAV client construction")
}

// --- Shared object body builders ---------------------------------------------

/// A minimal VEVENT calendar object (RFC 4791 §4.1); the summary round-trips
/// verbatim on every fixture.
pub fn event_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//e2e//EN\r\n\
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

/// A minimal VTODO calendar object; the summary round-trips verbatim on
/// every fixture that stores tasks.
pub fn vtodo_ics(uid: &str, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//fast-dav-rs//e2e//EN\r\n\
         BEGIN:VTODO\r\n\
         UID:{uid}\r\n\
         DTSTAMP:20260101T000000Z\r\n\
         SUMMARY:{summary}\r\n\
         STATUS:NEEDS-ACTION\r\n\
         END:VTODO\r\n\
         END:VCALENDAR"
    )
}

/// A minimal vCard (RFC 6352 §6.3.2 address-object creation); the formatted
/// name round-trips verbatim on every fixture.
pub fn vcard(fn_name: &str, email: &str) -> String {
    format!(
        "BEGIN:VCARD\r\n\
         VERSION:4.0\r\n\
         UID:{fn_name}@example.com\r\n\
         FN:{fn_name}\r\n\
         EMAIL:{email}\r\n\
         END:VCARD"
    )
}

/// A minimal RFC 4791 §5.2.2 `calendar-timezone` value: one `VTIMEZONE`
/// component for Europe/Paris with the standard/daylight rules.
pub fn vtimezone_ics() -> String {
    concat!(
        "BEGIN:VCALENDAR\r\n",
        "VERSION:2.0\r\n",
        "BEGIN:VTIMEZONE\r\n",
        "TZID:Europe/Paris\r\n",
        "BEGIN:STANDARD\r\n",
        "DTSTART:19701025T030000\r\n",
        "RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\n",
        "TZOFFSETFROM:+0200\r\n",
        "TZOFFSETTO:+0100\r\n",
        "TZNAME:CET\r\n",
        "END:STANDARD\r\n",
        "BEGIN:DAYLIGHT\r\n",
        "DTSTART:19700329T020000\r\n",
        "RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\n",
        "TZOFFSETFROM:+0100\r\n",
        "TZOFFSETTO:+0200\r\n",
        "TZNAME:CEST\r\n",
        "END:DAYLIGHT\r\n",
        "END:VTIMEZONE\r\n",
        "END:VCALENDAR"
    )
    .to_owned()
}
