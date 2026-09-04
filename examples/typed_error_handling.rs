//! Matching on the typed [`Error`] enum: `UnexpectedStatus` (with its
//! `Operation`), `UnexpectedStatusWithDav`, `PrincipalNotFound`,
//! `TokenRefresh` (with its reason), `InvalidEtag`, `Timeout`, and the
//! mandatory `#[non_exhaustive]` wildcard arm.
//!
//! No fixture needed — the classification logic runs on constructed errors,
//! plus one *real* `TokenRefresh` produced by pointing `OAuth2RefreshProvider`
//! at an unreachable token endpoint (connection refused, no server involved).
//!
//! ```sh
//! cargo run --example typed_error_handling   # runs offline
//! ```

use std::time::Duration;

use fast_dav_rs::webdav::{OAuth2RefreshProvider, TokenProvider, WebDavClient};
use fast_dav_rs::{Error, EtagReason, Operation, TokenRefreshReason};

/// Turn any error into an actionable one-liner. The wildcard arm is
/// mandatory: `Error` is `#[non_exhaustive]`, so new variants can appear in
/// minor releases without breaking your build.
fn classify(err: &Error) -> String {
    match err {
        // Which DAV call failed, and with what HTTP status. The `Operation`
        // discriminates "PROPFIND principal" from "REPORT multiget", "LOCK",
        // … without string parsing.
        Error::UnexpectedStatus {
            operation, status, ..
        } => match (operation, *status) {
            (Operation::Lock, hyper::StatusCode::METHOD_NOT_ALLOWED) => {
                "server has no LOCK support — fall back to etag-conditional writes".to_owned()
            }
            (op, code) => format!("{op} answered {code}"),
        },
        // Same shape, but the server sent a <D:error> precondition body
        // (e.g. 423 Locked + no-conflicting-lock).
        Error::UnexpectedStatusWithDav {
            operation,
            status,
            dav,
            ..
        } => format!(
            "{operation} answered {status}: {}",
            dav.precondition_code
                .as_deref()
                .unwrap_or("(no precondition reported)")
        ),
        // Auth succeeded but no principal exists: on some providers this
        // means the username form is wrong (email vs. internal account ID).
        Error::PrincipalNotFound { url, .. } => {
            format!("no principal at {url} — check the username form for this provider")
        }
        // The OAuth2 refresh grant failed; the reason says why, and neither
        // the error text nor the source chain ever contains a token.
        Error::TokenRefresh { reason, .. } => match reason {
            TokenRefreshReason::Rejected => {
                "refresh grant rejected — re-consent or re-issue the refresh token".to_owned()
            }
            TokenRefreshReason::MalformedResponse => {
                "token endpoint answered garbage — check the endpoint URL".to_owned()
            }
            TokenRefreshReason::Transport => {
                "token endpoint unreachable — network or outage".to_owned()
            }
            _ => "refresh failed for another reason".to_owned(),
        },
        Error::InvalidEtag { reason, .. } => match reason {
            EtagReason::Weak => {
                "weak etag cannot guard If-Match (RFC 9110 strong comparison)".to_owned()
            }
            other => format!("etag rejected: {other}"),
        },
        Error::Timeout { limit, .. } => format!(
            "gave up after {}s — retry or raise the builder timeout",
            limit.as_secs()
        ),
        // Connection vs Transport: a failed connect is retryable as-is; a
        // Transport error may have consumed the request (idempotency!).
        Error::Connection(_) => "could not reach the server at all".to_owned(),
        Error::Transport(_) => "request broken mid-flight — resend only if idempotent".to_owned(),
        // Everything else, present and future.
        other => format!("unclassified: {other}"),
    }
}

#[tokio::main]
async fn main() -> fast_dav_rs::Result<()> {
    // Errors you can construct without a server (public constructors):
    let constructed: Vec<Error> = vec![
        Error::unexpected_status(Operation::Lock, hyper::StatusCode::METHOD_NOT_ALLOWED),
        Error::unexpected_status(
            Operation::ReportCalendarMultiget,
            hyper::StatusCode::NOT_FOUND,
        ),
        Error::principal_not_found("https://dav.example.com/"),
        Error::invalid_etag(EtagReason::Weak),
        Error::timeout(Duration::from_secs(30)),
    ];
    for err in &constructed {
        println!("- {err}\n  -> {}", classify(err));
    }

    // A *real* TokenRefresh from a dead token endpoint (connection refused).
    let provider = OAuth2RefreshProvider::new(
        "http://127.0.0.1:1/oauth2/token", // port 1: nothing listens here
        "client-id",
        "client-secret",
        "refresh-token",
    )?;
    match TokenProvider::token(&provider).await {
        Ok(_) => println!("unexpected: the dead endpoint answered"),
        Err(err) => println!("- {err}\n  -> {}", classify(&err)),
    }

    // The same classification applies to live client errors:
    let client = WebDavClient::new("http://127.0.0.1:1/", None, None)?;
    match client.get("anything").await {
        Ok(_) => println!("unexpected: the dead endpoint answered"),
        Err(err) => println!("- {err}\n  -> {}", classify(&err)),
    }
    Ok(())
}
