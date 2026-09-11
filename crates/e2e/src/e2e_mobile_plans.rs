use crate::{
    e2e_mobile_snapshot::fetch_action,
    harness::{echo_factory, make_app_with_settings_store},
};
use app::{MobileAccess, MobileServer};
use gpui::TestAppContext;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "../tests/support/mobile_plan_native.rs"]
mod native;
use native::run_native;

/// 手机要求Norma生成计划，经正式GPUI配置/模型适配/SQLite产生可回查的建议，不启动执行Agent。
///
/// 数据构造：单只读手机、真实空Codex卡；回环模型返回一条指向实际卡片的建议，升级后revision=2。
/// 执行过程：只读拒绝 → 升权 → 生成请求 → 持久化回执 → 读取完整计划 → 重试/越权/撤销。
/// 预期结果：保留用户方向和模型建议、卡片仍为空闲；重复不产生新计划，未授权内容不可读。
#[test]
fn e2e_mobile_plan_generation_persists_proposals_without_dispatch() {
    run(Scenario::Publish);
}

/// 生成中关闭窗口会结束模型HTTP请求，旧建议不再发布。
///
/// 数据构造：一个空Codex卡，模型已读入请求但不回复；HTTP读预算5s，低于模型20s超时。
/// 执行过程：等待模型收到请求 → 关闭窗口 → 回查拒绝回执 → 验证HTTP EOF和空计划库。
/// 预期结果：route_expired、无成功结果、无计划；无需等模型超时才能收回连接。
#[test]
fn e2e_mobile_plan_generation_cancels_when_window_closes() {
    run(Scenario::Close);
}

/// 生成期间删除目标卡片后，迟到建议不能进入项目。
///
/// 数据构造：模型引用真实空Codex node，HTTP回复由测试释放，卡片先移除。
/// 执行过程：等请求到达 → 删除该node → 释放模型回复 → 回查计划和回执。
/// 预期结果：session_changed、无成功结果、无计划；不会创建替代卡或执行任务。
#[test]
fn e2e_mobile_plan_generation_rejects_removed_target() {
    run(Scenario::Remove);
}

/// 原生Swift经真实WSS和正式GPUI模型入口生成建议，重开本地记录后只回查原计划。
///
/// 数据构造：隔离配对手机、空Codex卡、本地模型HTTP响应；先只读再升权。
/// 执行过程：原生配对/越权拒绝 → 生成 → 丢弃初始回执 → 持久化回查 → 分页读取/重复提交。
/// 预期结果：准确方向/目标和唯一计划，无Agent进程；适用于macOS和iOS模拟器probe。
#[test]
#[ignore = "requires NORMA_SWIFT_PROBE"]
fn e2e_mobile_plan_generation_native() {
    run(Scenario::Native);
}

/// 方向优先事项经正式GPUI模型入口绑定来源，保留原计划并将指定项排到首位。
/// 数据构造：旧计划一项、选index0“补测试”；模型先返回“整理界面”，然后返回所选事项。
/// 执行过程：只读拒绝 → 升权 → 后台读取原待办 → 模型推理 → 排序和事务保存 → WSS回查。
/// 预期结果：新方向和完整指令正确，原计划保留，未启动Agent，也未新增执行任务。
#[test]
fn e2e_mobile_plan_priority_uses_original_todo() {
    run(Scenario::Priority);
}

/// 推理期间另一端先发布方向，正式GPUI路径应拒绝迟到建议。
/// 数据构造：同上原计划，模型读完输入后保存peer-plan再释放回复；只有两份历史记录。
/// 执行过程：请求已受理 → 对端发布 → 返回模型建议 → SQLite比较来源 → 拒绝并回读。
/// 预期结果：plan_changed，无成功计划ID；peer-plan仍是最新，Agent保持空闲。
#[test]
fn e2e_mobile_plan_priority_rejects_late_publish() {
    run(Scenario::PriorityRace);
}

#[derive(Clone, Copy)]
enum Scenario {
    Publish,
    Priority,
    PriorityRace,
    Native,
    Close,
    Remove,
}

fn run(scenario: Scenario) {
    let mut cx = TestAppContext::single();
    let directory = tempfile::tempdir().expect("隔离数据");
    cx.update(|cx| {
        app::install_norma_endpoint_for_e2e(directory.path().join("norma.sock"), cx).expect("路由")
    });
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("模型fixture");
    let endpoint = format!("http://{}/v1", listener.local_addr().expect("端口"));
    let settings = storage::SettingsStore::new(directory.path().join("app.sqlite"));
    settings
        .set_string("agent.provider.active", "openai")
        .expect("选择Provider");
    settings
        .set_string("agent.provider.openai.base_url", &endpoint)
        .expect("回环地址");
    settings
        .set_string("agent.provider.openai.model", "plan-fixture")
        .expect("模型");
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), settings);
    let workspace = tempfile::tempdir().expect("项目");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace.path(), window, cx)
        })
    });
    let node = app.update(cx, |app, _| {
        app.runtime
            .create_mobile_agent_card(
                canvas::AgentProfile::Codex,
                None,
                None,
                canvas::CanvasRect::new(0., 0., 640., 400.),
            )
            .expect("空卡")
    });
    let (path, snapshots, commands) = app.read_with(cx, |app, cx| {
        (
            app.app_database_path_for_e2e(),
            app.mobile_snapshot_source_for_e2e(cx),
            app.mobile_command_source_for_e2e(cx),
        )
    });
    let store = AppStore::new(path);
    let id = store
        .workspace_identity_store()
        .resolve_or_create(workspace.path(), "Plan")
        .expect("身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "Phone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        snapshots,
        commands,
    )
    .expect("WSS");
    if matches!(scenario, Scenario::Native) {
        let (ready, _received) = std::sync::mpsc::sync_channel(1);
        let (release, released) = std::sync::mpsc::sync_channel(1);
        release.send(()).expect("原生模型可直接回复");
        let model =
            std::thread::spawn(move || reply_once(listener, node, scenario, ready, released));
        run_native(cx, &server, directory.path(), &id);
        model.join().expect("模型回收");
        assert_eq!(
            app.read_with(cx, |app, _| app.runtime.live_len()),
            0,
            "原生规划不得启动执行Agent"
        );
        server.shutdown();
        return;
    }
    let snapshot = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"snapshot","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id}),
    );
    let route = snapshot["result"]["projects"][0]["route_id"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "项目当前route缺失：status={} error={} project={}",
                snapshot["result"]["status"], snapshot["error"], snapshot["result"]["projects"][0]
            )
        });
    let mut action = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{"operation_id":"plan-request","workspace_id":id,"route_id":route,"grant_revision":1,"command":{"type":"generate_plan","direction":"优先验证登录"}}});
    let priority = matches!(scenario, Scenario::Priority | Scenario::PriorityRace);
    let plan_store =
        storage::ProjectPlanStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let target = serde_json::from_value(snapshot["result"]["projects"][0]["cards"][0].clone())
        .expect("原卡快照");
    let old_draft = agent::ProjectPlanDraft {
        summary: "原建议".into(),
        todos: vec![agent::PlanTodo {
            target,
            title: "补测试".into(),
            instruction: "原登录验证".into(),
        }],
    };
    if priority {
        plan_store
            .publish("source-plan", &id, route, "原方向", &old_draft)
            .expect("原计划");
        action["request"]["command"]["priority"] = json!({"plan_id":"source-plan","todo_index":0});
    }
    let denied = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        denied["error"], "unauthorized",
        "只读不能调用模型生成/写入计划"
    );
    server
        .replace_permissions(
            "phone-a",
            1,
            &[MobileProjectAccess {
                workspace_id: id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("升权");
    action["request"]["grant_revision"] = json!(2);
    let (ready, received) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let model = std::thread::spawn(move || reply_once(listener, node, scenario, ready, released));
    let accepted = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        accepted["result"]["status"], "receipt",
        "生成请求需要可恢复的操作回执"
    );
    let query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id,"operation_id":"plan-request"});
    //30s matches command budget; local fixture must resolve without an unbounded polling loop.
    let deadline = Instant::now() + Duration::from_secs(30);
    while received.try_recv().is_err() {
        let waiting = fetch_action(cx, &server, &cert, &machine, query.clone());
        assert_eq!(
            waiting["result"]["receipt"]["state"], "accepted",
            "模型未回复前不得提前成功"
        );
        assert!(Instant::now() < deadline, "模型请求必须在预算内到达");
    }
    match scenario {
        Scenario::Publish | Scenario::Priority | Scenario::Native => {}
        Scenario::PriorityRace => {
            plan_store
                .publish("peer-plan", &id, route, "另一端方向", &old_draft)
                .expect("对端先发布");
        }
        Scenario::Close => cx.update(|window, _| window.remove_window()),
        Scenario::Remove => app.update(cx, |app, _| {
            app.runtime.remove_node(node);
        }),
    }
    release.send(()).expect("释放模型fixture");
    let receipt = loop {
        let next = fetch_action(cx, &server, &cert, &machine, query.clone());
        if next["result"]["receipt"]["state"] != "accepted" {
            break next;
        }
        assert!(Instant::now() < deadline, "计划回执必须有界结束");
    };
    model.join().expect("模型线程");
    let read = json!({"type":"project_plans","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id,"before":null});
    if !matches!(scenario, Scenario::Publish | Scenario::Priority) {
        assert_eq!(
            receipt["result"]["receipt"]["state"], "rejected",
            "过期建议必须拒绝"
        );
        assert_eq!(
            receipt["result"]["receipt"]["error"],
            if matches!(scenario, Scenario::Close) {
                "route_expired"
            } else if priority {
                "plan_changed"
            } else {
                "session_changed"
            },
            "拒绝必须说明真实边界变化"
        );
        assert!(
            receipt["result"]["receipt"]["result"].is_null(),
            "拒绝不得附带计划身份"
        );
        let plans = fetch_action(cx, &server, &cert, &machine, read);
        if priority {
            let items = plans["result"]["plans"]["items"]
                .as_array()
                .expect("历史计划");
            assert_eq!(items.len(), 2, "只能保留原计划和对端计划，迟到建议不入库");
            assert_eq!(items[0]["plan_id"], "peer-plan", "对端新方向不能被覆盖");
            assert_eq!(items[1]["plan_id"], "source-plan", "原计划及关联决定保留");
        } else {
            assert_eq!(
                plans["result"]["plans"]["items"],
                json!([]),
                "不能保存失去作用目标的建议"
            );
        }
        assert_eq!(
            app.read_with(cx, |app, _| app.runtime.live_len()),
            0,
            "不得启动替代执行Agent"
        );
        server.shutdown();
        return;
    }
    assert_eq!(
        receipt["result"]["receipt"]["state"], "succeeded",
        "计划必须在保存后成功"
    );
    let plan_id = receipt["result"]["receipt"]["result"]["plan_id"]
        .as_str()
        .expect("计划身份");
    let plans = fetch_action(cx, &server, &cert, &machine, read.clone());
    let items = plans["result"]["plans"]["items"]
        .as_array()
        .expect("真实计划");
    assert_eq!(
        items.len(),
        if priority { 2 } else { 1 },
        "只新增一份建议，保留原记录"
    );
    if priority {
        assert_eq!(items[1]["plan_id"], "source-plan", "不改写原计划");
        assert_eq!(
            items[0]["draft"]["todos"][0]["title"], "补测试",
            "选中的待办真实前移"
        );
        assert_eq!(
            items[0]["draft"]["todos"][1]["title"], "整理界面",
            "其他建议顺延保留"
        );
    }
    assert_eq!(items[0]["plan_id"], plan_id, "回执必须能定位保存的计划");
    assert_eq!(items[0]["direction"], "优先验证登录", "必须保留原始方向");
    assert_eq!(
        items[0]["draft"]["todos"][0]["target"]["node_id"], node,
        "建议指向真实Agent"
    );
    assert_eq!(
        items[0]["draft"]["todos"][0]["instruction"], "检查登录边界并补测试。",
        "建议原文不能被截断"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        0,
        "生成计划不得启动执行Agent"
    );
    let replay = fetch_action(cx, &server, &cert, &machine, action);
    assert_eq!(
        replay["result"]["receipt"], receipt["result"]["receipt"],
        "重复请求复用原回执，不再次调用模型"
    );
    let private = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"project_plans","device_id":"phone-a","token":"a".repeat(64),"workspace_id":"hidden","before":null}),
    );
    assert_eq!(private["error"], "unauthorized", "不能读取未授权项目计划");
    assert!(private.get("result").is_none(), "拒绝不得携带建议正文");
    server.revoke("phone-a").expect("撤销");
    let revoked = fetch_action(cx, &server, &cert, &machine, read);
    assert_eq!(revoked["error"], "unauthorized", "撤销立即禁止计划回读");
    server.shutdown();
}

fn reply_once(
    listener: std::net::TcpListener,
    node: u64,
    scenario: Scenario,
    ready: std::sync::mpsc::SyncSender<()>,
    released: std::sync::mpsc::Receiver<()>,
) {
    use std::io::{BufRead, Read, Write};
    listener.set_nonblocking(true).expect("有界accept");
    // Native: 90s Xcode startup + 30s pairing. Others: 20s model budget + 5s scheduling.
    let accept_budget = if matches!(scenario, Scenario::Native) {
        120
    } else {
        25
    };
    let deadline = Instant::now() + Duration::from_secs(accept_budget);
    let mut socket = loop {
        if let Ok((socket, _)) = listener.accept() {
            break socket;
        }
        assert!(Instant::now() < deadline, "模型调用缺失");
        std::thread::sleep(Duration::from_millis(10));
    };
    // Darwin may inherit O_NONBLOCK from the listener; make the 5s read budget effective.
    socket.set_nonblocking(false).expect("有界阻塞读取");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("完整请求预算");
    let mut reader = std::io::BufReader::new(&mut socket);
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("请求头");
        assert!(!line.is_empty(), "不能在请求头结束前EOF");
        if line == "\r\n" {
            break;
        }
        if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = n.trim().parse::<usize>().expect("长度");
        }
    }
    assert!(length > 0 && length < 128 * 1024, "请求正文有界");
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("正文");
    let request: Value = serde_json::from_slice(&body).expect("JSON");
    assert!(
        request["tools"].is_null() || request["tools"].as_array().is_some_and(Vec::is_empty),
        "规划没有执行工具"
    );
    if matches!(scenario, Scenario::Priority | Scenario::PriorityRace) {
        let messages = request["messages"].to_string();
        assert!(
            messages.contains("原登录验证"),
            "必须由正式入口读取来源待办并传给模型"
        );
    }
    ready.send(()).expect("模型已读取请求");
    released
        .recv_timeout(Duration::from_secs(5))
        .expect("测试应在5s内推进前台状态");
    if matches!(scenario, Scenario::Close) {
        let mut byte = [0];
        assert_eq!(
            reader
                .read(&mut byte)
                .expect("关闭应取消HTTP而非等20s模型超时"),
            0,
            "窗口关闭后客户端必须断开"
        );
        return;
    }
    let mut todos =
        vec![json!({"node_id":node,"title":"补测试","instruction":"检查登录边界并补测试。"})];
    if matches!(scenario, Scenario::Priority | Scenario::PriorityRace) {
        todos.insert(
            0,
            json!({"node_id":node,"title":"整理界面","instruction":"核对布局"}),
        );
    }
    let content = json!({"summary":"下一步验证登录","todos":todos}).to_string();
    let body=json!({"id":"fixture","object":"chat.completion","created":1,"model":"plan-fixture","choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":"stop"}]}).to_string();
    write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).expect("回复");
}
