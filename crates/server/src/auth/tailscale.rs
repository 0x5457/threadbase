//! Reading the tailnet user from a request proxied by `tailscale serve`.
//!
//! `tailscale serve` proxies to the loopback port and adds three headers naming
//! the user. These are only accepted when the connection comes from loopback,
//! i.e. from the local proxy. Anything that can reach the loopback port can
//! already read the database, so that is the limit of what a header check buys.
//!
//! If the proxy also forwards `X-Forwarded-For`, the claimed login is checked
//! against `tailscale whois` for that address. A mismatch is rejected.

use crate::auth::Identity;
use axum::http::HeaderMap;
use std::net::{IpAddr, SocketAddr};

/// Added by `tailscale serve` to every proxied request.
pub const LOGIN_HEADER: &str = "tailscale-user-login";
pub const NAME_HEADER: &str = "tailscale-user-name";
pub const AVATAR_HEADER: &str = "tailscale-user-profile-pic";

/// A tailnet identity claimed by one request.
#[derive(Debug, Clone)]
pub struct Presentment {
    pub identity: Identity,
    /// The tailnet address behind the proxy, when the proxy forwarded one.
    pub client: Option<IpAddr>,
}

/// Read the identity a proxied request presents, if any.
#[must_use]
pub fn present(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<Presentment> {
    if !peer.is_some_and(|addr| addr.ip().is_loopback()) {
        return None;
    }
    let login = header(headers, LOGIN_HEADER)?;
    let display_name = header(headers, NAME_HEADER);
    let avatar = header(headers, AVATAR_HEADER);
    Some(Presentment {
        identity: Identity::tailscale(&login, display_name.as_deref(), avatar.as_deref()),
        client: client_ip(headers, peer),
    })
}

/// The client address the proxy reported, if any.
///
/// Only the rightmost hop is used: that is the one `tailscaled` appended.
#[must_use]
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    if !peer.is_some_and(|addr| addr.ip().is_loopback()) {
        return None;
    }
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .map(str::trim)
        .and_then(|hop| hop.parse().ok())
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Whether `whois` says `client` belongs to `login`.
///
/// `None` means whois could not answer: the address is not a tailnet peer, or
/// the CLI is missing.
#[must_use]
pub fn whois_agrees(client: IpAddr, login: &str) -> Option<bool> {
    crate::tailscale::whois(client).map(|profile| profile.login.eq_ignore_ascii_case(login))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn loopback() -> Option<SocketAddr> {
        Some(SocketAddr::from(([127, 0, 0, 1], 4321)))
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn a_proxied_request_names_the_person() {
        let presentment = present(
            &headers(&[
                (LOGIN_HEADER, "ada@example.com"),
                (NAME_HEADER, "Ada"),
                (AVATAR_HEADER, "https://example.com/ada.png"),
            ]),
            loopback(),
        )
        .expect("identity");
        assert_eq!(presentment.identity.key, "tailscale:ada@example.com");
        assert_eq!(presentment.identity.display_name.as_deref(), Some("Ada"));
        assert_eq!(
            presentment.identity.avatar_url.as_deref(),
            Some("https://example.com/ada.png")
        );
    }

    /// A remote peer cannot claim an identity by sending the header.
    #[test]
    fn a_remote_peer_cannot_present_an_identity() {
        let remote = Some(SocketAddr::from(([100, 64, 1, 2], 4321)));
        assert!(present(&headers(&[(LOGIN_HEADER, "ada@example.com")]), remote).is_none());
        assert!(client_ip(&headers(&[("x-forwarded-for", "100.64.1.2")]), remote).is_none());
    }

    #[test]
    fn an_empty_login_is_not_an_identity() {
        assert!(present(&headers(&[(LOGIN_HEADER, "   ")]), loopback()).is_none());
        assert!(present(&HeaderMap::new(), loopback()).is_none());
    }

    #[test]
    fn the_rightmost_forwarded_hop_is_the_client() {
        let ip = client_ip(
            &headers(&[("x-forwarded-for", "10.0.0.1, 100.64.1.2")]),
            loopback(),
        );
        assert_eq!(ip, Some("100.64.1.2".parse().unwrap()));
    }
}
