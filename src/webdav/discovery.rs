//! RFC 6764 §5 service discovery via the `.well-known/caldav` and
//! `.well-known/carddav` URIs.
//!
//! The probe reuses the client's request pipeline (`build_and_send`): HTTP
//! redirects (301/302/303/307/308) are followed **only when the client's
//! `follow_redirects` is enabled** (the default — RFC 6764 §5 requires
//! clients to handle `.well-known` redirects), and credentials are attached
//! per the client's configuration, with `Authorization`/`Cookie` stripped
//! automatically when a hop crosses origins. With `follow_redirects(false)`
//! — or when a `Location` cannot be resolved or would downgrade https→http
//! — the probe returns the 3xx and discovery fails with a descriptive
//! error.
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
        if status.is_redirection() {
            // RFC 6764 §5 requires clients to handle .well-known redirects.
            // A 3xx reaching this point means the pipeline could not follow
            // the hop: `follow_redirects(false)` on the client, an
            // unresolvable `Location`, or an https→http downgrade.
            return Err(Error::other(format!(
                "discovery probe received redirect status {status} that was not followed \
                 (is follow_redirects disabled?); the {path} service URL cannot be resolved"
            )));
        }
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
/// **when the client's `follow_redirects` is enabled** (the builder default);
/// the **final** request URL is the discovered service URL. RFC 6764 §5
/// requires clients to handle `.well-known` redirects, so leave redirect
/// following enabled: with `follow_redirects(false)` — or when a hop cannot
/// be resolved/followed (e.g. an https→http downgrade) — the probe returns
/// the 3xx and discovery fails with a descriptive error.
///
/// # Fallbacks and errors
///
/// - `404 Not Found`, or a success answered directly on the `.well-known`
///   URI (where the real endpoint must not live, RFC 5785 §1.1): the base
///   URL is returned unchanged (RFC 6764 §6 permits retrying at the root).
/// - Any other non-success status: [`Error::UnexpectedStatus`] with
///   [`Operation::DiscoverWellKnownCaldav`], except a 3xx that could not be
///   followed (redirect following disabled, unresolvable `Location`, or an
///   https→http downgrade), which fails with a descriptive error instead.
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
/// [`Error::UnexpectedStatus`] with [`Operation::DiscoverWellKnownCarddav`]
/// (a 3xx that could not be followed fails with a descriptive error
/// instead — see [`discover_caldav`]).
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
