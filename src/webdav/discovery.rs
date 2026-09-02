//! RFC 6764 §5 service discovery via the `.well-known/caldav` and
//! `.well-known/carddav` URIs.
//!
//! The probe reuses the client's request pipeline (`build_and_send`): HTTP
//! redirects (301/302/303/307/308) are followed and credentials are attached
//! per the client's configuration, with `Authorization`/`Cookie` stripped
//! automatically when a hop crosses origins.
//!
//! DNS SRV record lookup (RFC 6764 §3/§6 step 2) is not implemented — the
//! caller supplies the base URL.

use bytes::Bytes;
use hyper::{HeaderMap, Method, StatusCode, header};

use crate::webdav::client::{PROBE_BODY, WebDavClient};
use crate::{Error, Operation, Result};

/// Shared implementation behind [`discover_caldav`] / [`discover_carddav`]:
/// probe `{base}/.well-known/{service}` and resolve the service URL from the
/// redirect chain.
async fn discover_well_known(
    client: &WebDavClient,
    service: &str,
    operation: Operation,
) -> Result<String> {
    let path = format!("/.well-known/{service}");
    let mut headers = HeaderMap::new();
    headers.insert("Depth", header::HeaderValue::from_static("0"));
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/xml; charset=utf-8"),
    );

    let (final_uri, resp) = client
        .build_and_send(
            Method::from_bytes(b"PROPFIND")?,
            &path,
            headers,
            Some(Bytes::from_static(PROBE_BODY.as_bytes())),
            None,
        )
        .await?;

    let status = resp.status();
    if status == StatusCode::NOT_FOUND {
        // RFC 6764 §6: a 404 on the initial "context path" means the server
        // does not advertise the service there — fall back to the base URL.
        return Ok(client.base().to_string());
    }
    if !status.is_success() {
        return Err(Error::UnexpectedStatus { operation, status });
    }
    if final_uri.path() == path {
        // Success served directly on the .well-known URI, without a redirect.
        // Per RFC 5785 §1.1 the actual service endpoint must not live there,
        // so the base URL is returned unchanged.
        Ok(client.base().to_string())
    } else {
        // Redirects were followed by the pipeline: the final request URL is
        // the discovered service URL.
        Ok(final_uri.to_string())
    }
}

/// Discover the CalDAV service "context path" for the client's base URL
/// (RFC 6764 §5, `.well-known/caldav`).
///
/// Issues the canonical probe — a `PROPFIND` with `Depth: 0` requesting
/// `DAV:current-user-principal` (RFC 6764 §6) — against
/// `{base}/.well-known/caldav`, reusing the client's redirect, auth, timeout,
/// and compression pipeline. Redirects (301/302/303/307/308) are followed
/// automatically; the **final** request URL is the discovered service URL.
///
/// # Fallbacks and errors
///
/// - `404 Not Found`, or a success answered directly on the `.well-known`
///   URI (where the real endpoint must not live, RFC 5785 §1.1): the base
///   URL is returned unchanged (RFC 6764 §6 permits retrying at the root).
/// - Any other non-success status: [`Error::UnexpectedStatus`] with
///   [`Operation::DiscoverWellKnownCaldav`].
/// - Transport failures (connection refused, timeout, …) propagate unchanged.
///
/// Client credentials (Basic/Bearer) are attached to the probe — RFC 6764 §5
/// allows servers to require authentication before redirecting; on
/// cross-origin redirect hops they are stripped automatically.
///
/// DNS SRV record lookup (RFC 6764 §3) is not part of this API.
///
/// # Example
///
/// ```no_run
/// use fast_dav_rs::{WebDavClient, discover_caldav};
///
/// # async fn run() -> fast_dav_rs::Result<()> {
/// let client = WebDavClient::builder("https://dav.example.com/")
///     .basic_auth("user", "pass")
///     .build()?;
/// let service_url = discover_caldav(&client).await?;
/// println!("CalDAV service lives at {service_url}");
/// # Ok(())
/// # }
/// ```
pub async fn discover_caldav(client: &WebDavClient) -> Result<String> {
    discover_well_known(client, "caldav", Operation::DiscoverWellKnownCaldav).await
}

/// Discover the CardDAV service "context path" for the client's base URL
/// (RFC 6764 §5, `.well-known/carddav`).
///
/// Behaves exactly like [`discover_caldav`] but probes
/// `{base}/.well-known/carddav`; non-success statuses map to
/// [`Error::UnexpectedStatus`] with [`Operation::DiscoverWellKnownCarddav`].
///
/// # Example
///
/// ```no_run
/// use fast_dav_rs::{WebDavClient, discover_carddav};
///
/// # async fn run() -> fast_dav_rs::Result<()> {
/// let client = WebDavClient::builder("https://dav.example.com/")
///     .basic_auth("user", "pass")
///     .build()?;
/// let service_url = discover_carddav(&client).await?;
/// println!("CardDAV service lives at {service_url}");
/// # Ok(())
/// # }
/// ```
pub async fn discover_carddav(client: &WebDavClient) -> Result<String> {
    discover_well_known(client, "carddav", Operation::DiscoverWellKnownCarddav).await
}
