use super::source_ip;
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
            source_ip(peer.parse().expect("peer"), host, &headers, authority),
            expected.parse::<std::net::IpAddr>().expect("预期"),
            "peer={peer}, Host={host}, authority={authority:?}"
        );
    }
    let peer = "127.0.0.1".parse().expect("peer");
    for value in ["198.51.100.8, 198.51.100.9", "bad", "198.51.100.9:443", ""] {
        headers.insert("x-forwarded-for", value.parse().expect("头"));
        assert_eq!(
            source_ip(peer, tunnel.as_str(), &headers, Some(&tunnel)),
            peer,
            "单IP协议拒绝 {value:?}"
        );
    }
    headers.insert("x-forwarded-for", "2001:db8::1".parse().expect("IPv6头"));
    assert_eq!(
        source_ip(peer, tunnel.as_str(), &headers, Some(&tunnel)),
        "2001:db8::1".parse::<std::net::IpAddr>().expect("IPv6"),
        "支持Funnel IPv6客户端"
    );
    headers.append("x-forwarded-for", "198.51.100.9".parse().expect("重复头"));
    assert_eq!(
        source_ip(peer, tunnel.as_str(), &headers, Some(&tunnel)),
        peer,
        "重复XFF不可信"
    );
    headers.insert("x-forwarded-for", "198.51.100.9".parse().expect("单IP"));
    headers.append("tailscale-funnel-request", "?1".parse().expect("重复标记"));
    assert_eq!(
        source_ip(peer, tunnel.as_str(), &headers, Some(&tunnel)),
        peer,
        "重复Funnel标记不可信"
    );
}
