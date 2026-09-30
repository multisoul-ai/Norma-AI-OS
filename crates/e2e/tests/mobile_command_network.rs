use app::{MobileAccess, MobileServer};
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/mobile.rs"]
mod support;

/// 手机指令已落盘但前台尚未执行时，撤销和服务关闭都必须使执行权失效。
///
/// 数据构造：两台获 control 的测试手机，共享 project-a；有界通道容量 8 等于正式服务上限。
/// 执行过程：A 提交并截留 → 重试无第二次投递 → 撤销 A → 尝试开始 → B 提交两条 → 一条开始 → shutdown。
/// 预期结果：A 无法开始；B 的已开始指令只获一次执行权，关闭后另一条排队指令不可执行；重新查询已撤销手机失败。
#[test]
fn queued_mobile_commands_cannot_outlive_revoke_or_shutdown() {
    let dir = tempfile::tempdir().expect("临时目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Norma")
        .expect("项目");
    let grants = [MobileProjectAccess {
        workspace_id: project.workspace_id.clone(),
        access: MobilePermission::Control,
    }];
    store
        .mobile_access_store()
        .register("phone-a", "A", &"a".repeat(64), &grants)
        .expect("A 控制权限");
    store
        .mobile_access_store()
        .register("phone-b", "B", &"b".repeat(64), &grants)
        .expect("B 控制权限");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (source, requests) = async_channel::bounded(8);
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        None,
        Some(source),
    )
    .expect("WSS");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let action = |device: &str, token: &str, operation: &str| {
        json!({"type":"command","device_id":device,"token":token,"request":{
            "workspace_id":project.workspace_id,"route_id":"current-route","operation_id":operation,"grant_revision":1,
            "command":{"type":"create_card","profile":"Claude","title":null}
        }})
    };
    let a = action("phone-a", &"a".repeat(64), "a-one");
    let accepted = support::exchange(&mut socket, &machine, "submit-a", a.clone());
    assert_eq!(
        accepted["result"]["receipt"]["state"], "accepted",
        "网络接收不等同于已经执行"
    );
    let call_a = requests.try_recv().expect("截留尚未执行的请求");
    let replay = support::exchange(&mut socket, &machine, "retry-a", a);
    assert_eq!(
        replay["result"]["receipt"]["state"], "accepted",
        "重试只应回查原请求"
    );
    assert!(requests.try_recv().is_err(), "重试不得向前台第二次投递");
    server.revoke("phone-a").expect("撤销 A");
    assert!(!call_a.lease.begin(), "撤销必须战胜已入队但未开始的指令");
    call_a.reply.send(Err("unauthorized")).expect("结束 A 请求");
    drop(call_a);
    let query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project.workspace_id,"operation_id":"a-one"});
    assert_eq!(
        support::exchange(&mut socket, &machine, "read-a", query)["error"],
        "unauthorized",
        "撤销的设备不能读取旧回执"
    );
    let b = action("phone-b", &"b".repeat(64), "b-one");
    assert_eq!(
        support::exchange(&mut socket, &machine, "submit-b", b)["result"]["receipt"]["state"],
        "accepted",
        "另一手机权限不受 A 撤销影响"
    );
    let call_b = requests.try_recv().expect("B 请求");
    assert!(call_b.lease.begin(), "有效 B 请求可准确开始一次");
    let c = action("phone-b", &"b".repeat(64), "b-two");
    let _ = support::exchange(&mut socket, &machine, "submit-c", c);
    let call_c = requests.try_recv().expect("另一排队请求");
    server.shutdown();
    assert!(
        !call_b.lease.begin(),
        "同一指令不能重复开始，即使关闭前已执行"
    );
    assert!(!call_c.lease.begin(), "服务关闭后迟到的前台请求不得执行");
    call_b
        .reply
        .send(Ok(json!({"node_id":1})))
        .expect("测试中已开始的操作结果");
    call_c
        .reply
        .send(Err("unauthorized"))
        .expect("被关闭取消的操作");
    let ledger = storage::MobileCommandStore::new(store.path().to_path_buf());
    // 3s only permits the background receipt transactions to finish before deleting their directory.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let a = ledger
            .get("phone-a", "a-one", &project.workspace_id, "inspection")
            .expect("A 账本")
            .expect("A");
        let b = ledger
            .get("phone-b", "b-one", &project.workspace_id, "inspection")
            .expect("B 账本")
            .expect("B");
        let c = ledger
            .get("phone-b", "b-two", &project.workspace_id, "inspection")
            .expect("C 账本")
            .expect("C");
        if a.state == "rejected" && b.state == "succeeded" && c.state == "rejected" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "关闭后已接收的结果仍须写入对应历史回执"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// 副作用完成后回执写入失败，查询必须明确结果未知，而不能永远显示仍在执行。
///
/// 数据构造：隔离 SQLite 的 BEFORE UPDATE trigger 仅拦截 mobile_commands 终态写入；一条已接受的创建指令。
/// 执行过程：提交并截留 → 安装失败触发器 → 模拟前台返回 node_id=42 → 反复查询 → 重试同 ID。
/// 预期结果：查询 outcome_unknown、无伪造成功；重复指令不再投递，原始 accepted 记录仍保留在数据库。
#[test]
fn receipt_write_failure_becomes_unknown_without_replaying() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Norma")
        .expect("项目");
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "A",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("控制权限");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (tx, rx) = async_channel::bounded(8);
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        None,
        Some(tx),
    )
    .expect("服务");
    let mut socket = support::connect(server.addresses()[0], cert);
    let action = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{"operation_id":"write-fail","workspace_id":project.workspace_id,"route_id":"route-a","grant_revision":1,"command":{"type":"create_card","profile":"Codex","title":null}}});
    let _ = support::exchange(&mut socket, &machine, "submit", action.clone());
    let call = rx.try_recv().expect("前台请求");
    assert!(call.lease.begin(), "测试副作用开始");
    let database = rusqlite::Connection::open(store.path()).expect("注入写失败");
    database.execute_batch("CREATE TRIGGER fail_receipt BEFORE UPDATE ON mobile_commands BEGIN SELECT RAISE(FAIL,'receipt unavailable'); END;").expect("仅拦截测试库终态");
    call.reply
        .send(Ok(json!({"node_id":42})))
        .expect("模拟已完成的前台副作用");
    // 3s bounds local receipt IO/scheduling; it must not wait for a 30s execution timeout.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project.workspace_id,"operation_id":"write-fail"});
    let receipt = loop {
        let response = support::exchange(&mut socket, &machine, "query", query.clone());
        if response["result"]["receipt"]["state"] != "accepted" {
            break response;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "终态写失败不能永远声称 accepted"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!(
        receipt["result"]["receipt"]["state"], "outcome_unknown",
        "不能把执行结果未知包装成成功"
    );
    assert!(
        receipt["result"]["receipt"]["result"].is_null(),
        "无持久化证据不得返回 node_id=42"
    );
    let repeated = support::exchange(&mut socket, &machine, "retry", action);
    assert_eq!(
        repeated["result"]["receipt"]["state"], "outcome_unknown",
        "重复提交也不能把未知结果退回 accepted"
    );
    assert!(rx.try_recv().is_err(), "失败回执不能重新派发同一指令");
    assert_eq!(
        database
            .query_row(
                "SELECT state FROM mobile_commands WHERE operation_id='write-fail'",
                [],
                |row| row.get::<_, String>(0)
            )
            .expect("原记录"),
        "accepted",
        "保留真实持久化状态供重启审计"
    );
}

/// 两台电脑上的同名项目、相同局部ID与操作ID不得串用授权、决定或回执。
///
/// 数据构造：两个隔离SQLite/WSS服务、独立证书；模拟恢复后的同一workspace-id，
///   项目名均为Norma，phone/node/run/session/turn/request/operation均相同，但设备token不同。
///   node=7只是两台电脑恰好相同的局部ID；8个通道槽与生产入口上限一致。
/// 执行过程：错误电脑/凭据拒绝 → A拒绝与B批准分别入队 → 撤销A → B执行/保存 → 关闭A → B回查。
/// 预期结果：撤销A取消其执行权，B保留准确allow回执；重复决定只返回B原回执，不再投递。
#[test]
fn two_computers_keep_colliding_project_and_approval_ids_isolated() {
    let dir = tempfile::tempdir().expect("两台电脑的隔离目录");
    let host = |name: &str, token: &str| {
        let store = AppStore::new(dir.path().join(format!("{name}.sqlite")));
        store
            .workspace_identity_store()
            .resolve_or_create(dir.path(), "Norma")
            .expect("同名项目");
        // Only these disposable fixture databases simulate cloned project metadata.
        rusqlite::Connection::open(store.path())
            .expect("测试库")
            .execute(
                "UPDATE workspace_identities SET workspace_id='same-workspace'",
                [],
            )
            .expect("相同项目ID");
        store
            .mobile_access_store()
            .register(
                "same-phone",
                "iPhone",
                token,
                &[MobileProjectAccess {
                    workspace_id: "same-workspace".into(),
                    access: MobilePermission::Control,
                }],
            )
            .expect("各电脑独立授权");
        let identity = norma::mobile::ServerIdentity::generate().expect("独立电脑身份");
        let cert = identity.certificate_der().to_vec();
        let machine = identity.machine_id();
        let (tx, rx) = async_channel::bounded(8);
        let server = MobileServer::start_with_sources(
            MobileAccess::with_identity_for_e2e(store.clone(), identity),
            vec!["127.0.0.1:0".parse().expect("独立回环端口")],
            None,
            Some(tx),
        )
        .expect("真实WSS服务");
        (store, server, cert, machine, rx)
    };
    // Device tokens are 32-byte secrets encoded as 64 hex characters, scoped to each Mac.
    let token_a = "a".repeat(64);
    let token_b = "b".repeat(64);
    let (store_a, server_a, cert_a, machine_a, calls_a) = host("a", &token_a);
    let (store_b, server_b, cert_b, machine_b, calls_b) = host("b", &token_b);
    assert_ne!(machine_a, machine_b, "同名项目不能合并两台电脑身份");
    let mut a = support::connect(server_a.addresses()[0], cert_a.clone());
    let mut b = support::connect(server_b.addresses()[0], cert_b);
    let decide = |token: &str, decision: &str| {
        json!({
            "type":"command", "device_id":"same-phone", "token":token,
            "request":{"operation_id":"same-operation", "workspace_id":"same-workspace", "route_id":"same-route", "grant_revision":1,
                "command":{"type":"decide_approval", "expected":{"node_id":7,"profile":"Claude","run_id":"same-run",
                    "session_id":"same-session","turn_id":"same-turn","request_id":"same-request"}, "decision":decision}}
        })
    };
    let wrong_machine = support::exchange(
        &mut b,
        &machine_a,
        "wrong-machine",
        decide(&token_b, "allow"),
    );
    assert_eq!(
        wrong_machine["error"], "unauthorized",
        "连接B不能使用A的电脑身份，即使B凭据正确"
    );
    assert!(calls_b.try_recv().is_err(), "错误电脑身份不能投递给B");
    let wrong_token =
        support::exchange(&mut b, &machine_b, "wrong-token", decide(&token_a, "deny"));
    assert_eq!(wrong_token["error"], "unauthorized", "A的凭据不能用于B");
    assert!(calls_b.try_recv().is_err(), "同名手机不能跨电脑共享token");
    let accepted_a = support::exchange(&mut a, &machine_a, "choose-a", decide(&token_a, "deny"));
    let accepted_b = support::exchange(&mut b, &machine_b, "choose-b", decide(&token_b, "allow"));
    assert_eq!(
        accepted_a["result"]["receipt"]["state"], "accepted",
        "A只接收自己的拒绝决定"
    );
    assert_eq!(
        accepted_b["result"]["receipt"]["state"], "accepted",
        "B相同operation-id不应与A冲突"
    );
    let call_a = calls_a.try_recv().expect("A的唯一决定");
    let call_b = calls_b.try_recv().expect("B的唯一决定");
    assert_eq!(
        serde_json::to_value(&call_a.request.command).expect("A命令")["decision"],
        "deny",
        "A不能收到B的批准"
    );
    assert_eq!(
        serde_json::to_value(&call_b.request.command).expect("B命令")["decision"],
        "allow",
        "B不能收到A的拒绝"
    );
    server_a.revoke("same-phone").expect("只撤销A");
    assert!(!call_a.lease.begin(), "撤销A后排队决定不能执行");
    assert!(call_b.lease.begin(), "撤销A不应取消B的执行权");
    call_a
        .reply
        .send(Err("unauthorized"))
        .expect("结束A未执行的决定");
    call_b
        .reply
        .send(Ok(
            json!({"node_id":7,"decision":"allow","status":"submitted"}),
        ))
        .expect("B的真实路由回执");
    let ledger_a = storage::MobileCommandStore::new(store_a.path().to_path_buf());
    let ledger_b = storage::MobileCommandStore::new(store_b.path().to_path_buf());
    // 3s = background SQLite receipt writes plus local scheduler margin, never an unbounded wait.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let recorded_a = ledger_a
            .get(
                "same-phone",
                "same-operation",
                "same-workspace",
                "inspection",
            )
            .expect("A回执")
            .expect("已接收A");
        let recorded_b = ledger_b
            .get(
                "same-phone",
                "same-operation",
                "same-workspace",
                "inspection",
            )
            .expect("B回执")
            .expect("已接收B");
        if recorded_a.state == "rejected" && recorded_b.state == "succeeded" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "A应保存拒绝状态，B应独立保存成功状态"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let mut a = support::connect(server_a.addresses()[0], cert_a);
    let auth = |token: &str| json!({"type":"authenticate","device_id":"same-phone","token":token});
    assert_eq!(
        support::exchange(&mut a, &machine_a, "auth-a", auth(&token_a))["error"],
        "unauthorized",
        "A已撤销的凭据不能重新认证"
    );
    server_a.shutdown();
    assert_eq!(
        support::exchange(&mut b, &machine_b, "auth-b", auth(&token_b))["result"]["status"],
        "authorized",
        "关闭A后B仍可独立访问"
    );
    let repeated = support::exchange(&mut b, &machine_b, "repeat-b", decide(&token_b, "allow"));
    assert_eq!(
        repeated["result"]["receipt"]["state"], "succeeded",
        "B重试应返回已保存的终态"
    );
    assert_eq!(
        repeated["result"]["receipt"]["result"]["decision"], "allow",
        "回执必须是B原来的批准"
    );
    assert_ne!(
        repeated["result"]["receipt"]["result"]["decision"], "deny",
        "不能把A的拒绝当作B的结果"
    );
    assert!(calls_b.try_recv().is_err(), "B重复决定不能再次投递");
    assert!(calls_a.try_recv().is_err(), "B后续查询不能投递到A");
}
