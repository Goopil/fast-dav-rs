//! CalDAV calendar-proxy support (the calendar-proxy companion spec of
//! RFC 6638, as implemented by CalendarServer, SabreDAV and friends).
//!
//! A calendar user can act as a **calendar proxy** for another user: the
//! proxy sees (read proxy) or manages (write proxy) the delegator's
//! calendars. Two pieces of state describe a delegation:
//!
//! - the delegator's principal lists its proxies through the
//!   `calendar-proxy-read-for`/`calendar-proxy-write-for` properties
//!   ([`CalDavClient::list_calendar_proxies`]);
//! - the delegator's `calendar-proxy-read`/`calendar-proxy-write` group
//!   principals list their members through `group-member-set`
//!   ([`CalDavClient::calendar_proxy_group_members`]).
//!
//! [`CalDavClient::grant_calendar_proxy`] /
//! [`CalDavClient::revoke_calendar_proxy`] change a delegation through an
//! `ACL` request on the proxy group principal.
//!
//! # Wire form caveat
//!
//! RFC 6638 itself does not define the calendar-proxy wire protocol: the
//! `calendar-proxy-read-for`/`-write-for` properties, the proxy group
//! principals, and the `ACL`/`PROPPATCH` request shapes used to change
//! delegations are a widely-implemented companion convention, not an IETF
//! standard. In particular the grant/revoke methods below issue an `ACL`
//! request whose exact acceptance semantics are **server-dependent** — some
//! servers require `PROPPATCH` of `group-member-set` instead. The e2e tests
//! against the SabreDAV fixture record the observed behavior of a real
//! server (see `tests/e2e/sabredav/caldav/proxy_tests.rs`).

use crate::Result;
use crate::caldav::streaming::parse_multistatus_bytes;
use crate::webdav::acl::{Ace, AcePrincipal, build_acl_body, principal_href_for_acl};
use crate::webdav::types::Privilege;
use crate::{CalDavClient, Depth, Error, Operation};

/// The calendar-proxy delegations of a principal (calendar-proxy companion
/// spec of RFC 6638).
///
/// Populated by
/// [`CalDavClient::list_calendar_proxies`](crate::CalDavClient::list_calendar_proxies):
/// each href is a principal for which the queried principal acts as a proxy
/// (the delegator). Empty vectors mean the server omitted the property —
/// typically because calendar-proxy support is disabled.
///
/// # Example
/// ```no_run
/// use fast_dav_rs::{CalDavClient, Result};
///
/// # async fn example() -> Result<()> {
/// let client = CalDavClient::new(
///     "https://cal.example.com/dav/",
///     Some("user01"),
///     Some("secret"),
/// )?;
/// let proxies = client.list_calendar_proxies("principals/user01/").await?;
/// println!("read proxy for: {:?}", proxies.read_for);
/// println!("write proxy for: {:?}", proxies.write_for);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CalendarProxyInfo {
    /// Principals (hrefs) for which this principal is a read proxy, from
    /// `calendar-proxy-read-for`. Sorted and de-duplicated.
    pub read_for: Vec<String>,
    /// Principals (hrefs) for which this principal is a write proxy, from
    /// `calendar-proxy-write-for`. Sorted and de-duplicated.
    pub write_for: Vec<String>,
}

/// The proxy group principal for `principal_path` (`calendar-proxy-read` or
/// `calendar-proxy-write`), keeping a single trailing slash off.
fn proxy_group_path(principal_path: &str, write: bool) -> String {
    let trimmed = principal_path.trim_end_matches('/');
    format!(
        "{trimmed}/calendar-proxy-{}",
        if write { "write" } else { "read" }
    )
}

impl CalDavClient {
    /// List the calendar-proxy delegations of a principal.
    ///
    /// Sends a `PROPFIND` (`Depth: 0`) for `calendar-proxy-read-for` and
    /// `calendar-proxy-write-for` (calendar-proxy companion spec of
    /// RFC 6638) against the principal at `principal_path` and returns the
    /// collected hrefs, sorted and de-duplicated.
    ///
    /// Servers without calendar-proxy support answer with empty properties;
    /// the returned [`CalendarProxyInfo`] then carries empty vectors (see
    /// the module docs for the wire-form caveat).
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnexpectedStatus`] with
    /// [`Operation::PropfindCalendarProxy`] when the server responds with a
    /// non-success status, and an error when the transport fails or the
    /// multistatus body cannot be parsed.
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::{CalDavClient, Result};
    ///
    /// # async fn example() -> Result<()> {
    /// let client = CalDavClient::new(
    ///     "https://cal.example.com/dav/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// let proxies = client.list_calendar_proxies("principals/user01/").await?;
    /// for delegator in &proxies.read_for {
    ///     println!("read proxy for {delegator}");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_calendar_proxies(&self, principal_path: &str) -> Result<CalendarProxyInfo> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <C:calendar-proxy-read-for/>
    <C:calendar-proxy-write-for/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(principal_path, Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindCalendarProxy,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        let mut info = CalendarProxyInfo::default();
        for mut item in parse_multistatus_bytes(&body)?.items {
            info.read_for.append(&mut item.calendar_proxy_read_for);
            info.write_for.append(&mut item.calendar_proxy_write_for);
        }
        info.read_for.sort();
        info.read_for.dedup();
        info.write_for.sort();
        info.write_for.dedup();
        Ok(info)
    }

    /// Resolve the members of a principal's calendar-proxy group.
    ///
    /// Sends a `PROPFIND` (`Depth: 0`) for `group-member-set` (RFC 3744
    /// §5.4) against the `calendar-proxy-read` (or `-write` when `write`)
    /// group principal of `principal_path` and returns the member hrefs,
    /// sorted and de-duplicated.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnexpectedStatus`] with
    /// [`Operation::PropfindCalendarProxy`] when the server responds with a
    /// non-success status (e.g. `404` when the server has no calendar-proxy
    /// support), and an error when the transport fails or the multistatus
    /// body cannot be parsed.
    pub async fn calendar_proxy_group_members(
        &self,
        principal_path: &str,
        write: bool,
    ) -> Result<Vec<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:group-member-set/>
  </D:prop>
</D:propfind>
"#;
        let resp = self
            .propfind(&proxy_group_path(principal_path, write), Depth::Zero, body)
            .await?;
        if !resp.status().is_success() {
            return Err(Error::UnexpectedStatus {
                operation: Operation::PropfindCalendarProxy,
                status: resp.status(),
            });
        }
        let body = resp.into_body();
        let mut members = Vec::new();
        for mut item in parse_multistatus_bytes(&body)?.items {
            members.append(&mut item.group_member_set);
        }
        members.sort();
        members.dedup();
        Ok(members)
    }

    /// Grant a delegate calendar-proxy rights on a principal.
    ///
    /// Resolves the proxy group's current `group-member-set`, then issues an
    /// `ACL` request (RFC 3744 §8.1) on the `calendar-proxy-read` (or
    /// `-write` when `write`) group principal granting `read` (resp.
    /// `write`) to `delegate_href` — keeping an ACE for every existing
    /// member so no current delegation is clobbered.
    ///
    /// # Wire form caveat
    ///
    /// The ACL-based wire form is **server-dependent** (see the module
    /// docs): RFC 6638 does not standardize how delegations are written, and
    /// servers that expect `PROPPATCH` of `group-member-set` reject the
    /// `ACL` request. The e2e suite records the observed behavior of the
    /// SabreDAV fixture instead of assuming conformance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] **before any network I/O** when
    /// `delegate_href` is empty. Returns
    /// [`Error::UnexpectedStatus`] with [`Operation::PropfindCalendarProxy`]
    /// when the member lookup fails, with [`Operation::Acl`] when the `ACL`
    /// request fails, and an error when the transport itself fails.
    pub async fn grant_calendar_proxy(
        &self,
        principal_path: &str,
        delegate_href: &str,
        write: bool,
    ) -> Result<()> {
        let delegate = principal_href_for_acl(delegate_href)?;
        let privilege = if write {
            Privilege::Write
        } else {
            Privilege::Read
        };
        let group_path = proxy_group_path(principal_path, write);
        let members = self
            .calendar_proxy_group_members(principal_path, write)
            .await?;

        let mut aces: Vec<Ace> = members
            .iter()
            .filter(|member| *member != &delegate)
            .map(|member| Ace {
                principal: AcePrincipal::Href(member.clone()),
                grant: vec![privilege.clone()],
                deny: Vec::new(),
                protected: false,
            })
            .collect();
        aces.push(Ace {
            principal: AcePrincipal::Href(delegate),
            grant: vec![privilege],
            deny: Vec::new(),
            protected: false,
        });
        let body = build_acl_body(&aces)?;
        self.acl(&group_path, &body).await?;
        Ok(())
    }

    /// Revoke a delegate's calendar-proxy rights on a principal.
    ///
    /// Resolves the proxy group's current `group-member-set`, then issues an
    /// `ACL` request (RFC 3744 §8.1) on the `calendar-proxy-read` (or
    /// `-write` when `write`) group principal granting `read` (resp.
    /// `write`) to every remaining member — omitting `delegate_href`, whose
    /// access therefore ends. When no member remains, the body is an empty
    /// `<D:acl>` (zero ACEs is schema-valid per RFC 3744 §5.5 and clears the
    /// explicitly granted access).
    ///
    /// # Wire form caveat
    ///
    /// The ACL-based wire form is **server-dependent** (see the module
    /// docs); the e2e suite records the observed behavior of the SabreDAV
    /// fixture instead of assuming conformance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] **before any network I/O** when
    /// `delegate_href` is empty. Returns
    /// [`Error::UnexpectedStatus`] with [`Operation::PropfindCalendarProxy`]
    /// when the member lookup fails, with [`Operation::Acl`] when the `ACL`
    /// request fails, and an error when the transport itself fails.
    pub async fn revoke_calendar_proxy(
        &self,
        principal_path: &str,
        delegate_href: &str,
        write: bool,
    ) -> Result<()> {
        let delegate = principal_href_for_acl(delegate_href)?;
        let privilege = if write {
            Privilege::Write
        } else {
            Privilege::Read
        };
        let group_path = proxy_group_path(principal_path, write);
        let members = self
            .calendar_proxy_group_members(principal_path, write)
            .await?;

        let remaining: Vec<&String> = members.iter().filter(|m| *m != &delegate).collect();
        let body = if remaining.is_empty() {
            // Zero ACEs is valid per the RFC 3744 §5.5 DTD (`<!ELEMENT acl
            // (ace*)>`); `build_acl_body` requires at least one entry, so the
            // empty document is built inline here.
            "<D:acl xmlns:D=\"DAV:\"></D:acl>".to_string()
        } else {
            let aces: Vec<Ace> = remaining
                .into_iter()
                .map(|member| Ace {
                    principal: AcePrincipal::Href(member.clone()),
                    grant: vec![privilege.clone()],
                    deny: Vec::new(),
                    protected: false,
                })
                .collect();
            build_acl_body(&aces)?
        };
        self.acl(&group_path, &body).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_group_path_normalizes_trailing_slash() {
        assert_eq!(
            proxy_group_path("principals/users/test/", false),
            "principals/users/test/calendar-proxy-read"
        );
        assert_eq!(
            proxy_group_path("principals/users/test", true),
            "principals/users/test/calendar-proxy-write"
        );
    }
}
