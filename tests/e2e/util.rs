use std::sync::atomic::{AtomicU64, Ordering};

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

pub fn unique_uid(prefix: &str) -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{}@example.com",
        prefix,
        chrono::Utc::now().timestamp_micros(),
        counter
    )
}
