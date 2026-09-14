use app::{MobileAccess, MobileServer};
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "support/mobile.rs"]
mod support;

/// 实时绑定先验证媒体身份、电脑和项目权限，再读取凭据或连接 OpenAI。
///
/// 数据构造：隔离库与本机 TLS；phone-a 只读项目 A，项目 B 未授权。
///   rtc_media_a 是媒体会话；call_function_a 是工具调用，不能代替媒体身份。
/// 执行过程：错媒体 ID → 错电脑 → 错口令 → 越权项目 → 只读绑定。
/// 预期结果：每项明确拒绝，均不能返回可执行绑定或创建任何任务。
#[test]
fn binding_checks_media_and_authorization_before_openai_access() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "A")
        .expect("项目 A")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "A",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("测试 TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
    )
    .expect("WSS");
    let action = |call: &str, token: &str, project: &str| {
        json!({
            "type":"bind_realtime", "device_id":"phone-a", "token":token,
            "binding":{"call_id":call,"workspace_id":project,"generation":1}
        })
    };
    let request = |target: &str, action| {
        let mut socket = support::connect(server.addresses()[0], cert.clone());
        support::exchange(&mut socket, target, "binding", action)
    };
    let invalid = request(
        &machine,
        action("call_function_a", &"a".repeat(64), &workspace),
    );
    assert_eq!(
        invalid["error"], "invalid_request",
        "工具 call_id 不得充当媒体会话"
    );
    let wrong_machine = request(
        "another-mac",
        action("rtc_media_a", &"a".repeat(64), &workspace),
    );
    assert_eq!(
        wrong_machine["error"], "unauthorized",
        "绑定不可转给其他电脑"
    );
    let wrong_token = request(&machine, action("rtc_media_a", &"b".repeat(64), &workspace));
    assert_eq!(
        wrong_token["error"], "unauthorized",
        "错口令不能打开 sideband"
    );
    let wrong_project = request(
        &machine,
        action("rtc_media_a", &"a".repeat(64), "project-b"),
    );
    assert_eq!(wrong_project["error"], "unauthorized", "未授权项目不能绑定");
    let read_only = request(&machine, action("rtc_media_a", &"a".repeat(64), &workspace));
    assert_eq!(
        read_only["error"], "unauthorized",
        "控制工具绑定不能提升只读授权"
    );
    assert!(
        read_only.get("result").is_none(),
        "拒绝时不能附带可执行绑定"
    );
    server.shutdown();
}

/// 同会话 sideband 的绑定代次唯一；重连只替换连接，挂断/撤销释放服务器连接。
///
/// 数据构造：两台手机均有项目 A control；回环假 OpenAI 只发送真实协议握手事件。
///   两次有效绑定 generation=1、2；同代次重试与另一手机都不得建立第三条 sideband。
/// 执行过程：绑定1并核对 URI → 同代次/另一手机拒绝 → 绑定2取代1 → 撤销手机。
/// 预期结果：只有两次上游握手；收到 session.updated 前不能报告 attached；旧连接失效。
#[test]
fn binding_replacement_is_scoped_and_releases_the_old_sideband() {
    use std::time::Duration;
    use tungstenite::Message;
    let directory = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "A")
        .expect("项目")
        .workspace_id;
    let grants = [MobileProjectAccess {
        workspace_id: workspace.clone(),
        access: MobilePermission::Control,
    }];
    store
        .mobile_access_store()
        .register("phone-a", "A", &"a".repeat(64), &grants)
        .expect("A");
    store
        .mobile_access_store()
        .register("phone-b", "B", &"b".repeat(64), &grants)
        .expect("B");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
    )
    .expect("服务");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("测试 sideband");
    server
        .use_realtime_server_for_e2e(listener.local_addr().expect("地址"))
        .expect("仅测试回环入口");
    let (observed, handshakes) = std::sync::mpsc::channel();
    let (acknowledge, acknowledgements) = std::sync::mpsc::channel();
    let upstream = std::thread::spawn(move || {
        // 5s = 3s wire budget + 2s heartbeat; a broken cancellation must fail instead of hanging.
        for _ in 0..2 {
            let stream = listener.accept().expect("两次有效绑定").0;
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("有界等待关闭");
            let mut socket = tungstenite::accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    observed
                        .send(request.uri().to_string())
                        .expect("记录媒体身份");
                    Ok(response)
                },
            )
            .expect("sideband 升级");
            socket
                .send(Message::text(
                    json!({"type":"session.created","session":{"type":"realtime"}}).to_string(),
                ))
                .expect("已创建事件");
            let update: serde_json::Value =
                serde_json::from_str(socket.read().expect("更新请求").to_text().expect("JSON"))
                    .expect("解析");
            assert_eq!(
                update["session"]["type"], "realtime",
                "必须使用 Realtime 会话更新，不能误用 GPT-Live 协议"
            );
            acknowledgements
                .recv_timeout(Duration::from_secs(5))
                .expect("先观察未确认状态，再模拟上游确认");
            socket
                .send(Message::text(
                    json!({"type":"session.updated","session":{"type":"realtime"}}).to_string(),
                ))
                .expect("上游确认");
            assert!(
                socket.read().is_err(),
                "换代和撤销必须释放旧上游连接，不保持无人持有的 sideband"
            );
        }
    });
    let bind = |device: &str, token: &str, generation| {
        json!({"type":"bind_realtime",
        "device_id":device,"token":token,"binding":{"call_id":"rtc_media_a","workspace_id":workspace,"generation":generation}})
    };
    let mut first = support::connect(server.addresses()[0], cert.clone());
    let initial = support::exchange(
        &mut first,
        &machine,
        "first",
        bind("phone-a", &"a".repeat(64), 1),
    );
    assert_eq!(
        initial["result"]["state"], "connecting",
        "不能在上游确认前声称已连接"
    );
    acknowledge.send(()).expect("允许首上游确认");
    let attached = wait_attached(&mut first);
    assert_eq!(
        attached["result"]["binding"]["generation"], 1,
        "首连接的代次不能丢失"
    );
    assert_eq!(
        attached["result"]["binding"]["workspace_id"], workspace,
        "项目身份应来自已验证绑定"
    );
    assert_eq!(
        handshakes
            .recv_timeout(Duration::from_secs(3))
            .expect("首握手"),
        "/v1/realtime?call_id=rtc_media_a",
        "上游只能收到媒体 call_id，不能发送 function call_id"
    );
    let mut stale = support::connect(server.addresses()[0], cert.clone());
    assert_eq!(
        support::exchange(
            &mut stale,
            &machine,
            "stale",
            bind("phone-a", &"a".repeat(64), 1)
        )["error"],
        "stale_binding",
        "同代次不能抢占已建立的执行方"
    );
    let mut other = support::connect(server.addresses()[0], cert.clone());
    assert_eq!(
        support::exchange(
            &mut other,
            &machine,
            "other",
            bind("phone-b", &"b".repeat(64), 2)
        )["error"],
        "session_in_use",
        "另一已配对手机也不能夺走同一媒体会话"
    );
    assert!(handshakes.try_recv().is_err(), "拒绝的请求不应触及 OpenAI");
    let mut second = support::connect(server.addresses()[0], cert);
    let _ = support::exchange(
        &mut second,
        &machine,
        "second",
        bind("phone-a", &"a".repeat(64), 2),
    );
    acknowledge.send(()).expect("允许第二上游确认");
    let attached = wait_attached(&mut second);
    assert_eq!(
        attached["result"]["binding"]["generation"], 2,
        "新连接必须使用新代次"
    );
    // A heartbeat sent before replacement may already be in the TCP receive buffer. Drain
    // up to four frames (same 8s bound as setup) to observe the terminal stale-binding reply.
    let mut old = serde_json::Value::Null;
    for _ in 0..4 {
        old = serde_json::from_str(
            first
                .read()
                .expect("旧连接失效通知")
                .to_text()
                .expect("JSON"),
        )
        .expect("旧响应");
        if old.get("error").is_some() {
            break;
        }
    }
    assert_eq!(
        old["error"], "stale_binding",
        "旧连接不能继续声明自己拥有执行权"
    );
    assert_eq!(
        handshakes
            .recv_timeout(Duration::from_secs(3))
            .expect("第二握手"),
        "/v1/realtime?call_id=rtc_media_a",
        "重连必须加入原媒体会话"
    );
    server.revoke("phone-a").expect("撤销");
    assert!(second.read().is_err(), "撤销应关闭绑定手机连接");
    upstream.join().expect("上游释放检查");
    assert!(handshakes.try_recv().is_err(), "不可出现第三个上游执行方");
    server.shutdown();
}

fn wait_attached(
    socket: &mut tungstenite::WebSocket<
        rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>,
    >,
) -> serde_json::Value {
    // At most four 2s heartbeat frames: setup acknowledgement has a separate 5s deadline.
    for _ in 0..4 {
        let value: serde_json::Value =
            serde_json::from_str(socket.read().expect("绑定状态").to_text().expect("JSON"))
                .expect("绑定响应");
        if value["result"]["state"] == "attached" {
            return value;
        }
        assert!(
            value.get("error").is_none(),
            "有效绑定不能返回拒绝：{value}"
        );
    }
    panic!("有效上游确认未能使绑定进入 attached");
}
