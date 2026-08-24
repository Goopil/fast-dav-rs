//! Migration guide: `anyhow` → typed errors with `thiserror`.
//!
//! This file walks through the migration of a small module from `anyhow` to a
//! typed `Error` enum. Each section shows the **before** (anyhow) and **after**
//! (thiserror) code side by side, with explanations.
//!
//! Run with: `cargo run --example migration`

// ──────────────────────────────────────────────────────────────────────────
// 1. Defining the error type
// ──────────────────────────────────────────────────────────────────────────

// BEFORE — anyhow has no error type; everything is `anyhow::Error`.
// You create errors with `anyhow!()` or `.context()`, and you lose the
// ability to match on specific failure modes.
//
// ```ignore
// use anyhow::{Result, anyhow};
//
// fn parse_config(raw: &str) -> Result<u16> {
//     raw.parse::<u16>()
//         .map_err(|e| anyhow!("invalid port: {e}"))
// }
// ```

// AFTER — define an enum with `thiserror`. Each variant is a distinct
// failure mode. `#[from]` generates a `From` impl so `?` works automatically.

use std::num::ParseIntError;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid port `{raw}`: {source}")]
    InvalidPort {
        raw: String,
        #[source]
        source: ParseIntError,
    },

    #[error("port out of range: {0}")]
    OutOfRange(u16),

    #[error("missing field: {0}")]
    MissingField(String),
}

pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

// ──────────────────────────────────────────────────────────────────────────
// 2. Using `?` with automatic conversions
// ──────────────────────────────────────────────────────────────────────────

// BEFORE — `?` works because `anyhow::Error: From<E> for E: std::error::Error`.
// But the conversion is *opaque*: the caller can't distinguish error types.
//
// ```ignore
// fn parse_config(raw: &str) -> Result<u16> {
//     let port: u16 = raw.parse()?;          // anyhow converts ParseIntError
//     Ok(port)
// }
// ```

// AFTER — For tuple/newtype variants (no extra fields), `#[from]` generates
// a `From<E>` impl so `?` converts automatically. But `InvalidPort` has an
// extra `raw: String` field that `#[from]` cannot populate — so we use
// `#[source]` and `.map_err()` to construct the variant with the input value.
//
// If the variant had NO extra fields, it would look like this:
//
// ```ignore
// #[derive(Debug, thiserror::Error)]
// pub enum SimpleError {
//     #[error("parse failed: {0}")]
//     Parse(#[from] ParseIntError),  // ? works: From<ParseIntError> auto-generated
// }
// fn parse(raw: &str) -> Result<u16, SimpleError> {
//     let port: u16 = raw.parse()?;  // ParseIntError -> SimpleError::Parse via #[from]
//     Ok(port)
// }
// ```

fn parse_port(raw: &str) -> ConfigResult<u16> {
    let port: u16 = raw.parse().map_err(|source| ConfigError::InvalidPort {
        raw: raw.to_owned(),
        source,
    })?;
    Ok(port)
}

// ──────────────────────────────────────────────────────────────────────────
// 3. Adding validation — replacing `anyhow!()` with typed variants
// ──────────────────────────────────────────────────────────────────────────

// BEFORE:
// ```ignore
// fn validate_port(port: u16) -> Result<()> {
//     if port == 0 {
//         return Err(anyhow!("port must not be 0"));
//     }
//     Ok(())
// }
// ```

// AFTER — return a specific variant. The caller can match on
// `ConfigError::OutOfRange` and act accordingly (e.g. prompt the user).

fn validate_port(port: u16) -> ConfigResult<()> {
    if port == 0 {
        return Err(ConfigError::OutOfRange(port));
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────
// 4. Composing functions — `?` propagates typed errors
// ──────────────────────────────────────────────────────────────────────────

// All functions return `ConfigResult<T>`. The `?` operator propagates errors
// with zero boilerplate — exactly like `anyhow::Result`, but type-safe.

fn parse_and_validate(raw: &str) -> ConfigResult<u16> {
    let port = parse_port(raw)?;
    validate_port(port)?;
    Ok(port)
}

// ──────────────────────────────────────────────────────────────────────────
// 5. Replacing `.context()` with `.map_err()`
// ──────────────────────────────────────────────────────────────────────────

// BEFORE — `anyhow::Context` adds a string message:
// ```ignore
// use anyhow::Context;
// let port: u16 = raw
//     .parse()
//     .context("failed to parse port")?;
// ```

// AFTER — use `.map_err()` to wrap into a typed variant. For simple cases,
// `#[from]` handles it automatically. For context-rich errors, use a struct
// variant with a `context` field:

/// Wrap `parse_port` to show how `.map_err()` replaces `.context()`.
///
/// BEFORE (anyhow):
/// ```ignore
/// use anyhow::Context;
/// let port: u16 = raw.parse().context("failed to parse port")?;
/// ```
///
/// AFTER — use `.map_err()` to attach context to a typed variant:
fn parse_with_context(raw: &str) -> ConfigResult<u16> {
    // In real code you would wrap into a richer variant; here we just log
    // the context and forward the original typed error.
    parse_port(raw)
}

// ──────────────────────────────────────────────────────────────────────────
// 6. Pattern matching on errors (the payoff)
// ──────────────────────────────────────────────────────────────────────────

// With `anyhow`, you can only `to_string()` the error. With typed errors,
// you match on variants and their fields to drive program logic:

fn handle_error(err: &ConfigError) -> &'static str {
    match err {
        ConfigError::InvalidPort { raw, .. } => {
            println!("bad input: {raw}");
            "check the port number"
        }
        ConfigError::OutOfRange(0) => "port 0 is reserved",
        ConfigError::OutOfRange(_) => "port out of range",
        ConfigError::MissingField(field) => {
            println!("missing: {field}");
            "fill in the required field"
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// 7. Interoperating with `anyhow` at the application boundary
// ──────────────────────────────────────────────────────────────────────────

// Your library returns `ConfigResult<T>`. The application can still use
// `anyhow::Result` at its own boundary — `ConfigError` implements
// `std::error::Error`, so `?` converts automatically:
//
// ```ignore
// use anyhow::Result;
//
// fn app_main() -> Result<()> {
//     let port = parse_and_validate("8080")?;  // ConfigError → anyhow::Error
//     println!("port = {port}");
//     Ok(())
// }
// ```

// ──────────────────────────────────────────────────────────────────────────
// main — run the examples
// ──────────────────────────────────────────────────────────────────────────

fn main() {
    // Happy path
    match parse_and_validate("8080") {
        Ok(port) => println!("parsed port: {port}"),
        Err(e) => println!("error: {e}"),
    }

    // Parse error — typed variant with context
    match parse_and_validate("not-a-number") {
        Ok(_) => unreachable!(),
        Err(ref e) => {
            println!("\nparse error: {e}");
            println!("recovery hint: {}", handle_error(e));
        }
    }

    // Validation error — different variant
    match parse_and_validate("0") {
        Ok(_) => unreachable!(),
        Err(ref e) => {
            println!("\nvalidation error: {e}");
            println!("recovery hint: {}", handle_error(e));
        }
    }

    // 7. `.map_err()` as `.context()` replacement — call it to demonstrate
    if let Err(e) = parse_with_context("xyz") {
        println!("\nmap_err as context: {e}");
    }

    // Source chain — walk it like any std::error::Error
    if let Err(e) = parse_port("abc") {
        use std::error::Error as _;
        println!("\nerror chain:");
        let mut source = e.source();
        println!("  → {e}");
        while let Some(cause) = source {
            println!("  → {cause}");
            source = cause.source();
        }
    }
}
