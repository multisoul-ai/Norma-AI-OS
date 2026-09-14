use app::{MobileAccess, MobileServer};
use norma::mobile::ServerIdentity;
use std::process::Command;
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/mobile.rs"]
mod wire;

/// 安装包测试上游必须同时满足独立数据库、显式启用与回环地址，不能借用生产凭据。
///
/// 数据构造：四个独立子进程分别使用缺失开关、错误数据库、非回环上游、完整隔离；
///   临时TLS身份与一个control项目均只属于本测试，没有真实供应商凭据。
/// 执行过程：前三种配置启动即拒绝 → 合法配置绑定 → 捕获实际WebSocket请求。
/// 预期结果：不完整隔离不能启动或导出身份；合法请求只到本机且不携带Authorization，
///   仅提供当前App的公开证书供L3固定证书，绝不导出私钥。
#[test]
fn bundle_upstream_requires_isolation() {
    let Ok(case) = std::env::var("NORMA_REALTIME_ISOLATION_CASE") else {
        let executable = std::env::current_exe().expect("test binary");
        for case in ["missing-flag", "wrong-database", "external", "valid"] {
            let root = tempfile::tempdir().expect("isolated child database");
            let result = Command::new(&executable)
                .args([
                    "--exact",
                    "bundle_upstream_requires_isolation",
                    "--nocapture",
                ])
                .env("NORMA_REALTIME_ISOLATION_CASE", case)
                .env("NORMA_TEST_DATA_ROOT", root.path())
                .env_remove("NORMA_REALTIME_L3_UPSTREAM")
                .env_remove("NORMA_MOBILE_L3_LOOPBACK")
                .output()
                .expect("run environment case without global test races");
            assert!(
                result.status.success(),
                "{case}隔离检查失败：{}",
                String::from_utf8_lossy(&result.stdout)
            );
        }
        return;
    };
    let root =
        std::path::PathBuf::from(std::env::var_os("NORMA_TEST_DATA_ROOT").expect("child root"));
    let store = AppStore::new(root.join(if case == "wrong-database" {
        "other.sqlite"
    } else {
        "app.sqlite"
    }));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(&root, "isolation")
        .expect("workspace")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-test",
            "test",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("temporary grant");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test upstream");
    std::env::set_var(
        "NORMA_REALTIME_L3_UPSTREAM",
        if case == "external" {
            "192.0.2.1:1234".to_owned()
        } else {
            listener.local_addr().expect("address").to_string()
        },
    );
    if case != "missing-flag" {
        std::env::set_var("NORMA_MOBILE_L3_LOOPBACK", "1");
    }
    let identity = ServerIdentity::generate().expect("isolated TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("loopback")],
    );
    if case != "valid" {
        assert!(server.is_err(), "{case}必须在使用任何上游或长期凭据前拒绝");
        assert!(
            !root.join("realtime-public-identity.json").exists(),
            "隔离失败不能产生身份文件"
        );
        return;
    }
    let server = server.expect("complete isolation must start");
    let public: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("realtime-public-identity.json")).expect("L3公开身份文件"),
    )
    .expect("公开身份JSON");
    assert_eq!(public["machine_id"], machine, "公开身份必须属于当前服务");
    assert_eq!(
        public["certificate_der"],
        serde_json::json!(cert),
        "必须提供当前TLS证书"
    );
    assert!(public.get("private_key").is_none(), "L3不得接收服务器私钥");
    assert_eq!(
        public.as_object().map(serde_json::Map::len),
        Some(2),
        "仅允许公开身份和证书两个字段"
    );
    let mut phone = wire::connect(server.addresses()[0], cert);
    let bound = wire::exchange(
        &mut phone,
        &machine,
        "bind",
        serde_json::json!({
        "type":"bind_realtime", "device_id":"phone-test", "token":"a".repeat(64),
        "binding":{"call_id":"rtc_isolation", "workspace_id":workspace, "generation":1}}),
    );
    assert_eq!(
        bound["result"]["state"], "connecting",
        "本机隔离会话必须开始绑定"
    );
    listener.set_nonblocking(true).expect("bounded accept");
    // 5s = local connect budget(3s) + scheduler margin(2s); no cloud request is permitted.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "隔离App没有连接本机上游"
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("accept: {error}"),
        }
    };
    stream.set_nonblocking(false).expect("blocking handshake");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("bounded handshake");
    let _peer = tungstenite::accept_hdr(
        stream,
        |request: &tungstenite::handshake::server::Request, response| {
            assert_eq!(
                request.uri().to_string(),
                "/v1/realtime?call_id=rtc_isolation",
                "保留真实绑定身份"
            );
            assert!(
                request.headers().get("Authorization").is_none(),
                "本机替身绝不能收到长期凭据"
            );
            Ok(response)
        },
    )
    .expect("local WebSocket handshake");
    server.shutdown();
}
