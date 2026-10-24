//! DNS-based service discovery (RFC 6764 §3/§6): SRV record lookup and the
//! TXT `path=` context hint, opt-in via the `srv` cargo feature.
//!
//! The RFC 6764 resolution order implemented by
//! [`discover_caldav_srv`](crate::discover_caldav_srv) /
//! [`discover_carddav_srv`](crate::discover_carddav_srv):
//! SRV record (`_{service}s._tcp` TLS first, then `_{service}._tcp`) → TXT
//! `path=` context on the same owner name → `.well-known` probe on the
//! resolved authority → base-URL fallback (§6). The existing
//! [`discover_caldav`](crate::discover_caldav) /
//! [`discover_carddav`](crate::discover_carddav) functions remain
//! DNS-free: no caller gains an implicit DNS hop.
//!
//! Failure semantics: an absent record (or any DNS error) falls through to
//! the plain `.well-known` flow on the client's base URL; a *found* record
//! makes the resolved URL authoritative — probe failures there propagate
//! as real errors, never silently masked.

use std::future::Future;
use std::pin::Pin;

use crate::Result;

/// One resolved SRV record (RFC 6764 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrvRecord {
    /// Service host (trailing dot stripped).
    pub target: String,
    /// TCP port.
    pub port: u16,
    /// Lower priority wins.
    pub priority: u16,
    /// Among equal priorities, higher weight is preferred.
    pub weight: u16,
}

/// Injectable DNS resolver used by the `discover_*_srv` functions.
///
/// The crate ships a [`hickory-resolver`](https://docs.rs/hickory-resolver)
/// backed default (system configuration) when the `srv` feature is enabled;
/// implement this trait to mock DNS in tests or plug a custom resolver, and
/// set it with [`WebDavClientBuilder::srv_resolver`](crate::WebDavClientBuilder::srv_resolver).
/// Boxed futures keep the trait object-safe (`Arc<dyn SrvResolver>`).
pub trait SrvResolver: Send + Sync {
    /// Resolve an SRV query (e.g. `_caldavs._tcp.example.com`).
    fn lookup_srv(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SrvRecord>>> + Send + '_>>;
    /// Resolve the TXT strings of `query` (the RFC 6764 §6 `path=` hint
    /// lives on the SRV owner name).
    fn lookup_txt(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;
}
