//! Retry with exponential backoff for transient failures (`429`, `503`, `504`).
//!
//! Integrated in the shared request pipeline
//! ([`WebDavClient::build_and_send`](crate::webdav::client::WebDavClient)):
//! when a response arrives with a retryable status, retry budget remains,
//! and the method is retryable per the configured policy, the request is
//! re-sent after a delay. A `429` honors the server's `Retry-After` header
//! (integer seconds or HTTP-date); every other case uses an exponential
//! backoff (base 2, initial 250 ms, capped at 8 s) with ±25 % jitter.

use hyper::{HeaderMap, Method, StatusCode, header};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Initial delay of the exponential backoff (doubled per attempt).
pub(crate) const RETRY_BACKOFF_INITIAL: Duration = Duration::from_millis(250);
/// Upper bound for the exponential backoff delay.
pub(crate) const RETRY_BACKOFF_CAP: Duration = Duration::from_secs(8);

/// Retryable transient statuses: `429 Too Many Requests`, `503 Service
/// Unavailable`, `504 Gateway Timeout`.
#[doc(hidden)]
pub fn is_retryable_status(status: StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 503 | 504)
}

/// Methods retried by the default (idempotent-only) policy.
const IDEMPOTENT_METHODS: [&str; 5] = ["GET", "HEAD", "OPTIONS", "PROPFIND", "REPORT"];

/// True for the methods the default policy considers idempotent and safe to
/// re-send automatically (`GET`, `HEAD`, `OPTIONS`, `PROPFIND`, `REPORT`).
#[doc(hidden)]
pub fn is_idempotent_method(method: &Method) -> bool {
    IDEMPOTENT_METHODS.contains(&method.as_str())
}

/// Delay to wait before the next attempt.
///
/// On `429`, the `Retry-After` header is honored verbatim when parseable
/// (integer seconds or HTTP-date); otherwise — and for `503`/`504` — an
/// exponential backoff with jitter is used.
pub(crate) fn retry_delay(
    status: StatusCode,
    headers: &HeaderMap,
    attempt: usize,
    initial: Duration,
    cap: Duration,
) -> Duration {
    if status == StatusCode::TOO_MANY_REQUESTS {
        if let Some(value) = headers
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
        {
            if let Some(delay) = retry_after_delay(value) {
                return delay;
            }
        }
    }
    backoff_delay(attempt, initial, cap)
}

/// Exponential backoff for `attempt` (0-based): `initial * 2^attempt`,
/// capped, with ±25 % hash-based jitter applied before the cap.
#[doc(hidden)]
pub fn backoff_delay(attempt: usize, initial: Duration, cap: Duration) -> Duration {
    let mut delay = initial;
    for _ in 0..attempt {
        delay = delay.saturating_mul(2);
        if delay >= cap {
            break;
        }
    }
    let jittered = delay.mul_f64(jitter_ratio(jitter_seed(attempt)));
    jittered.min(cap)
}

// ponytail: hash-based jitter (FNV-1a over attempt index + sub-second clock
// nanos) instead of a `rand` dependency — the only goal is spreading
// concurrent retries apart, not statistical quality. Upgrade path: `rand`.
fn jitter_seed(attempt: usize) -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()));
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for byte in (attempt as u64)
        .to_le_bytes()
        .into_iter()
        .chain(nanos.to_le_bytes())
    {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// Jitter factor in `[0.75, 1.25]` derived from the seed.
fn jitter_ratio(seed: u64) -> f64 {
    0.75 + f64::from((seed % 501) as u32) / 1000.0
}

/// Delay instructed by a `Retry-After` header value, if parseable: either an
/// integer number of seconds or an HTTP-date (IMF-fixdate, RFC 9110 §5.6.7).
/// A date in the past yields [`Duration::ZERO`] (retry immediately).
/// Unparseable values yield `None` (caller falls back to exponential
/// backoff).
#[doc(hidden)]
pub fn retry_after_delay(value: &str) -> Option<Duration> {
    if let Ok(secs) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let unix = parse_http_date(value)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(Duration::from_secs((unix - now).max(0) as u64))
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Parse an IMF-fixdate HTTP-date (`Sun, 06 Nov 1994 08:49:37 GMT`) into a
/// Unix timestamp in seconds. The timezone is assumed GMT (modern servers
/// always send `GMT`; obsolete RFC 850 / asctime formats are not supported).
#[doc(hidden)]
pub fn parse_http_date(value: &str) -> Option<i64> {
    let rest = value.trim().split_once(',')?.1.trim_start();
    let mut parts = rest.split_ascii_whitespace();
    let day: i64 = parts.next()?.parse().ok()?;
    let mon = parts.next()?;
    let month = MONTHS.iter().position(|m| mon.eq_ignore_ascii_case(m))? as i64 + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    let mut hms = parts.next()?.split(':');
    let h: i64 = hms.next()?.parse().ok()?;
    let m: i64 = hms.next()?.parse().ok()?;
    let s: i64 = hms.next()?.parse().ok()?;
    if !(1..=31).contains(&day) || h > 23 || m > 59 || s > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + h * 3_600 + m * 60 + s)
}

/// Days since 1970-01-01 for a proleptic Gregorian date
/// (Howard Hinnant's `days_from_civil` algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
