use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use serde_json::json;
use std::{fs, path::Path, time::Duration};

/// Codex原生卡应通过真实工具事件披露两步计划，不另开执行器。
///
/// 数据构造：1张100×40原生卡，2个只在计划工具内的步骤，不读写项目或运行命令；90s=模型75s+Hook15s。
/// 执行过程：空卡就绪 → 同PTY提交计划要求 → 收集实际Hook → 准确终态 → 关闭本次PTY。
/// 预期结果：唯一run/PTY不变，有准确提交/结束；原始工具载荷保留用于证明进度数据来源。
#[test]
#[ignore = "requires real Codex and an explicit NORMA_PROGRESS_EVIDENCE directory"]
fn native_codex_plan_hook_evidence() {
    verify(AgentProfile::Codex);
}

/// Claude原生卡应通过真实任务管理工具披露两步计划，不读写项目代码。
///
/// 数据构造：1张100×40原生卡，2个任务工具步骤；90s=模型75s+Hook15s，启动独立30s。
/// 执行过程：复用原生Claude启动 → 提交 → 观察TaskCreate/TaskUpdate等真实工具载荷 → 结束并回收。
/// 预期结果：仅原会话运行，有准确首尾事件；证据不能用模拟数据或其他用户会话替代。
#[test]
#[ignore = "requires real Claude and an explicit NORMA_PROGRESS_EVIDENCE directory"]
fn native_claude_plan_hook_evidence() {
    verify(AgentProfile::Claude);
}

fn verify(profile: AgentProfile) {
    let evidence = std::env::var("NORMA_PROGRESS_EVIDENCE").expect("显式证据目录");
    let output = Path::new(&evidence);
    assert!(
        output.is_absolute() && output.is_dir(),
        "只能写明确存在的证据目录"
    );
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("已信任仓库");
    let _socket = enter_mobile_canvas(&app, cx, workspace);
    let events = app.update(cx, |app, _| app.observe_agent_hook_events_for_e2e());
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_mobile_agent_card(profile, None, CanvasRect::new(0., 0., 640., 400.), 100, 40)
            .expect("隔离原生卡")
    });
    let ready = drain_until(
        &app,
        cx,
        |r| {
            (r.mobile_card_snapshots()[0].state == "ready")
                .then_some(r.mobile_card_snapshots().remove(0))
        },
        Duration::from_secs(30),
    );
    if ready.is_none() {
        app.update(cx, |app, _| app.runtime.kill_all());
    }
    let initial = ready.expect("原卡必须先就绪");
    let tool = if profile == AgentProfile::Codex {
        "update_plan"
    } else {
        "TaskCreate and TaskUpdate"
    };
    let prompt = format!("This is an isolated plan-tool protocol check. Use your built-in {tool} tool(s), invoking them through the JavaScript tool orchestration wrapper if required. This protocol check requires three actual plan-tool updates; merely printing the final marker is not sufficient. Create exactly two steps: 'Check alpha' and 'Check beta'. First mark alpha in progress and beta pending. Then mark alpha completed and beta in progress. Finally mark both completed. Do not inspect, read or modify project files. Do not run shell commands, skills, agents, or any other tools. Finish with exactly NORMA_PLAN_DONE.");
    let sent = app.update(cx, |app, _| {
        app.runtime.send_mobile_message(&initial, prompt)
    });
    if sent.is_err() {
        app.update(cx, |app, _| app.runtime.kill_all());
    }
    let receipt = sent.expect("原PTY可接受输入");
    let result = drain_until(
        &app,
        cx,
        |r| {
            let card = r.mobile_card_snapshots().remove(0);
            (matches!(card.state.as_str(), "done" | "failed" | "ended")
                && card.task_timing.is_some())
            .then_some(card)
        },
        Duration::from_secs(90),
    );
    let observed: Vec<_> = events.try_iter().map(|e| json!({"provider_event":e.provider_event,"tool_name":e.tool_name,"session_id":e.session_id,"turn_id":e.turn_id,"raw_json":e.raw_json})).collect();
    let path = output.join(format!(
        "{}-plan-hooks.json",
        profile.display_title().to_lowercase()
    ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&observed).expect("证据JSON"),
    )
    .expect("保留实际事件");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("证据仅当前用户可读");
    let received = receipt.try_recv();
    let state = app.read_with(cx, |app, _| {
        let screen = app.runtime.node_view(node).map(|view| {
            let runtime::NodeView::Term { snapshot, .. } = view;
            snapshot.rows.iter().map(|row| row.iter().map(|c| c.c).collect::<String>()).collect::<Vec<_>>()
        });
        json!({"card":app.runtime.mobile_card_snapshots(),"screen":screen,"receipt":format!("{received:?}")})
    });
    let state_path = output.join("plan-final-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&state).expect("终态诊断"),
    )
    .expect("保留明确探针诊断");
    fs::set_permissions(&state_path, fs::Permissions::from_mode(0o600))
        .expect("只允许当前用户读取");
    app.update(cx, |app, _| app.runtime.kill_all());
    let final_card = result.expect("90s内必须结束计划工具检查");
    assert_eq!(final_card.run_id, initial.run_id, "不得另开执行器或运行");
    assert_eq!(final_card.state, "done", "必须真实完成，失败不能冒充进度");
    assert_eq!(
        final_card.task_summary.as_deref(),
        Some("NORMA_PLAN_DONE"),
        "原生Stop的真实回复须进入原卡摘要"
    );
    assert!(
        !final_card
            .task_summary
            .as_deref()
            .unwrap_or("")
            .contains("Check alpha"),
        "完成回复不能残留工具步骤正文"
    );
    if profile == AgentProfile::Claude {
        let progress = final_card
            .task_progress
            .as_ref()
            .expect("真实TaskCreate/TaskUpdate须投影计划");
        assert_eq!(progress.total, 2, "真实工具创建两个步骤");
        assert_eq!(progress.completed, 2, "两次准确完成必须反映在快照");
        assert!(
            progress.current_step.is_none(),
            "全完成不能仍标记正在执行步骤"
        );
    } else {
        assert!(
            final_card.task_progress.is_none(),
            "当前Codex未提供计划工具，最终文字不能冒充步骤"
        );
    }
    assert_eq!(
        received.expect("实际回执").expect("已接受")["session_id"].as_str(),
        final_card.session_id.as_deref(),
        "输入和结果属于原会话"
    );
    assert!(
        observed
            .iter()
            .any(|e| e["provider_event"] == "UserPromptSubmit"),
        "必须有真实提交"
    );
    assert!(
        observed.iter().any(|e| e["provider_event"] == "Stop"),
        "必须有真实结束"
    );
    assert!(
        !observed.iter().any(|e| matches!(
            e["tool_name"].as_str(),
            Some("Bash" | "exec_command" | "Write" | "Edit")
        )),
        "只验证计划工具，不得执行文件或命令任务"
    );
    println!(
        "PLAN_HOOK_EVIDENCE provider={} events={} tools={}",
        profile.display_title(),
        observed.len(),
        json!(observed
            .iter()
            .filter_map(|e| e["tool_name"].as_str())
            .collect::<Vec<_>>())
    );
    app.read_with(cx, |app, _| {
        assert!(
            app.runtime.mobile_agent_run_id(node).is_none(),
            "本次隔离进程必须回收"
        )
    });
}
