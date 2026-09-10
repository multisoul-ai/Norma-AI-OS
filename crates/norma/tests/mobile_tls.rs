use norma::mobile::ServerIdentity;
use rustls::{
    pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, ServerConnection,
    StreamOwned,
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};

/// 电脑身份由持久证书决定，TLS 只允许持有该证书私钥的 Mac 接收手机内容。
///
/// 数据构造：两台独立身份 A/B；每条连接读写超时 2s（本机握手通常 <10ms，提供 200 倍余量）。
/// 执行过程：连接 A 并信任 A → 加密收发 ping/pong；再连接 A 但只信任 B → TLS 拒绝。
/// 预期结果：A/B 身份不同；正确证书完成双向通信，错误证书在应用内容前失败；身份调试输出不含私钥。
#[test]
fn pinned_machine_tls_rejects_another_computer() {
    let a = ServerIdentity::generate().expect("生成 A");
    let b = ServerIdentity::generate().expect("生成 B");
    assert_ne!(a.machine_id(), b.machine_id(), "同名电脑也必须有不同身份");
    assert_eq!(
        a.certificate_sha256().len(),
        64,
        "SHA-256 为 32 字节、64 hex 字符"
    );
    assert!(
        !format!("{a:?}").contains("private_key"),
        "调试对象不得输出私钥字段"
    );
    exchange(&a, &a, true);
    exchange(&a, &b, false);
}

fn exchange(server: &ServerIdentity, trusted: &ServerIdentity, accepted: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("临时回环端口");
    let address = listener.local_addr().expect("地址");
    let config = server.server_config().expect("TLS 配置");
    let peer = std::thread::spawn(move || {
        let (socket, _) = listener.accept().expect("客户端连接");
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("有界读");
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("有界写");
        let mut tls = StreamOwned::new(ServerConnection::new(config).expect("TLS server"), socket);
        let mut input = [0u8; 4];
        if tls.read_exact(&mut input).is_err() {
            return false;
        }
        assert_eq!(&input, b"ping", "只有已认证 TLS 通道可接收精确内容");
        tls.write_all(b"pong").expect("加密响应");
        tls.flush().expect("提交响应");
        true
    });
    let mut roots = RootCertStore::empty();
    roots
        .add(trusted.certificate_der().to_vec().into())
        .expect("测试仅信任指定电脑证书");
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("TLS 1.3")
            .with_root_certificates(roots)
            .with_no_client_auth();
    let socket = TcpStream::connect(address).expect("回环连接");
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("有界读");
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("有界写");
    let client = ClientConnection::new(
        Arc::new(config),
        ServerName::try_from("norma.local").expect("证书 DNS 名"),
    )
    .expect("TLS client");
    let mut tls = StreamOwned::new(client, socket);
    let result = tls.write_all(b"ping").and_then(|_| {
        let mut reply = [0u8; 4];
        tls.read_exact(&mut reply)?;
        assert_eq!(&reply, b"pong", "已认证电脑返回精确响应");
        Ok(())
    });
    assert_eq!(result.is_ok(), accepted, "不可信证书必须导致 TLS 失败");
    drop(tls);
    assert_eq!(
        peer.join().expect("收口服务端"),
        accepted,
        "错电脑不能收到应用内容"
    );
}

/// Keychain 中的证书和私钥跨重新打开保持不变，损坏记录不能被静默换成另一台电脑。
///
/// 数据构造：当前测试进程+纳秒组成独立 Keychain account，不读写生产 account；测试结束删除该唯一项目。
/// 执行过程：首次加载 → 第二次加载 → 核验证书不变 → 仅将测试项目改为坏 JSON → 再加载。
/// 预期结果：前两次返回同一机器身份；坏项目明确失败，不能生成新身份覆盖原记录。
#[cfg(target_os = "macos")]
#[test]
#[ignore = "writes and removes one isolated test Keychain item"]
fn native_keychain_preserves_identity_and_fails_closed() {
    const SERVICE: &str = "com.norma-ai.os.mobile-server";
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时钟")
        .as_nanos();
    let account = format!("mobile-test-{}-{suffix}", std::process::id());
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = security_framework::passwords::delete_generic_password(SERVICE, &self.0);
        }
    }
    let _cleanup = Cleanup(account.clone());
    let a = ServerIdentity::load_or_create(&account).expect("首次 Keychain 保存");
    let b = ServerIdentity::load_or_create(&account).expect("重新打开");
    assert_eq!(a.machine_id(), b.machine_id(), "重新打开不能变成另一台电脑");
    assert_eq!(a.certificate_der(), b.certificate_der(), "证书指纹必须稳定");
    security_framework::passwords::set_generic_password(SERVICE, &account, b"invalid-json")
        .expect("仅破坏隔离项目");
    assert!(
        ServerIdentity::load_or_create(&account).is_err(),
        "损坏 Keychain 必须停止，不可静默覆盖"
    );
    assert_eq!(
        security_framework::passwords::get_generic_password(SERVICE, &account).expect("读隔离项目"),
        b"invalid-json",
        "失败不得改写项目"
    );
}

/// 移动入口只接受 TLS 内的指定 WebSocket 路径；浏览器 Origin、query 和慢握手均不能进入应用路由。
///
/// 数据构造：本机临时监听；正确 /mobile、错误 /other、含 query、带 Origin 的四次握手。
/// 执行过程：分别完成可信 TLS 后请求 Upgrade；最后让已过截止时间的连接尝试握手。
/// 预期结果：仅无 Origin 的 /mobile 成功；其他入口失败，过期连接立即释放而非无限占用 worker。
#[test]
fn mobile_upgrade_enforces_route_origin_and_deadline() {
    use norma::mobile::accept_mobile_websocket;
    use tungstenite::client::IntoClientRequest;
    for (route, origin, allowed) in [
        ("/mobile", false, true),
        ("/other", false, false),
        ("/mobile?token=x", false, false),
        ("/mobile", true, false),
    ] {
        let identity = ServerIdentity::generate().expect("证书");
        let listener = TcpListener::bind("127.0.0.1:0").expect("临时端口");
        let addr = listener.local_addr().expect("地址");
        let config = identity.server_config().expect("TLS");
        let server = std::thread::spawn(move || {
            let (socket, _) = listener.accept().expect("连接");
            // macOS accept 会继承非阻塞监听器的标志；移动入口必须规范化后才做限时阻塞握手。
            socket
                .set_nonblocking(true)
                .expect("模拟真实非阻塞 listener 的 accept");
            // 2s 比回环握手正常的 <10ms 留 200 倍余量，仍限制未认证连接占用时间。
            accept_mobile_websocket(
                socket,
                config,
                std::time::Instant::now() + Duration::from_secs(2),
            )
            .is_ok()
        });
        let mut roots = RootCertStore::empty();
        roots
            .add(identity.certificate_der().to_vec().into())
            .expect("信任准确电脑");
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .expect("TLS13")
                .with_root_certificates(roots)
                .with_no_client_auth();
        let socket = TcpStream::connect(addr).expect("本地连接");
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("有界读");
        let tls = StreamOwned::new(
            ClientConnection::new(
                Arc::new(config),
                ServerName::try_from("norma.local").expect("证书名"),
            )
            .expect("TLS client"),
            socket,
        );
        let mut req = format!("wss://{addr}{route}")
            .into_client_request()
            .expect("WS 请求");
        if origin {
            req.headers_mut().insert(
                "origin",
                "https://untrusted.example".parse().expect("测试 Origin"),
            );
        }
        assert_eq!(
            tungstenite::client(req, tls).is_ok(),
            allowed,
            "客户端必须只进入指定原生路径"
        );
        assert_eq!(
            server.join().expect("回收服务端"),
            allowed,
            "拒绝的握手不能进入应用路由"
        );
    }
    let identity = ServerIdentity::generate().expect("证书");
    let listener = TcpListener::bind("127.0.0.1:0").expect("临时端口");
    let _client = TcpStream::connect(listener.local_addr().expect("地址")).expect("静默客户端");
    let (socket, _) = listener.accept().expect("连接");
    let start = std::time::Instant::now();
    assert!(
        accept_mobile_websocket(socket, identity.server_config().expect("TLS"), start).is_err(),
        "已到期握手不能阻塞读取"
    );
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "已到期连接应立即失败，1s 为调度余量"
    );
}
