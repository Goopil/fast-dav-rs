//! E2E tests against the Nextcloud fixture (`nextcloud-test/`).
//!
//! Bring the fixture up first: `./nextcloud-test/setup.sh` (http://localhost:8083,
//! override with `NEXTCLOUD_URL`; Basic auth `test`/`fixture-dav-password`).
//! All DAV paths live under the Nextcloud standard `/remote.php/dav/` tree.

#[path = "../util.rs"]
pub(crate) mod util;

mod crud;
mod discovery;
mod locking;
mod sync;
