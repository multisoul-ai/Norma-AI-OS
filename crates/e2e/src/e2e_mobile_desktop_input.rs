use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use std::{path::Path, time::Duration};

/// 手机停止原有Claude卡后继续同一真实会话，停止结果必须先落盘再返回成功回执。
///
/// 数据构造：原生mobile_control=false卡、已信任cwd、独立AppDB；无工具长生成与一条固定续发。
/// 执行过程：原生PTY启动 → 明确生成 → Swift TLS/WSS停止/回查/重放 → 同会话续发 → 核对原回合存储。
/// 预期结果：精确Interrupted终态，原模式/run/session和唯一PTY保留；不得新建受控执行器或误报Completed。
#[test]
#[ignore = "requires real native Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_original_claude_card_stop_and_continue() {
    verify_original_stop(AgentProfile::Claude);
}

/// 原生Codex手机停止通过新增时间线终态确认，并继续同一PTY/session。
/// 数据构造：原生false卡，无工具长回答与短续发；复用Swift精确回执、重放及持久化检查。
/// 执行过程：真实生成 → 手机停止 → Provider确认 → SQLite结果 → 原会话续聊。
/// 预期结果：原回合Interrupted，唯一PTY及模式保留；旧请求不能中断下一回合。
#[test]
#[ignore = "requires real native Codex and NORMA_SWIFT_PROBE"]
fn e2e_original_codex_card_stop_and_continue() {
    verify_original_stop(AgentProfile::Codex);
}

fn verify_original_stop(profile: AgentProfile) {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("原生Swift客户端");
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("已信任仓库");
    let _socket = enter_mobile_canvas(&app, cx, workspace);
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_agent(profile, CanvasRect::new(0., 0., 640., 400.), 80, 24)
            .expect("原生640×400卡/80×24PTY")
    });
    crate::e2e_mobile_messages::prime(&app, cx, node, profile, true);
    let (key, database, workspace_id) = app.read_with(cx, |app, _| {
        (
            app.runtime
                .mobile_latest_turn_key(node)
                .expect("准确原回合"),
            app.app_database_path_for_e2e(),
            app.norma_workspace_identity_for_e2e().expect("项目").0,
        )
    });
    crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &probe, "interrupts");
    let result = storage::MobileTurnStore::for_workspace(database, workspace_id)
        .get(&key)
        .expect("正式终态库")
        .expect("原停止结果");
    assert_eq!(
        result.state,
        storage::MobileTurnState::Interrupted,
        "不能以完成代替原生中断"
    );
    app.update(cx, |app, _| {
        assert_eq!(app.runtime.live_len(), 1, "停止和续发都必须使用唯一原PTY");
        assert_eq!(app.runtime.mobile_agent_run_id(node), Some(key.run_id.as_str()), "不能悄悄重开运行");
        assert_eq!(app.runtime.agent_session_id(node).as_deref(), Some(key.session_id.as_str()), "续发保留原session");
        assert!(matches!(&app.runtime.document.node(node).expect("原卡").kind, canvas::CanvasNodeKind::Agent(spec) if !spec.mobile_control), "不能迁移为受控模式");
        app.runtime.kill_all();
        assert!(app.runtime.mobile_agent_run_id(node).is_none(), "验收关闭后不可继续控制");
    });
}

/// 原有 Claude 终端卡接收手机输入，原生手机读到同一真实会话。
///
/// 数据构造：隔离 AppDB、已信任仓库 cwd；一张原生卡，一条禁止工具/文件操作的固定回复提示。
/// 执行过程：原生创建 → Ready → 手机输入原PTY → 准确Hook回执 → 手机经WSS读取 → 关闭。
/// 预期结果：原生终端始终存在；手机读取准确回复，旧 run 被拒绝；不自动派任务或留下进程。
#[test]
#[ignore = "requires real Claude, bundled Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_desktop_claude_first_prompt_native() {
    verify(AgentProfile::Claude, "NORMA_CLAUDE_CHAT_READY");
}

/// 原有 Codex 终端卡接收手机输入，原生手机读到同一真实会话。
///
/// 数据构造：与 Claude 相同的隔离环境，仅 Provider 和预期固定回复换为 Codex；不配置假终端。
/// 执行过程：真实原生PTY输入 → 准确Hook → 手机只读核验 → 回收。
/// 预期结果：桌面和手机的 session/run 一致；无假提示、无工具操作、无第二张卡。
#[test]
#[ignore = "requires real Codex, bundled Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_desktop_codex_first_prompt_native() {
    verify(AgentProfile::Codex, "NORMA_CODEX_APP_READY");
}

fn verify(profile: AgentProfile, expected: &str) {
    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式使用原生手机测试客户端");
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("仓库");
    let _sockets = enter_mobile_canvas(&app, cx, workspace);
    let node = app.update(cx, |app, cx| {
        let node = app
            .runtime
            .spawn_agent(profile, CanvasRect::new(0.0, 0.0, 640.0, 400.0), 80, 24)
            .expect("创建原生卡片");
        cx.notify();
        node
    });
    // 30s 是原生 CLI 冷启动与 SessionStart 的上限，未收到 Ready 不能输入。
    let ready = drain_until(
        &app,
        cx,
        |runtime| {
            (runtime
                .mobile_card_snapshots()
                .iter()
                .any(|card| card.node_id == node && card.state == "ready"))
            .then_some(())
        },
        Duration::from_secs(30),
    );
    if ready.is_none() {
        dump_terminal(&app, cx, node);
    }
    ready.expect("原生终端 Ready");
    let (session_before, receipt) = app.update(cx, |app, _| {
        let card = app.runtime.mobile_card_snapshots().remove(0);
        let receipt = app
            .runtime
            .send_mobile_message(
                &card,
                format!(
                    "Reply exactly {expected}. Do not use tools, read files, or change anything."
                ),
            )
            .expect("直接输入原终端");
        (card.session_id, receipt)
    });
    // 30s = Provider Hook 确认预算；只接收原会话准确消息，不凭 PTY 写入认定成功。
    let outcome = drain_until(
        &app,
        cx,
        |_| receipt.try_recv().ok(),
        Duration::from_secs(30),
    )
    .expect("原生消息回执");
    if outcome.is_err() {
        dump_terminal(&app, cx, node);
    }
    let result = outcome.expect("Provider 确认");
    if let Some(session) = session_before {
        assert_eq!(
            result["session_id"].as_str(),
            Some(session.as_str()),
            "手机输入必须留在原会话"
        );
    }
    assert!(
        result["session_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "首条消息必须获得真实会话身份"
    );
    // 120s = 冷启动30s + 模型回复90s，不自动操作工具审批。
    drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Done)).then_some(())
        },
        Duration::from_secs(120),
    )
    .expect("真实回复完成");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("agent-start-input").is_none(),
        "必须使用原生卡片，不应有新表单"
    );
    let session = app.read_with(cx, |app, _| {
        assert!(
            app.runtime.node_view(node).is_some(),
            "原生终端必须实际存在"
        );
        assert_eq!(
            app.runtime.mobile_card_snapshots().len(),
            1,
            "重复点击不能创建额外卡片"
        );
        app.runtime
            .agent_session_id(node)
            .expect("真实 Provider 会话")
    });
    crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &probe, "conversation");
    app.update(cx, |app, _| {
        assert_eq!(
            app.runtime.agent_session_id(node),
            Some(session),
            "手机读取不能替换桌面会话"
        );
        app.runtime.kill_all();
        assert!(
            app.runtime.mobile_agent_run_id(node).is_none(),
            "结束测试必须回收受控运行"
        );
    });
}

fn dump_terminal(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node: u64,
) {
    app.read_with(cx, |app, _| {
        eprintln!("native state: {:?}", app.runtime.mobile_card_snapshots());
        if let Some(runtime::NodeView::Term { snapshot, .. }) = app.runtime.node_view(node) {
            for row in &snapshot.rows {
                eprintln!("{}", row.iter().map(|cell| cell.c).collect::<String>());
            }
        }
    });
}

/// 手机对新建原生 Claude 卡连续输入两次，读取同会话完整回复与成果；电脑保持原终端。
/// 数据构造：隔离数据库、原生安全模式、两条禁止工具的提示，含LF/Tab。
/// 执行过程：创建原生卡 → Ready → Swift WSS输入/回执/历史/重复去重 → 回收。
/// 预期结果：一个PTY、两个真实回合，不重开会话、不执行工具或虚构成果终态。
#[test]
#[ignore = "requires real CLI and NORMA_SWIFT_PROBE"]
fn e2e_mobile_native_card_claude_messages() {
    verify_phone(AgentProfile::Claude);
}

/// 手机对新建原生 Codex 卡连续输入两次，验证输入与增量历史保留在原生终端。
/// 数据构造：与Claude同样的隔离环境，Codex使用只读沙箱与逐次审批。
/// 执行过程：原生CLI待输入 → Swift两次真实派发 → 完整历史/成果/回查 → 关闭。
/// 预期结果：没有受控app-server替代会话，没有第二个PTY，旧快照和重复请求不派发。
#[test]
#[ignore = "requires real CLI and NORMA_SWIFT_PROBE"]
fn e2e_mobile_native_card_codex_messages() {
    verify_phone(AgentProfile::Codex);
}

fn verify_phone(profile: AgentProfile) {
    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("原生测试客户端");
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("workspace");
    let _sockets = enter_mobile_canvas(&app, cx, workspace);
    let node = app.update(cx, |app, cx| {
        let node = app
            .runtime
            .spawn_mobile_agent_card(profile, None, CanvasRect::new(0., 0., 640., 400.), 80, 24)
            .expect("原生手机卡");
        cx.notify();
        node
    });
    // 30s 是 CLI 冷启动预算，只读取状态，不向信任对话框输入任何内容。
    drain_until(
        &app,
        cx,
        |runtime| {
            runtime
                .mobile_card_snapshots()
                .iter()
                .any(|card| card.node_id == node && card.state == "ready")
                .then_some(())
        },
        Duration::from_secs(30),
    )
    .expect("原生终端 Ready");
    crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &probe, "native-card");
    app.update(cx, |app, _| {
        assert_eq!(app.runtime.live_len(), 1, "整个手机闭环只有原始PTY");
        app.runtime.kill_all();
    });
}
