use super::permitted;

/// 手机只公布 LAN 和 Tailscale，不能把不可达的普通 VPN 隧道误列为局域网电脑地址。
///
/// 数据构造：现场 en0=192.168.110.118（非点对点）；utun5=10.8.0.37 和 fdcc::25（点对点 VPN）；
///   Tailscale 分配范围为 100.64/10 与 fd7a:115c:a1e0::/48，即使点对点也必须保留。
/// 执行过程：用同一接口筛选函数检查现场 LAN/VPN、Tailscale IPv4/IPv6 与公网地址。
/// 预期结果：LAN/Tailscale 可以公布；普通 VPN、公网和 loopback 不得作为手机连接地址。
#[test]
fn advertised_interfaces_exclude_unrelated_vpn_tunnels() {
    assert!(
        permitted("192.168.110.118".parse().expect("LAN"), false),
        "物理局域网仍可连接"
    );
    assert!(
        !permitted("10.8.0.37".parse().expect("VPN"), true),
        "不能把普通 VPN 地址推荐给手机"
    );
    assert!(
        !permitted(
            "fdcc:ad94:bacf:61a4::cafe:25".parse().expect("VPN IPv6"),
            true
        ),
        "不能通过 IPv6 重新公布同一 VPN"
    );
    assert!(
        permitted("100.100.10.2".parse().expect("Tailscale"), true),
        "必须保留 Tailscale 点对点路由"
    );
    assert!(
        permitted(
            "fd7a:115c:a1e0::1234".parse().expect("Tailscale IPv6"),
            true
        ),
        "必须保留 Tailscale IPv6"
    );
    assert!(
        !permitted("8.8.8.8".parse().expect("公网"), false),
        "不公布公网监听地址"
    );
    assert!(
        !permitted("127.0.0.1".parse().expect("loopback"), false),
        "手机不能使用电脑的 loopback"
    );
}
