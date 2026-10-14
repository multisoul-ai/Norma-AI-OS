//! Forwarding trust must originate from the accepted TCP connection, never HTTP headers.
use axum::http::HeaderMap;
use std::net::{IpAddr, SocketAddr};

/// Implementations must correlate the complete socket tuple with its owning process
/// and verify the Tailscale executable. Lookup failure/ambiguity must return false.
/// Process names or a running Tailscale process alone are insufficient evidence.
pub(super) trait PeerProcessResolver {
    fn is_tailscale_proxy(&self, peer: SocketAddr, local: SocketAddr) -> bool;
}

/// R8 fallback on every platform until an unprivileged macOS resolver is validated.
/// libproc FD enumeration can deny access to root-owned/sandboxed extensions;
/// accepting headers without reliable socket ownership would let local clients split buckets.
pub(super) struct UnverifiedPeer;
impl PeerProcessResolver for UnverifiedPeer {
    fn is_tailscale_proxy(&self, _peer: SocketAddr, _local: SocketAddr) -> bool {
        false
    }
}

pub(super) fn source_ip(
    peer: SocketAddr,
    local: SocketAddr,
    resolver: &dyn PeerProcessResolver,
    host: &str,
    headers: &axum::http::HeaderMap,
    tunnel: Option<&workspace::HostAuthority>,
) -> IpAddr {
    // Headers and Host are forgeable by any local process. Verify the socket owner first.
    if peer.ip().is_loopback()
        && resolver.is_tailscale_proxy(peer, local)
        && tunnel.is_some_and(|t| t.as_str().eq_ignore_ascii_case(host))
        && single_header(headers, "tailscale-funnel-request") == Some("?1")
    {
        if let Some(ip) = single_header(headers, "x-forwarded-for").and_then(|v| v.parse().ok()) {
            return ip;
        }
    }
    peer.ip()
}
fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}

#[cfg(test)]
#[path = "../../tests/unit/connector_forwarding.rs"]
mod tests;
