use super::*;

fn hook(method: &str, turn: &str, hook_id: &str) -> Value {
    json!({"method":method,"params":{"threadId":"thread-a","turnId":turn,"run":{
        "id":hook_id,"eventName":"permissionRequest","executionMode":"sync","scope":"turn",
        "source":"user","sourcePath":"/tmp/test-hooks.json","status":if method=="hook/started" {"running"} else {"completed"}
    }}})
}

/// 外部 Hook 等待不能冒充手机可批准请求；必须绑定真实回合并在完成或换回合时消失。
///
/// 数据构造：run-a/turn-a，两个同时进行的同步 PermissionRequest Hook，加一个前回合和异步事件。
/// 执行过程：推送 Hook 开始 → 查询 → 伪造手机批准 → 完成一项 → 换回合 → 迟到旧 Hook。
/// 预期结果：准确显示两项外部等待且不生成客户端审批；完成后保留另一项，换回合清空且拒绝旧事件。
#[test]
fn external_hooks_are_visible_but_never_become_mobile_approvals() {
    let (mut runtime, id, _dir, server) = tests::setup();
    // Real Codex IDs embed the Hook configuration path; the phone must receive a redacted ID.
    let private_id = "permission-request:3:/private/test-hooks.json:exec-first";
    let mut asynchronous = hook("hook/started", "turn-a", "async");
    asynchronous["params"]["run"]["executionMode"] = json!("async");
    tests::advance(
        &runtime,
        id,
        vec![
            hook("hook/started", "turn-a", private_id),
            hook("hook/started", "turn-a", "second"),
            hook("hook/started", "old-turn", "old"),
            asynchronous,
        ],
    );
    runtime.drain_all();
    let active = runtime
        .mobile_codex_external_approvals(id, "run-a")
        .expect("当前运行");
    assert_eq!(active.len(), 2, "只应展示当前回合的两项同步外部审批");
    let mobile = runtime.mobile_approval_snapshots(id).expect("手机统一查询");
    assert!(
        !serde_json::to_string(&mobile)
            .expect("手机JSON")
            .contains("/private/"),
        "不只是sourcePath，Hook ID中的配置路径也不能暴露"
    );
    assert_eq!(
        mobile.len(),
        2,
        "手机必须看见两项外部等待，不能伪装成没有事项"
    );
    assert_eq!(
        mobile[0].tool, "external_permission_hook",
        "明确标识外部所有权"
    );
    assert!(
        mobile[0].decisions.is_empty(),
        "外部等待不得提供手机批准选项"
    );
    assert!(
        mobile[1].decisions.is_empty(),
        "第二项也不能生成可执行的决定"
    );
    assert!(
        mobile[0].details.get("sourcePath").is_none(),
        "手机不得收到外部配置文件路径"
    );
    assert_eq!(
        runtime.choose_mobile_approval(&mobile[0].target, ApprovalDecision::Allow),
        Err("decision_unavailable"),
        "统一入口同样拒绝响应外部 Hook"
    );
    assert!(
        active
            .iter()
            .any(|h| h.id == private_id && h.source == "user"),
        "保留第一项来源"
    );
    assert!(active.iter().any(|h| h.id == "second"), "保留第二项");
    assert!(
        !active.iter().any(|h| h.id == "old" || h.id == "async"),
        "旧回合与异步 Hook 不得冒充当前等待"
    );
    assert!(
        runtime
            .mobile_codex_approvals(id)
            .expect("手机审批")
            .is_empty(),
        "Hook 不能自动变成可批准请求"
    );
    assert!(
        runtime
            .decide_mobile_codex_approval(id, "run-a", private_id, ApprovalDecision::Allow)
            .is_err(),
        "手机不能响应一个 Hook ID"
    );
    assert!(
        runtime
            .mobile_codex_external_approvals(id, "old-run")
            .is_err(),
        "旧运行不可查当前等待"
    );
    assert!(
        matches!(
            runtime.agent_task_state(id),
            Some(AgentTaskState::WaitingApproval { .. })
        ),
        "Hook 等待不能显示成仍在执行工具"
    );
    tests::advance(
        &runtime,
        id,
        vec![hook("hook/completed", "turn-a", private_id)],
    );
    runtime.drain_all();
    let active = runtime
        .mobile_codex_external_approvals(id, "run-a")
        .expect("剩余项");
    assert_eq!(active.len(), 1, "只能移除已完成的 Hook");
    assert_eq!(active[0].id, "second", "未完成第二项必须保留");
    tests::advance(
        &runtime,
        id,
        vec![hook("hook/completed", "turn-a", "second")],
    );
    runtime.drain_all();
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Thinking),
        "无 Hook 与客户端审批后返回思考态"
    );
    tests::advance(
        &runtime,
        id,
        vec![
            hook("hook/started", "turn-a", "third"),
            json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-b"}}}),
            hook("hook/started", "turn-a", "late"),
        ],
    );
    runtime.drain_all();
    assert!(
        runtime
            .mobile_codex_external_approvals(id, "run-a")
            .expect("新回合")
            .is_empty(),
        "换回合撤销全部旧 Hook，不接收迟到旧项"
    );
    runtime.kill_all();
    assert!(
        runtime
            .mobile_codex_external_approvals(id, "run-a")
            .is_err(),
        "关闭后不可查询旧运行"
    );
    assert!(
        server.join().expect("服务结束").is_empty(),
        "整个检查不能向 Provider 发送任何权限响应"
    );
}

/// 正式 Codex 启动保留本机外部审批 Hook，并向 Runtime 暴露真实等待来源。
///
/// 数据构造：独立临时目录与一次精确 printf 写入请求；使用用户当前外部 Hook，不批准任何写入。
/// 执行过程：正式 prepare/attach → 最多等 120s（与既有模型冷启动上限相同）→ 观察外部等待 → 正式中断。
/// 预期结果：有外部 Hook 来源、没有伪造客户端审批，临时文件不存在；中断回执只取真实 Provider 终态。
/// 此检查依赖当前用户的外部 PermissionRequest Hook；不关闭、替换或修改任何全局 Hook。
#[test]
#[ignore = "requires Codex/model and an installed blocking external PermissionRequest Hook"]
fn real_codex_external_hook_remains_owned_by_provider() {
    let directory = tempfile::Builder::new()
        .prefix("norma-codex-hook-")
        .tempdir_in("/tmp")
        .expect("隔离目录");
    let mut runtime = Runtime::new_for_workspace(directory.path());
    runtime.set_agent_hook_config(Some(crate::AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "test-token".into(),
        workspace_id: "hook-test".into(),
        workspace_route_id: "hook-route".into(),
        norma_socket: "/tmp/unused.sock".into(),
    }));
    let id = runtime
        .create_mobile_agent_card(
            AgentProfile::Codex,
            None,
            None,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
        .expect("空卡");
    let target = directory.path().join("output.txt");
    let task = format!("This is an I/O test in a disposable directory. Use exec_command exactly once: printf hello-from-norma > {}\nRequest approval for this exact write. Do not run other commands, retry, or delegate. Reply briefly and stop.", target.display());
    // 100×30 是既有真实 Codex PTY 检查尺寸；正式启动仍使用 read-only + untrusted + user。
    let prepared = runtime
        .mobile_agent_launch(id, Path::new("unused"), 100, 30)
        .expect("预约")
        .with_approval_audit(storage::ApprovalAuditStore::new(
            directory.path().join("audit.db"),
        ))
        .with_initial_task(task)
        .prepare()
        .expect("正式启动必须保留外部 Hook");
    runtime.attach_mobile_agent(prepared).expect("正式附着");
    let run = runtime
        .mobile_agent_run_id(id)
        .expect("运行身份")
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(120);
    let active = loop {
        runtime.drain_all();
        let hooks = runtime
            .mobile_codex_external_approvals(id, &run)
            .expect("运行应可用");
        if let Some(hook) = hooks.into_iter().next() {
            break hook;
        }
        assert!(
            Instant::now() < deadline,
            "应观察到本机外部 PermissionRequest Hook 的等待"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(!active.source_path.is_empty(), "真实 Hook 必须保留来源文件");
    assert!(
        !runtime
            .mobile_codex_approvals(id)
            .expect("可控请求")
            .iter()
            .any(|a| a.id == active.id),
        "外部 Hook ID 不得进入手机审批列表"
    );
    assert!(
        runtime
            .decide_mobile_codex_approval(id, &run, &active.id, ApprovalDecision::Allow)
            .is_err(),
        "不能冒充外部 Hook 的所有者"
    );
    assert!(!target.exists(), "未经允许不能写入临时文件");
    let receipt = runtime
        .interrupt_mobile_codex(id, &run, &active.turn_id)
        .expect("正式中断");
    // 30s = 正式 RPC 等待预算；Hook 的 7200s 配置不能拖住用户的停止请求。
    let deadline = Instant::now() + Duration::from_secs(30);
    let outcome = loop {
        runtime.drain_all();
        if let Ok(result) = receipt.try_recv() {
            break result;
        }
        assert!(Instant::now() < deadline, "外部 Hook 等待也必须能取消");
        std::thread::sleep(Duration::from_millis(10));
    };
    std::fs::write("/tmp/norma-ios-codex-external-hook-v12.json", serde_json::to_vec_pretty(&json!({
        "source":active.source,"source_path":active.source_path,"hook_id":active.id,"turn_id":active.turn_id,
        "outcome":outcome,"target_exists":target.exists(),"state":format!("{:?}",runtime.agent_task_state(id))
    })).expect("证据 JSON")).expect("保存真实证据");
    runtime.kill_all();
    assert_eq!(
        outcome.expect("中断成功")["status"],
        "interrupted",
        "只能使用 Provider 真实终态"
    );
    assert!(!target.exists(), "结束后仍不能出现写入");
}
