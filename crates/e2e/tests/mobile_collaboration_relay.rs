use app::{MobileAccess, MobileServer};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/thread_review.rs"]
mod thread_review;
#[path = "support/thread_terminal.rs"]
mod thread_terminal;
#[path = "support/thread_terminal_control.rs"]
mod thread_terminal_control;
#[path = "support/thread_followup.rs"]
mod thread_followup;

/// Mac 主动出站连接正式本机中继，只在端到端认证后暴露现有授权项目。
///
/// 数据构造：临时安装证书、1 个项目、1 台控制设备；64 个 a 为隔离配对 token。
///   子进程总预算 40s = 启动/注册 10s + 认证负例与读取 15s + 撤权/退出 15s。
/// 执行过程：Node 启动正式网关 → Mac 出站注册 → Node 手机握手/错误认证/正确认证/读取 →
///   截留线程派工并返回 fixture 回执 → 重试/错线程/改正文 → 第二个任务排队后撤权 → 停服。
///   原生专项另有 3 个 Swift 派工，恢复/轮询后仍只能新增 3 次；快照 route 是显式 fixture。
///   Node 与 Swift 分属独立预算主体，先等真实60秒窗口再切换客户端，不放宽生产限额。
///   原任务 test-runtime:7 已持久完成；other-runtime:7 使用同数字 ID、另一节点且仍运行。
/// 预期结果：讨论不派工；同任务只派一次且保留线程绑定；撤权取消排队 lease，停服后不重连。
///   结果只返回原任务的持久回复，重复查询复用事件；其他线程/配对设备不能读取。
///   冻结 result.txt = 20,000 × (结果6 UTF-8字节 + 换行1) = 140,000 字节；JSON字节数组
///   超过160KiB单片，必须分段完整取回，错线程/指纹/任务引用/偏移不得读到正文。
///   验收用独立操作，经原线程校验后复用原验收事务；丢回执重连不得重复执行或改写执行事件。
///   前台回执由测试模拟，仅证明真实网络和执行队列入口，不代表 Coding Agent 已执行。
#[test]
fn outbound_mac_relay_reuses_pairing_and_stops_on_revoke_and_shutdown() {
    run(false, false);
}

/// 正式中继绑定任务接管，重复输入只执行一次，断线不复活旧能力，在途读取期间撤权立即生效。
///
/// 数据构造：原任务test-runtime:7、1台控制手机、2条独立加密连接、文字draft=5字节。
/// 执行过程：原派工→接管/输入/重试/冲突→竞争连接→断开→新连接接管/交还→在途读取时撤权。
/// 预期结果：只有1次输入到前台fixture，旧成功回执不授予新连接输入权，撤权返回前租约已关闭。
/// 层级：正式Node/Rust网络和SQLite回执；前台显式fixture，不算真实PTY或Computer Use验收。
#[test]
fn thread_terminal_control_is_bound_deduplicated_and_revoked_during_pending_read() {
    run(true, false);
}

/// 原任务追问经正式中继进入原会话队列，新回合回复不能被旧任务终态替代。
///
/// 数据构造：原任务test-runtime:7、节点2/review-run/review-session；追问1条，重试2次。
/// 执行过程：查询未发操作→错线程/来源/正文负例→入队→原会话确认新回合→断线查询→撤权。
/// 预期结果：只发送一次，仍为原节点；只返回followup-turn的回复，不返回original-turn成果。
/// 层级：正式网关/Mac网络/SQLite；前台显式fixture，不作为真实Coding Agent验收。
#[test]
fn thread_followup_reuses_original_session_and_returns_only_its_confirmed_turn() {
    run(false, true);
}

fn run(control: bool, followup: bool) {
    let directory = tempfile::tempdir().expect("隔离数据库");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "Relay project")
        .expect("项目")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "86AAD6D7-2A37-4D69-BAF7-03C424336A35",
            "Relay phone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("已配对手机");
    store
        .mobile_access_store()
        .register(
            "relay-reader",
            "Read only",
            &"c".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读手机");
    assert!(
        serde_json::from_value::<app::MobileCommand>(json!({"type":"thread_task",
        "agent_definition_id":2,"task":"绕过来源","context":""}))
        .is_err(),
        "普通命令入口不得反序列化内部线程派工"
    );
    assert!(serde_json::from_value::<app::MobileCommand>(json!({"type":"thread_terminal", "command":{}})).is_err(),
        "普通命令入口不得伪造终端接管能力");
    let identity = norma::mobile::ServerIdentity::generate().expect("Mac 安装身份");
    let fixture = directory.path().join("fixture.json");
    let mut public_fixture = json!({"certificate": identity.certificate_der(),
        "machine_id": identity.machine_id(), "project_id": project});
    let (sender, receiver) = async_channel::bounded::<app::LiveSnapshotRequest>(1);
    let snapshots = std::thread::spawn(move || {
        while let Ok(mut request) = receiver.recv_blocking() {
            for project in &mut request.projects {
                project.state = "open".into();
                project.route_id = Some(if followup { "route-a" } else { "relay-live-route" }.into());
                project.card_count = Some(if followup { 1 } else { 0 });
                project.cards = request.include_cards.then(|| if followup {
                    vec![thread_followup::card()]
                } else { Vec::new() });
            }
            let _ = request.reply.send(request.projects);
        }
    });
    let (commands, dispatched) = async_channel::bounded(8);
    let (terminals, terminal_reads) = async_channel::bounded(8);
    let control_state = std::sync::Arc::new(thread_terminal_control::State::default());
    let (pending, pending_read) = crossbeam_channel::bounded(1);
    let followup_turn = std::sync::Arc::new(std::sync::Mutex::new("original-turn".to_owned()));
    let terminal_worker = if control {
        thread_terminal_control::serve_reads(terminal_reads, control_state.clone(), pending)
    } else if followup { thread_followup::serve(terminal_reads, followup_turn.clone()) }
    else { thread_terminal::serve(terminal_reads) };
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("本机监听")],
        app::MobileSources { snapshots: Some(sender), commands: Some(commands), terminals: Some(terminals), ..Default::default() },
    )
    .expect("Mac 服务");
    public_fixture["endpoints"] = json!([format!("wss://{}/mobile", server.addresses()[0])]);
    std::fs::write(&fixture, public_fixture.to_string()).expect("公开测试身份");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = std::process::Command::new("node")
        .arg(root.join("services/norma-cloud/check-mac-relay.mjs"))
        .arg(&fixture)
        .env("NORMA_TERMINAL_CONTROL_CHECK", if control { "1" } else { "0" })
        .env("NORMA_THREAD_FOLLOWUP_CHECK", if followup { "1" } else { "0" })
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("独立 Node 手机");
    let mut input = child.stdin.take().expect("fixture input");
    let mut output = BufReader::new(child.stdout.take().expect("fixture output"));
    let ready = line(&mut output);
    let port = ready["port"].as_u64().expect("中继端口") as u16;
    server
        .start_local_collaboration_relay_for_e2e(port)
        .expect("出站中继");
    assert!(
        server
            .start_local_collaboration_relay_for_e2e(port)
            .is_err(),
        "同一服务不能重复启动中继连接"
    );
    writeln!(input, "start").expect("开始网络验证");
    assert_eq!(
        line(&mut output)["phase"],
        "task-submitted",
        "线程任务应到达现有执行入口"
    );
    let call = dispatched.try_recv().expect("第一次派工");
    assert_eq!(call.request.workspace_id, project, "目标是原 Mac 项目");
    let command = serde_json::to_value(&call.request.command).expect("任务内容");
    assert_eq!(command["type"], "thread_task", "讨论消息不能走任务入口");
    assert_eq!(
        command["agent_definition_id"], 2,
        "必须保留稳定 Agent 定义 ID"
    );
    assert_eq!(command["task"], "修复首屏导航", "派工正文不能被替换");
    assert!(call.lease.begin(), "合法任务可以开始");
    assert!(!call.lease.begin(), "同任务不能开始两次");
    assert!(
        dispatched.try_recv().is_err(),
        "重试和讨论消息不得产生额外执行"
    );
    // Persist explicit execution fixtures in the actual project store, then reuse task_id in another runtime.
    // This validates identity-safe result transport, not a real Coding Agent run.
    let project_store = storage::ProjectStore::for_database_paths(
        &storage::DatabasePaths::isolated_test(directory.path()),
        directory.path(),
    )
    .expect("隔离项目库");
    for name in ["Alice", "Blair"] {
        project_store
            .agent_definition_store()
            .create(&storage::NewAgentDefinition {
                name: name.into(),
                runtime_profile: canvas::AgentProfile::Codex,
                workflow_description: String::new(),
            })
            .expect("真实定义约束");
    }
    let task_store = project_store.agent_dispatch_task_store();
    let instruction = "工作线程上下文（参考信息）：\n已确认目标：首屏只展示当前任务。\n\n当前任务：\n修复首屏导航";
    let mut task = storage::NewOrchestrationAgentDispatchTask {
        runtime_instance_id: "test-runtime".into(),
        runtime_task_id: 7,
        task_ref: "test-runtime:7".into(),
        root_task_ref: "test-runtime:7".into(),
        parent_task_ref: None,
        source_definition_id: None,
        source_node_id: None,
        source_provider: None,
        target_definition_id: 2,
        target_node_id: 2,
        original_instruction: instruction.into(),
        task_text: instruction.into(),
        state: storage::AgentDispatchTaskState::Submitted,
        tracking: storage::AgentDispatchTracking::Managed,
    };
    task_store.create_orchestration(&task).expect("原任务");
    task_store
        .finish_task_with_result(
            7,
            storage::AgentDispatchTaskState::Done,
            None,
            Some(&storage::StoredAgentDispatchResult {
                text: Some("原任务：首屏导航已修复".into()),
                truncated: false,
            }),
        )
        .expect("原任务持久回复");
    task.runtime_instance_id = "other-runtime".into();
    task.task_ref = "other-runtime:7".into();
    task.root_task_ref = task.task_ref.clone();
    task.target_node_id = 3;
    task_store
        .create_orchestration(&task)
        .expect("新进程复用数字 ID");
    thread_review::seed(&directory.path().join("app.sqlite"), &call.source, "original-turn");
    call.reply
        .send(Ok(
            json!({"status":"accepted","task_id":7,"node_id":2,"task_ref":"test-runtime:7"}),
        ))
        .expect("fixture 前台回执");
    writeln!(input, "task-finished").expect("读取派工回执");
    if followup {
        thread_followup::finish(&server, dispatched, &directory.path().join("app.sqlite"), &mut input, &mut output, &followup_turn);
        assert!(child.wait().expect("手机结束").success(), "追问正式网络断言必须通过");
        drop(server);
        snapshots.join().expect("快照源释放");
        terminal_worker.join().expect("终端源释放");
        return;
    }
    if control {
        thread_terminal_control::finish(&server, dispatched, control_state, pending_read, &mut input, &mut output);
        assert!(child.wait().expect("手机结束").success(), "终端控制真实网络断言必须通过");
        drop(server);
        snapshots.join().expect("快照源释放");
        terminal_worker.join().expect("终端源释放");
        return;
    }
    assert_eq!(line(&mut output)["phase"], "revision-submitted", "修改意见必须成为原线程的独立任务");
    let revision = dispatched.try_recv().expect("一次真实队列入口");
    let app::MobileCommand::ThreadTask { agent_definition_id, task: text, context } = &revision.request.command else {
        panic!("退回必须复用现有任务执行入口");
    };
    assert_eq!(*agent_definition_id, 2, "必须交给原Agent");
    assert_eq!(text, "根据原成果修改：\n把按钮改为蓝色", "修改意见不能丢失");
    assert!(revision.lease.begin(), "修改任务必须持有独立执行权");
    assert!(dispatched.try_recv().is_err(), "重复及错成果不能增加执行");
    task.runtime_instance_id = "revision-runtime".into();
    task.runtime_task_id = 8; task.task_ref = "revision-runtime:8".into(); task.root_task_ref = task.task_ref.clone();
    task.target_node_id = 2;
    task.original_instruction = format!("工作线程上下文（参考信息）：\n{context}\n\n当前任务：\n{text}");
    task.task_text = task.original_instruction.clone();
    task_store.create_orchestration(&task).expect("修改任务持久身份");
    task_store.finish_task_with_result(8, storage::AgentDispatchTaskState::Done, None,
        Some(&storage::StoredAgentDispatchResult { text: Some("修改任务：按钮已改为蓝色".into()), truncated: false }))
        .expect("修改任务fixture回复");
    thread_review::seed(&directory.path().join("app.sqlite"), &revision.source, "revision-turn");
    revision.reply.send(Ok(json!({"status":"accepted","task_id":8,"node_id":2,"task_ref":"revision-runtime:8"})))
        .expect("修改派工回执");
    writeln!(input, "revision-finished").expect("回原线程读取修改成果");
    assert_eq!(
        line(&mut output)["phase"],
        "acceptance-submitted",
        "验收应到达既有前台入口"
    );
    let acceptance = dispatched.try_recv().expect("独立验收命令");
    assert_eq!(
        acceptance.request.workspace_id, project,
        "验收不能换到另一项目"
    );
    assert_eq!(
        acceptance.request.route_id, "accept-route",
        "重试不能覆盖原验收路由"
    );
    assert!(acceptance.lease.begin(), "验收必须先取得可撤销的执行权");
    let app::MobileCommand::AcceptDelivery { key, fingerprint } = &acceptance.request.command
    else {
        panic!("验收必须复用 AcceptDelivery，不能重新派工");
    };
    assert_eq!(key.turn_id, "original-turn", "验收绑定原冻结回合");
    let saved = storage::MobileCommandStore::new(directory.path().join("app.sqlite"))
        .accept_delivery(&acceptance.source, key, fingerprint)
        .expect("Mac 事务保存验收与成功回执");
    assert!(
        dispatched.try_recv().is_err(),
        "重复验收和错误来源不能再入执行队列"
    );
    acceptance
        .reply
        .send(Ok(saved.result()))
        .expect("返回持久验收结果");
    writeln!(input, "acceptance-finished").expect("手机重连核对验收");
    let ui_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ui_worker = std::env::var_os("NORMA_REVIEW_UI_FIXTURE").map(|_| {
        assert!(std::env::var_os("NORMA_NATIVE_RELAY_CHECK").is_none(), "UI与Swift队列探针须独立运行");
        thread_review::serve_ui_commands(dispatched.clone(), directory.path().into(), ui_stop.clone())
    });
    assert_eq!(
        line(&mut output)["phase"],
        "acceptance-readonly",
        "同一来源手机需要验证权限降级"
    );
    ui_stop.store(true, std::sync::atomic::Ordering::Release);
    if let Some(worker) = ui_worker { worker.join().expect("UI fixture前台工作必须正常收口"); }
    let reader = server
        .replace_permissions(
            "86AAD6D7-2A37-4D69-BAF7-03C424336A35",
            1,
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("原设备降为只读");
    assert_eq!(reader.revision, 2, "权限修订从1增加到2");
    writeln!(input, "readonly").expect("检查只读设备");
    assert_eq!(
        line(&mut output)["phase"],
        "acceptance-restore",
        "只读可查记录，不可提交验收"
    );
    let control = server
        .replace_permissions(
            "86AAD6D7-2A37-4D69-BAF7-03C424336A35",
            2,
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("恢复控制权限");
    assert_eq!(control.revision, 3, "恢复权限也必须产生新修订");
    writeln!(input, "control").expect("继续独立撤权用例");
    assert_eq!(
        line(&mut output)["phase"],
        "authenticated",
        "错误凭据必须拒绝，合法手机能读授权项目"
    );
    let native_tasks = if std::env::var("NORMA_NATIVE_RELAY_CHECK").as_deref() == Ok("1") {
        let lost = dispatched.try_recv().expect("Swift 丢回执任务只派一次");
        let fresh = dispatched.try_recv().expect("Swift 持久队列首次派工");
        let later = dispatched.try_recv().expect("Swift 有界轮转派工");
        assert_eq!(
            lost.request.route_id, "swift-before-disconnect",
            "恢复不可换路由重派"
        );
        assert_eq!(
            fresh.request.route_id, "relay-live-route",
            "首次派工取最新快照路由"
        );
        assert_eq!(
            later.request.route_id, "relay-live-route",
            "轮转后仍用当前路由"
        );
        Some((lost, fresh, later))
    } else {
        None
    };
    let pending = dispatched.try_recv().expect("待撤权任务");
    assert!(dispatched.try_recv().is_err(), "冲突请求不可排队");
    server
        .revoke("86AAD6D7-2A37-4D69-BAF7-03C424336A35")
        .expect("实时撤销");
    if let Some((lost, fresh, later)) = native_tasks {
        assert!(
            !lost.lease.begin(),
            "权限收紧后丢回执任务的 lease 必须保持撤销"
        );
        assert!(
            !fresh.lease.begin(),
            "权限收紧后直接恢复派工的 lease 必须保持撤销"
        );
        assert!(
            !later.lease.begin(),
            "权限收紧后批次轮转派工的 lease 必须保持撤销"
        );
        lost.reply
            .send(Err("unauthorized"))
            .expect("结束丢回执任务");
        fresh
            .reply
            .send(Err("unauthorized"))
            .expect("结束首次派工任务");
        later.reply.send(Err("unauthorized")).expect("结束轮转任务");
    }
    assert!(!pending.lease.begin(), "撤权必须取消尚未执行的线程任务");
    pending
        .reply
        .send(Err("unauthorized"))
        .expect("结束被撤任务");
    writeln!(input, "revoked").expect("检查撤权");
    assert_eq!(
        line(&mut output)["phase"],
        "revoked",
        "既有通道必须结束，重连也不能绕过撤权"
    );
    server.shutdown();
    writeln!(input, "stopped").expect("检查退出");
    assert_eq!(
        line(&mut output)["phase"],
        "stopped",
        "停服后 Mac 必须离线且不自动重连"
    );
    assert!(
        child.wait().expect("Node 结束").success(),
        "真实网络检查必须全部通过"
    );
    drop(server);
    snapshots.join().expect("快照源释放");
    terminal_worker.join().expect("终端读取源释放");
}

fn line(reader: &mut impl BufRead) -> Value {
    let mut text = String::new();
    assert!(
        reader.read_line(&mut text).expect("读取测试结果") > 0,
        "子进程不能提前退出"
    );
    serde_json::from_str(&text).expect("测试结果 JSON")
}
