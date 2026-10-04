//! Typed access control list (ACL) request bodies (RFC 3744).
//!
//! [`build_acl_body`] renders a list of [`Ace`]s into the XML body of the
//! WebDAV `ACL` method (RFC 3744 §8.1), which
//! [`WebDavClient::acl`](crate::webdav::WebDavClient::acl) sends. ACEs carry
//! typed [`Privilege`]s (RFC 3744 §3, plus the CalDAV `read-free-busy`
//! extension, RFC 4791 §6.1.1) and one of the RFC 3744 §5.2 principals
//! exposed as [`AcePrincipal`].
//!
//! Privilege elements carry their RFC namespace: WebDAV privileges serialize
//! as `<D:…/>` (`DAV:`, declared on the root), while `read-free-busy`
//! serializes as `<C:read-free-busy/>` in the CalDAV namespace
//! (`urn:ietf:params:xml:ns:caldav`, RFC 4791 §6.1.1), declared as `xmlns:C`
//! on the root `<D:acl>` element.
//!
//! # Example
//! ```no_run
//! use fast_dav_rs::webdav::acl::{Ace, AcePrincipal, build_acl_body};
//! use fast_dav_rs::webdav::{Privilege, WebDavClient};
//! use fast_dav_rs::Result;
//!
//! # async fn example(client: &WebDavClient) -> Result<()> {
//! let body = build_acl_body(&[Ace {
//!     principal: AcePrincipal::Href("/principals/users/bob/".into()),
//!     grant: vec![Privilege::Read],
//!     deny: Vec::new(),
//!     protected: false,
//! }])?;
//! client
//!     .acl("principals/users/test/calendar-proxy-read/", &body)
//!     .await?;
//! # Ok(())
//! # }
//! ```

use crate::webdav::types::Privilege;
use crate::webdav::xml::escape_xml;
use crate::{Error, Result};

/// The principal an access control entry applies to (RFC 3744 §5.2).
///
/// The href variant identifies one principal by its URI; the other variants
/// map to the RFC 3744 §5.2.1-§5.2.3 predefined principals.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AcePrincipal {
    /// The principal identified by the given href (RFC 3744 §5.2.1).
    Href(String),
    /// The principal of the request itself (`<D:self/>`, RFC 3744 §5.2.3).
    SelfPrincipal,
    /// Every unauthenticated principal (`<D:unauthenticated/>`, RFC 3744
    /// §5.2.2).
    Unauthenticated,
    /// Every principal (`<D:all/>`, RFC 3744 §5.2.2).
    All,
}

/// One access control entry (RFC 3744 §5.3): privileges granted to, or
/// denied from, a [`AcePrincipal`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ace {
    /// Principal the entry applies to.
    pub principal: AcePrincipal,
    /// Privileges granted to the principal (`<D:grant>`).
    pub grant: Vec<Privilege>,
    /// Privileges denied to the principal (`D:deny`).
    pub deny: Vec<Privilege>,
    /// Whether the entry is protected (the `<D:protected/>` element, RFC
    /// 3744 §5.5): servers must not let inherited ACLs override it.
    pub protected: bool,
}

/// Map a typed [`Privilege`] to its XML namespace prefix and element local
/// name. WebDAV privileges (RFC 3744 §3) live in `DAV:` (prefix `D`); the
/// CalDAV `read-free-busy` extension lives in
/// `urn:ietf:params:xml:ns:caldav` (prefix `C`, RFC 4791 §6.1.1).
/// `Privilege::Other(name)` passes through as a `D:` element when the name
/// is a conforming lowercase element local name, `None` otherwise.
fn privilege_xml_parts(privilege: &Privilege) -> Option<(&'static str, &str)> {
    match privilege {
        Privilege::Read => Some(("D", "read")),
        Privilege::Write => Some(("D", "write")),
        Privilege::WriteProperties => Some(("D", "write-properties")),
        Privilege::WriteContent => Some(("D", "write-content")),
        Privilege::Bind => Some(("D", "bind")),
        Privilege::Unbind => Some(("D", "unbind")),
        Privilege::Unlock => Some(("D", "unlock")),
        Privilege::All => Some(("D", "all")),
        Privilege::ReadFreeBusy => Some(("C", "read-free-busy")),
        Privilege::Other(name) => {
            if !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
                Some(("D", name))
            } else {
                None
            }
        }
    }
}

/// Serialize one privilege into
/// `<D:privilege><{prefix}:{name}/></D:privilege>`.
fn append_privilege(body: &mut String, privilege: &Privilege, context: &str) -> Result<()> {
    let (prefix, name) = privilege_xml_parts(privilege).ok_or_else(|| {
        Error::InvalidInput(format!(
            "cannot serialize {context} privilege: unrecognized privilege element name"
        ))
    })?;
    body.push_str("<D:privilege><");
    body.push_str(prefix);
    body.push(':');
    body.push_str(name);
    body.push_str("/></D:privilege>");
    Ok(())
}

/// Render one [`Ace`] into its `<D:ace>` XML fragment (RFC 3744 §8.1.1).
fn append_ace(body: &mut String, ace: &Ace) -> Result<()> {
    // RFC 3744 §2.3: an ACE either grants or denies a given privilege, and
    // the DTD requires at least one of `<D:grant>`/`<D:deny>` per ACE.
    if ace.grant.is_empty() && ace.deny.is_empty() {
        return Err(Error::InvalidInput(
            "an ACE must grant or deny at least one privilege (RFC 3744 §8.1.1)".to_string(),
        ));
    }
    for denied in &ace.deny {
        if ace.grant.contains(denied) {
            return Err(Error::InvalidInput(format!(
                "an ACE must not grant and deny the same privilege ({denied:?}); RFC 3744 §2.3 forbids conflicting ACEs"
            )));
        }
    }

    body.push_str("<D:ace>");
    match &ace.principal {
        AcePrincipal::Href(href) => {
            if href.trim().is_empty() {
                return Err(Error::InvalidInput(
                    "ACE principal href must not be empty".to_string(),
                ));
            }
            body.push_str("<D:principal><D:href>");
            body.push_str(&escape_xml(href));
            body.push_str("</D:href></D:principal>");
        }
        AcePrincipal::SelfPrincipal => body.push_str("<D:principal><D:self/></D:principal>"),
        AcePrincipal::Unauthenticated => {
            body.push_str("<D:principal><D:unauthenticated/></D:principal>");
        }
        AcePrincipal::All => body.push_str("<D:principal><D:all/></D:principal>"),
    }

    if !ace.grant.is_empty() {
        body.push_str("<D:grant>");
        for privilege in &ace.grant {
            append_privilege(body, privilege, "granted")?;
        }
        body.push_str("</D:grant>");
    }
    if !ace.deny.is_empty() {
        body.push_str("<D:deny>");
        for privilege in &ace.deny {
            append_privilege(body, privilege, "denied")?;
        }
        body.push_str("</D:deny>");
    }
    if ace.protected {
        body.push_str("<D:protected/>");
    }
    body.push_str("</D:ace>");
    Ok(())
}

/// Build the XML body of an `ACL` request (RFC 3744 §8.1) from typed ACEs.
///
/// The result is a `<D:acl>` document carrying one `<D:ace>` per entry
/// (RFC 3744 §8.1.1), ready for
/// [`WebDavClient::acl`](crate::webdav::WebDavClient::acl). The root element
/// declares both namespaces privileges live in: `xmlns:D` → `DAV:` and
/// `xmlns:C` → `urn:ietf:params:xml:ns:caldav` — the CalDAV namespace carries
/// `read-free-busy`, serialized as `<C:read-free-busy/>` per RFC 4791 §6.1.1.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`] before any serialization when:
/// - `aces` is empty (an `ACL` body needs at least one entry),
/// - an ACE neither grants nor denies a privilege (RFC 3744 §8.1.1 DTD),
/// - an ACE grants and denies the same privilege (RFC 3744 §2.3),
/// - an href principal is empty, or
/// - a privilege cannot be serialized to an XML element name (e.g. a
///   `Privilege::Other` name that is not a lowercase element local name).
///
/// # Example
/// ```no_run
/// use fast_dav_rs::webdav::acl::{Ace, AcePrincipal, build_acl_body};
/// use fast_dav_rs::webdav::Privilege;
///
/// let body = build_acl_body(&[Ace {
///     principal: AcePrincipal::Href("/principals/users/bob/".into()),
///     grant: vec![Privilege::Read],
///     deny: Vec::new(),
///     protected: false,
/// }])
/// .unwrap();
/// assert!(body.contains("<D:grant><D:privilege><D:read/></D:privilege></D:grant>"));
/// ```
pub fn build_acl_body(aces: &[Ace]) -> Result<String> {
    if aces.is_empty() {
        return Err(Error::InvalidInput(
            "an ACL body requires at least one access control entry (ace)".to_string(),
        ));
    }
    let mut body =
        String::from("<D:acl xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">");
    for ace in aces {
        append_ace(&mut body, ace)?;
    }
    body.push_str("</D:acl>");
    Ok(body)
}

/// Validate a principal href for use in an ACL ACE and return it trimmed.
///
/// Accepts full URLs (`https://dav.example.com/principals/users/bob/`) as
/// well as server-relative paths (`/principals/users/bob/`) — hrefs are sent
/// verbatim inside `<D:href>`, exactly as the server returned them. The
/// CalDAV calendar-proxy helpers use this before any network I/O.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`] when the href is empty or whitespace-only.
pub fn principal_href_for_acl(principal_url: &str) -> Result<String> {
    let trimmed = principal_url.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput(
            "principal href must not be empty".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_xml_parts_maps_namespaces_and_validates_other() {
        assert_eq!(privilege_xml_parts(&Privilege::Read), Some(("D", "read")));
        assert_eq!(
            privilege_xml_parts(&Privilege::ReadFreeBusy),
            Some(("C", "read-free-busy"))
        );
        assert_eq!(
            privilege_xml_parts(&Privilege::Other("all".into())),
            Some(("D", "all"))
        );
        assert_eq!(privilege_xml_parts(&Privilege::Other("Read".into())), None);
        assert_eq!(privilege_xml_parts(&Privilege::Other(String::new())), None);
    }

    #[test]
    fn principal_href_for_acl_trims_and_validates() {
        assert_eq!(
            principal_href_for_acl("  /principals/users/bob/  ").unwrap(),
            "/principals/users/bob/"
        );
        assert!(principal_href_for_acl("   ").is_err());
    }
}
