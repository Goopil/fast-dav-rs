//! E2E tests against the Radicale fixture (`radicale-test/`).
//!
//! Bring the fixture up first: `./radicale-test/setup.sh` (http://localhost:8081,
//! override with `RADICALE_URL`; Basic auth `test`/`test`).

#[path = "../util.rs"]
pub(crate) mod util;

mod attachments;
mod core;
mod locking;
mod sync;
