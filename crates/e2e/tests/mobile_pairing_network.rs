use app::{MobileAccess, MobileServer};
use norma::mobile::ServerIdentity;
use std::time::Duration;
use storage::{AppStore, MobilePermission, MobileProjectAccess};
use tungstenite::Message;
#[path = "support/mobile.rs"]
mod support;
use support::{connect, exchange};

/// 手机经真实回环 WSS 扫码、等 Mac 授权、读回范围；撤销后旧连接失权，服务释放后连接关闭。
///
/// 数据构造：真实 TLS 身份+隔离 SQLite+随机回环端口；读超时 3s，允许本机调度但不无限挂起。
/// 执行过程：认证失败 → 扫码 → 远程伪造批准被拒 → Mac 批准 → 同一连接认证成功 → Mac 撤销 → 再认证失败 → Drop 服务。
/// 预期结果：未授权不可读；远程不能调用本地批准；只获真实项目只读范围；撤销不能沿用旧连接，关闭立即终止通道。
#[test]
fn websocket_pairing_requires_desktop_scope_and_closes_on_shutdown() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Norma")
        .expect("真实项目");
    let identity = ServerIdentity::generate().expect("身份");
    let certificate = identity.certificate_der().to_vec();
    let machine_id = identity.machine_id();
    let access = MobileAccess::with_identity_for_e2e(store.clone(), identity);
    let server = MobileServer::start(access, vec!["127.0.0.1:0".parse().expect("回环地址")])
        .expect("开启受控入口");
    let addresses = server.addresses();
    let address = addresses[0];
    let qr = server.begin_pairing("Mac").expect("Mac 打开扫码");
    let payload: serde_json::Value =
        serde_json::from_str(&qr.to_json().expect("QR")).expect("QR JSON");
    assert_eq!(
        payload["endpoints"][0],
        format!("wss://{address}/mobile"),
        "QR 必须包含实际监听地址和端口"
    );
    let mut socket = connect(address, certificate.clone());
    let authenticate =
        serde_json::json!({"type":"authenticate","device_id":"phone-a","token":"a".repeat(64)});
    let first = exchange(&mut socket, &machine_id, "first", authenticate.clone());
    assert_eq!(first["error"], "unauthorized", "未扫码手机不能认证");
    let snapshot = exchange(
        &mut socket,
        &machine_id,
        "unauthorized-snapshot",
        serde_json::json!({
            "type":"snapshot", "device_id":"phone-a", "token":"a".repeat(64), "workspace_id":null
        }),
    );
    assert_eq!(
        snapshot["error"], "unauthorized",
        "项目快照入口必须验证手机凭据"
    );
    assert!(
        snapshot.get("result").is_none(),
        "未授权响应不得包含任何项目数据"
    );
    let claim = exchange(
        &mut socket,
        &machine_id,
        "scan",
        serde_json::json!({"type":"pair","request":{
            "offer_id":payload["offer_id"],"secret":payload["secret"],"device_id":"phone-a","device_name":"iPhone","device_token":"a".repeat(64)
        }}),
    );
    assert_eq!(
        claim["result"]["status"], "pending",
        "扫码只进入 Mac 待核对状态"
    );
    let pending = server.pending().expect("读 pending").expect("Mac 有待确认");
    assert_eq!(
        claim["result"]["pending"]["verification_code"], pending.verification_code,
        "双端显示同一个核对码"
    );
    let remote_approval = exchange(
        &mut socket,
        &machine_id,
        "forged",
        serde_json::json!({"type":"approve","request_id":pending.request_id}),
    );
    assert_eq!(
        remote_approval["error"], "invalid_request",
        "远程协议没有批准配对操作"
    );
    server
        .approve(
            &pending.request_id,
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("Mac 明确批准");
    let granted = exchange(
        &mut socket,
        &machine_id,
        "after-approval",
        authenticate.clone(),
    );
    assert_eq!(
        granted["result"]["grant"]["projects"][0]["workspace_id"], project.workspace_id,
        "只返回真实授权项目"
    );
    assert_eq!(
        granted["result"]["grant"]["projects"][0]["access"], "read",
        "不得升级控制权限"
    );
    assert!(
        granted["result"]["grant"].get("token").is_none(),
        "响应不能输出设备凭据"
    );
    server.revoke("phone-a").expect("Mac 撤销");
    assert!(socket.read().is_err(), "撤销必须关闭已认证连接");
    let mut socket = connect(address, certificate);
    let denied = exchange(&mut socket, &machine_id, "revoked", authenticate);
    assert_eq!(
        denied["error"], "unauthorized",
        "重新连接也不能复用撤销前认证"
    );
    drop(server);
    assert!(socket.read().is_err(), "关闭 owner 必须中止既有连接");
    assert!(
        store
            .mobile_access_store()
            .authenticate("phone-a", &"a".repeat(64))
            .expect("重开")
            .is_none(),
        "撤销仍持久化"
    );
}

/// 未认证客户端不断发送合法 Ping 也不能永久占用连接槽位。
///
/// 数据构造：一台隔离服务和一条未认证 TLS 连接；10s 认证期限，额外 2s 留作调度/关闭传播余量。
/// 执行过程：完成 TLS/WS → 每 200ms 发 Ping 并收 Pong → 观察认证截止前后的连接。
/// 预期结果：至少一次 Ping/Pong 正常，之后连接主动关闭；不能通过流量反复延长未认证期限。
#[test]
fn unauthenticated_traffic_cannot_extend_connection_lifetime() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let identity = ServerIdentity::generate().expect("身份");
    let cert = identity.certificate_der().to_vec();
    let access =
        MobileAccess::with_identity_for_e2e(AppStore::new(dir.path().join("app.sqlite")), identity);
    let server =
        MobileServer::start(access, vec!["127.0.0.1:0".parse().expect("回环")]).expect("服务");
    let mut socket = connect(server.addresses()[0], cert);
    let started = std::time::Instant::now();
    let mut received = 0;
    let mut closed = false;
    // 12 = 10s 认证期限 + 2s 本机调度余量；每 200ms 有流量，远小于 30s idle timeout。
    while started.elapsed() < Duration::from_secs(12) {
        if socket.send(Message::Ping(vec![1].into())).is_err() || socket.read().is_err() {
            closed = true;
            break;
        }
        received += 1;
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        received > 0,
        "必须先确认通道正常，不能把握手失败当作认证超时通过"
    );
    assert!(closed, "未认证连接必须在固定期限关闭，Ping 不能续期");
}

/// 手机快照已排到前台但尚未完成时撤销授权，不能返回撤销前读取的数据。
///
/// 数据构造：一台手机仅有 A 只读权限；前台队列容量 1，仅允许这条请求进入。
/// 执行过程：真实 WSS 发快照 → 截留前台请求 → 本地撤销 → 返回旧项目信息 → 网络再次校验授权。
/// 预期结果：请求确实进入过前台队列；撤销不被等待前台的锁阻塞；最终 unauthorized 且不含任何项目。
#[test]
fn revocation_during_foreground_snapshot_drops_old_data() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let id = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Private A")
        .expect("A")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("初始只读授权");
    let identity = ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (source, requests) = async_channel::bounded(1);
    let server = MobileServer::start_with_snapshots(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(source),
    )
    .expect("WSS");
    let address = server.addresses()[0];
    let client = std::thread::spawn(move || {
        let mut socket = connect(address, cert);
        exchange(
            &mut socket,
            &machine,
            "revoke-during-read",
            serde_json::json!({
                "type":"snapshot", "device_id":"phone-a", "token":"a".repeat(64), "workspace_id":null
            }),
        )
    });
    // 1s to observe delivery is below the production 2s foreground deadline.
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let request = loop {
        if let Ok(request) = requests.try_recv() {
            break request;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "必须在有限时间内观察真实请求进入前台队列"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(
        request.projects[0].workspace_id, id,
        "请求确实通过了初次授权检查"
    );
    server
        .revoke("phone-a")
        .expect("等待前台时必须允许立即撤销");
    request
        .reply
        .try_send(request.projects)
        .expect("模拟前台返回旧快照");
    let reply = client.join().expect("客户端应正常取得错误回执");
    assert_eq!(reply["error"], "unauthorized", "撤销后必须再次拒绝读取结果");
    assert!(
        reply.get("result").is_none(),
        "不得把撤销前的数据装入错误回执"
    );
    assert!(
        !reply.to_string().contains("Private A"),
        "项目名称也不得泄漏给已撤销手机"
    );
}
