//! Turning a request that came through `tailscale serve` into a person.
//!
//! `tailscale serve` is the only ingress, and it proxies from the local machine
//! to the loopback port this server binds. A request therefore carries three
//! headers naming the tailnet user. Those headers are only meaningful when the
//! socket peer is loopback — that is the trust boundary: anything that can reach
//! the loopback port can already read the database.
//!
//! When the proxy also forwards `X-Forwarded-For`, the claimed identity is
//! checked against `tailscale whois` for that address, so a forged header from a
//! local process that *does* forward an address is refused rather than believed.

use crate::auth::Identity;
use axum::http::HeaderMap;
use std::net::{IpAddr, SocketAddr};

/// Set by `tailscale serve` on every proxied request.
pub const LOGIN_HEADER: &str = "tailscale-user-login";
pub const NAME_HEADER: &str = "tailscale-user-name";
pub const AVATAR_HEADER: &str = "tailscale-user-profile-pic";

/// A tailnet identity presented by one request.
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

/// The address the proxy reported, if it forwarded one.
///
/// Only the rightmost hop is used: it is the address `tailscaled` appended, and
/// the client cannot shorten it.
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

/// Whether `whois` agrees that `client` is `login`.
///
/// `None` means whois could not answer — a peer that is not on the tailnet (a
/// local test request) or a machine without the CLI. The caller decides how much
/// to trust the header in that case; this function does not guess.
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

    /// The header is only meaningful behind the local proxy. A remote peer
    /// cannot claim an identity by sending it.
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
