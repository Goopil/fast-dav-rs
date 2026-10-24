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

#[cfg(feature = "srv")]
pub use discover_impls::{discover_caldav_srv, discover_carddav_srv};

#[cfg(feature = "srv")]
mod discover_impls {
    use std::cmp::Reverse;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, OnceLock};

    use super::{SrvRecord, SrvResolver};
    use crate::webdav::client::WebDavClient;
    use crate::webdav::discovery::discover_well_known;
    use crate::{Error, Operation, Result};

    /// The default resolver: hickory with the system configuration, built
    /// once per process (clients clone cheaply; DNS setup is process-global
    /// by nature).
    fn default_resolver() -> Arc<dyn SrvResolver> {
        static DEFAULT: OnceLock<Arc<dyn SrvResolver>> = OnceLock::new();
        Arc::clone(DEFAULT.get_or_init(|| {
            Arc::new(
                hickory_resolver::Resolver::builder_tokio()
                    .expect("system DNS configuration must be readable")
                    .build()
                    .expect("system DNS resolver must build"),
            )
        }))
    }

    impl SrvResolver for hickory_resolver::TokioResolver {
        fn lookup_srv(
            &self,
            query: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<SrvRecord>>> + Send + '_>> {
            let query = query.to_string();
            Box::pin(async move {
                let lookup = hickory_resolver::Resolver::srv_lookup(self, query.as_str())
                    .await
                    .map_err(|e| Error::other(format!("SRV lookup {query} failed: {e}")))?;
                let mut records = Vec::new();
                for record in lookup.answers() {
                    if let hickory_resolver::proto::rr::RData::SRV(srv) = &record.data {
                        let mut target = srv.target.to_string();
                        if target.ends_with('.') {
                            target.pop();
                        }
                        records.push(SrvRecord {
                            target,
                            port: srv.port,
                            priority: srv.priority,
                            weight: srv.weight,
                        });
                    }
                }
                Ok(records)
            })
        }

        fn lookup_txt(
            &self,
            query: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
            let query = query.to_string();
            Box::pin(async move {
                let lookup = hickory_resolver::Resolver::txt_lookup(self, query.as_str())
                    .await
                    .map_err(|e| Error::other(format!("TXT lookup {query} failed: {e}")))?;
                let mut strings = Vec::new();
                for record in lookup.answers() {
                    if let hickory_resolver::proto::rr::RData::TXT(txt) = &record.data {
                        let joined: String = txt
                            .txt_data
                            .iter()
                            .map(|s| String::from_utf8_lossy(s))
                            .collect();
                        strings.push(joined);
                    }
                }
                Ok(strings)
            })
        }
    }

    /// Lowest priority wins; highest weight first among ties (spec decision).
    fn select_srv(mut records: Vec<SrvRecord>) -> Option<SrvRecord> {
        records.sort_by_key(|r| (r.priority, Reverse(r.weight)));
        records.into_iter().next()
    }

    /// First `path=` attribute across the TXT strings (RFC 6764 §6); the
    /// value keeps a leading slash.
    fn txt_path(records: &[String]) -> Option<String> {
        for record in records {
            for chunk in record.split_whitespace() {
                if let Some(path) = chunk.strip_prefix("path=") {
                    return Some(if path.starts_with('/') {
                        path.to_string()
                    } else {
                        format!("/{path}")
                    });
                }
            }
        }
        None
    }

    /// `scheme://host:port` out of a candidate base (drops any TXT path).
    fn candidate_origin(candidate: &str) -> String {
        let (scheme, rest) = candidate.split_once("://").unwrap_or(("", candidate));
        let authority = rest.split('/').next().unwrap_or(rest);
        format!("{scheme}://{authority}")
    }

    async fn discover_srv(
        client: &WebDavClient,
        domain: &str,
        service: &str,
        tls_query: &str,
        plain_query: &str,
        operation: Operation,
    ) -> Result<String> {
        let domain = domain.trim();
        if domain.is_empty() {
            return Err(Error::InvalidInput(
                "SRV discovery requires a non-empty domain".to_string(),
            ));
        }
        let resolver: Arc<dyn SrvResolver> = client
            .srv_resolver()
            .cloned()
            .unwrap_or_else(default_resolver);

        let mut candidate_base: Option<String> = None;
        // TLS record first; the plain (http) record is skipped entirely
        // when the client requires https. Any DNS absence OR failure at
        // this stage degrades to the plain .well-known flow (spec).
        for (query, scheme) in [
            (format!("{tls_query}.{domain}"), "https"),
            (format!("{plain_query}.{domain}"), "http"),
        ] {
            if scheme == "http" && client.require_https() {
                continue;
            }
            let Ok(records) = resolver.lookup_srv(&query).await else {
                continue;
            };
            let Some(record) = select_srv(records) else {
                continue;
            };
            let authority = format!("{scheme}://{}:{}", record.target, record.port);
            let path = resolver
                .lookup_txt(&query)
                .await
                .ok()
                .as_deref()
                .and_then(txt_path);
            candidate_base = Some(match path {
                Some(path) => format!("{authority}{path}"),
                None => format!("{authority}/"),
            });
            break;
        }

        match candidate_base {
            // Authoritative: the probe targets the SRV authority root; a
            // 404 falls back to the SRV+TXT candidate base, any other
            // failure propagates (no silent masking).
            Some(candidate) => {
                let origin = candidate_origin(&candidate);
                discover_well_known(
                    client,
                    &format!("{origin}/.well-known/{service}"),
                    &candidate,
                    operation,
                )
                .await
            }
            // No record anywhere: exactly today's flow, on the client base.
            None => {
                discover_well_known(
                    client,
                    &format!("/.well-known/{service}"),
                    &client.base_url_string(),
                    operation,
                )
                .await
            }
        }
    }

    /// Discover the CalDAV service via DNS SRV/TXT records (RFC 6764 §3/§6).
    ///
    /// Requires the `srv` cargo feature. Resolution order: TLS SRV record
    /// (`_caldavs._tcp.{domain}`, then `_caldav._tcp.{domain}` unless the
    /// client requires https), TXT `path=` hint on the same owner name, the
    /// `.well-known` probe against the resolved authority, and the
    /// base-URL fallback on `404` (§6). No record — or any DNS error —
    /// degrades gracefully to the plain [`discover_caldav`] flow; a found
    /// record makes its URL authoritative (probe failures propagate).
    ///
    /// The probe follows the crate's credential invariant: credentials are
    /// only ever sent to the client's base origin, so a cross-origin SRV
    /// authority is probed unauthenticated.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::{WebDavClient, discover_caldav_srv};
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = WebDavClient::builder("https://dav.example.com/")
    ///     .basic_auth("user", "pass")
    ///     .build()?;
    /// let service_url = discover_caldav_srv(&client, "example.com").await?;
    /// println!("CalDAV service lives at {service_url}");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover_caldav_srv(client: &WebDavClient, domain: &str) -> Result<String> {
        discover_srv(
            client,
            domain,
            "caldav",
            "_caldavs._tcp",
            "_caldav._tcp",
            Operation::DiscoverWellKnownCaldav,
        )
        .await
    }

    /// Discover the CardDAV service via DNS SRV/TXT records — the CardDAV
    /// mirror of [`discover_caldav_srv`] (`_carddavs._tcp.{domain}`, then
    /// `_carddav._tcp.{domain}`).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::{WebDavClient, discover_carddav_srv};
    ///
    /// # async fn run() -> fast_dav_rs::Result<()> {
    /// let client = WebDavClient::builder("https://dav.example.com/")
    ///     .basic_auth("user", "pass")
    ///     .build()?;
    /// let service_url = discover_carddav_srv(&client, "example.com").await?;
    /// println!("CardDAV service lives at {service_url}");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover_carddav_srv(client: &WebDavClient, domain: &str) -> Result<String> {
        discover_srv(
            client,
            domain,
            "carddav",
            "_carddavs._tcp",
            "_carddav._tcp",
            Operation::DiscoverWellKnownCarddav,
        )
        .await
    }

    #[cfg(test)]
    mod helper_tests {
        use super::{select_srv, txt_path};
        use crate::webdav::srv::SrvRecord;

        #[test]
        fn lowest_priority_wins_highest_weight_among_ties() {
            let records = vec![
                SrvRecord {
                    target: "a".into(),
                    port: 1,
                    priority: 10,
                    weight: 0,
                },
                SrvRecord {
                    target: "b".into(),
                    port: 2,
                    priority: 5,
                    weight: 1,
                },
                SrvRecord {
                    target: "c".into(),
                    port: 3,
                    priority: 5,
                    weight: 100,
                },
            ];
            let best = select_srv(records).unwrap();
            assert_eq!((best.target.as_str(), best.port), ("c", 3));
            assert!(select_srv(Vec::new()).is_none());
        }

        #[test]
        fn first_path_attribute_wins_and_is_root_anchored() {
            let records = vec![
                "some txt".to_string(),
                "path=/a/ other=1".to_string(),
                "path=/b/".to_string(),
            ];
            assert_eq!(txt_path(&records).as_deref(), Some("/a/"));
            assert_eq!(txt_path(&["path=b/".to_string()]).as_deref(), Some("/b/"));
            assert_eq!(txt_path(&["no path here".to_string()]), None);
            assert!(txt_path(&[]).is_none());
        }
    }
}
