use super::{source_ip, PeerProcessResolver, UnverifiedPeer};
use std::net::SocketAddr;
struct Proxy(SocketAddr, SocketAddr);
impl PeerProcessResolver for Proxy {
    fn is_tailscale_proxy(&self, peer: SocketAddr, local: SocketAddr) -> bool {
        (peer, local) == (self.0, self.1)
    }
}
fn source_ip_for_test(
    peer: std::net::IpAddr,
    host: &str,
    headers: &HeaderMap,
    tunnel: Option<&workspace::HostAuthority>,
) -> std::net::IpAddr {
    let local = "127.0.0.1:47615".parse().expect("本机socket");
    let peer = SocketAddr::new(peer, 12345);
    source_ip(peer, local, &Proxy(peer, local), host, headers, tunnel)
}
use axum::http::HeaderMap;

/// 场景：同一组有效代理头经过非回环peer、不同Host、未配置隧道和合法Funnel路径。
/// 执行：用表驱动调用来源提取，构造IPv4/IPv6客户端与重复/多跳/非法头。
/// 预期：只有完整可信元组采用单个转发IP；其他路径均保留TCP peer。
#[test]
fn forwarding_requires_the_entire_trusted_proxy_tuple() {
    let tunnel = "device.tailnet.ts.net".parse().expect("authority");
    let mut headers = HeaderMap::new();
    headers.insert("tailscale-funnel-request", "?1".parse().expect("标记"));
    headers.insert("x-forwarded-for", "198.51.100.9".parse().expect("来源"));
    for (peer, host, authority, expected) in [
        (
            "127.0.0.1",
            "device.tailnet.ts.net",
            Some(&tunnel),
            "198.51.100.9",
        ),
        (
            "::1",
            "device.tailnet.ts.net",
            Some(&tunnel),
            "198.51.100.9",
        ),
        (
            "192.0.2.1",
            "device.tailnet.ts.net",
            Some(&tunnel),
            "192.0.2.1",
        ),
        ("127.0.0.1", "localhost:47615", Some(&tunnel), "127.0.0.1"),
        ("127.0.0.1", "device.tailnet.ts.net", None, "127.0.0.1"),
    ] {
        assert_eq!(
            source_ip_for_test(peer.parse().expect("peer"), host, &headers, authority),
            expected.parse::<std::net::IpAddr>().expect("预期"),
            "peer={peer}, Host={host}, authority={authority:?}"
        );
    }
    let peer = "127.0.0.1".parse().expect("peer");
    for value in ["198.51.100.8, 198.51.100.9", "bad", "198.51.100.9:443", ""] {
        headers.insert("x-forwarded-for", value.parse().expect("头"));
        assert_eq!(
            source_ip_for_test(peer, tunnel.as_str(), &headers, Some(&tunnel)),
            peer,
            "单IP协议拒绝 {value:?}"
        );
    }
    headers.insert("x-forwarded-for", "2001:db8::1".parse().expect("IPv6头"));
    assert_eq!(
        source_ip_for_test(peer, tunnel.as_str(), &headers, Some(&tunnel)),
        "2001:db8::1".parse::<std::net::IpAddr>().expect("IPv6"),
        "支持Funnel IPv6客户端"
    );
    headers.append("x-forwarded-for", "198.51.100.9".parse().expect("重复头"));
    assert_eq!(
        source_ip_for_test(peer, tunnel.as_str(), &headers, Some(&tunnel)),
        peer,
        "重复XFF不可信"
    );
    headers.insert("x-forwarded-for", "198.51.100.9".parse().expect("单IP"));
    headers.append("tailscale-funnel-request", "?1".parse().expect("重复标记"));
    assert_eq!(
        source_ip_for_test(peer, tunnel.as_str(), &headers, Some(&tunnel)),
        peer,
        "重复Funnel标记不可信"
    );
}

/// 场景：完整合法头仍须通过真实连接四元组识别；生产回退策略不信任任何对端。
/// 构造：fake仅确认127.0.0.1:12345→127.0.0.1:47615；变更源端口/目标端口并调用来源提取。
/// 预期：仅匹配tuple可使用XFF；其他进程、复用端口和无法识别均保留peer，不能分桶。
#[test]
fn forwarding_requires_verified_socket_owner_and_fallback_ignores_xff() {
    let peer: SocketAddr = "127.0.0.1:12345".parse().expect("peer");
    let local: SocketAddr = "127.0.0.1:47615".parse().expect("local");
    let tunnel: workspace::HostAuthority = "device.tailnet.ts.net".parse().expect("隧道");
    let mut headers = HeaderMap::new();
    headers.insert("tailscale-funnel-request", "?1".parse().expect("标记"));
    headers.insert("x-forwarded-for", "198.51.100.9".parse().expect("XFF"));
    let proxy = Proxy(peer, local);
    assert_eq!(
        source_ip(
            peer,
            local,
            &proxy,
            tunnel.as_str(),
            &headers,
            Some(&tunnel)
        ),
        "198.51.100.9".parse::<std::net::IpAddr>().expect("公网IP"),
        "fake确认代理方可采信"
    );
    for (actual_peer, actual_local) in [
        (SocketAddr::new(peer.ip(), 12346), local),
        (peer, SocketAddr::new(local.ip(), 47616)),
    ] {
        assert_eq!(
            source_ip(
                actual_peer,
                actual_local,
                &proxy,
                tunnel.as_str(),
                &headers,
                Some(&tunnel)
            ),
            peer.ip(),
            "不同socket不属于已确认代理"
        );
    }
    assert_eq!(
        source_ip(
            peer,
            local,
            &UnverifiedPeer,
            tunnel.as_str(),
            &headers,
            Some(&tunnel)
        ),
        peer.ip(),
        "生产回退一律按peer地址计数"
    );
}
