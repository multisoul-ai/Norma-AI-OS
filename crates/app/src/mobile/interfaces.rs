use anyhow::{ensure, Result};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

/// Native interface discovery. No shell, public addresses, wildcard binds or unscoped IPv6 link-local.
pub(super) fn addresses() -> Result<Vec<SocketAddr>> {
    let mut first = std::ptr::null_mut();
    // SAFETY: getifaddrs initializes a linked list whose lifetime is guarded below.
    ensure!(
        unsafe { libc::getifaddrs(&mut first) } == 0,
        "无法读取局域网地址"
    );
    struct Interfaces(*mut libc::ifaddrs);
    impl Drop for Interfaces {
        fn drop(&mut self) {
            unsafe { libc::freeifaddrs(self.0) };
        }
    }
    let _guard = Interfaces(first);
    let mut addresses = BTreeSet::new();
    let mut next = first;
    while !next.is_null() {
        // SAFETY: nodes and sockaddr family-sized data are owned by the guarded getifaddrs list.
        let entry = unsafe { &*next };
        next = entry.ifa_next;
        if entry.ifa_addr.is_null() || entry.ifa_flags & libc::IFF_UP as u32 == 0 {
            continue;
        }
        let ip = unsafe {
            match i32::from((*entry.ifa_addr).sa_family) {
                libc::AF_INET => Some(IpAddr::V4(Ipv4Addr::from(
                    (*(entry.ifa_addr.cast::<libc::sockaddr_in>()))
                        .sin_addr
                        .s_addr
                        .to_ne_bytes(),
                ))),
                libc::AF_INET6 => Some(IpAddr::V6(Ipv6Addr::from(
                    (*(entry.ifa_addr.cast::<libc::sockaddr_in6>()))
                        .sin6_addr
                        .s6_addr,
                ))),
                _ => None,
            }
        };
        let point_to_point = entry.ifa_flags & libc::IFF_POINTOPOINT as u32 != 0;
        if let Some(ip) = ip.filter(|ip| permitted(*ip, point_to_point)) {
            addresses.insert(ip);
        }
    }
    let mut addresses: Vec<_> = addresses.into_iter().collect();
    // Prefer Tailscale and ordinary IPv4 before ULA; publish at most four explicit routes.
    addresses.sort_by_key(|ip| {
        (
            if tailscale(*ip) {
                0
            } else if ip.is_ipv4() {
                1
            } else {
                2
            },
            *ip,
        )
    });
    ensure!(
        !addresses.is_empty(),
        "没有可用的局域网或 Tailscale 地址，请先连接网络"
    );
    Ok(addresses
        .into_iter()
        .take(4)
        // Stable port lets saved phone endpoints survive restarting Norma on this Mac.
        .map(|ip| SocketAddr::new(ip, 9443))
        .collect())
}

fn tailscale(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            a == 100 && (64..=127).contains(&b)
        }
        IpAddr::V6(ip) => ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
    }
}
fn permitted(ip: IpAddr, point_to_point: bool) -> bool {
    // Ordinary VPN tunnel addresses need not be reachable from the phone, even on the same LAN.
    if point_to_point && !tailscale(ip) {
        return false;
    }
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || tailscale(IpAddr::V4(ip)),
        IpAddr::V6(ip) => ip.is_unique_local(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mobile_interface_tests.rs"]
mod tests;
