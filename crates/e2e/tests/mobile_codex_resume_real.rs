use canvas::{AgentProfile, CanvasRect};
use runtime::{NodeInput, NodeView, Runtime};
use serde_json::json;
use std::path::Path;
use std::time::{Duration, Instant};
use terminal::{Key, KeyInput, PtyCommand, PtyHandle};

fn screen(runtime: &Runtime, node: u64) -> String {
    let Some(NodeView::Term { snapshot, .. }) = runtime.node_view(node) else {
        return String::new();
    };
    let total = snapshot.history_size + snapshot.lines;
    // 与手机相同的120行尾部上限；100列×120行最多12000个字符。
    runtime
        .node_terminal_rows(node, total.saturating_sub(120), total.saturating_sub(1))
        .unwrap_or_default()
        .iter()
        .map(|row| row.iter().map(|c| c.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn wait(runtime: &mut Runtime, node: u64, predicate: impl Fn(&Runtime) -> bool) {
    // 30s复用现有真实Codex冷启动预算；每10ms推进一次PTY，最多3000次。
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        runtime.drain_all();
        if predicate(runtime) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("真实Codex未到达预期状态：{}", screen(runtime, node));
}

/// 真实Codex空卡通过/resume恢复隔离历史后，手机无需先发桌面消息即可输入。
///
/// 数据构造：1个临时CODEX_HOME与1张100×24原PTY；构造130行历史（130>尾部上限120）。
///   仅复用登录凭据的私有符号链接，不复制凭据、不读写用户会话，不发送模型任务。
/// 执行过程：启动真实Codex → 等待空卡ready → /resume指定隔离UUID → 确认历史标记已上屏
///   → 验证手机ready/可发送及旧run失效 → 统一回收本测试PTY。
/// 预期结果：没有SessionStart也能从恢复画面就绪；不虚构session，不更换终端，不触发LLM任务。
/// 层级：L2真实CLI/PTY，使用公开Runtime接口，不是启动真实.app或iOS真机验收。
#[test]
#[ignore = "requires authenticated Codex CLI; optional NORMA_CODEX_BIN selects the installed executable"]
fn real_codex_resume_empty_card_is_ready_without_a_bootstrap_prompt() {
    let directory = tempfile::tempdir().expect("隔离测试目录");
    let workspace = directory.path().canonicalize().expect("真实路径");
    let codex_home = workspace.join("codex-home");
    let sessions = codex_home.join("sessions/2026/09/13");
    std::fs::create_dir_all(&sessions).expect("隔离会话目录");
    let original_home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("用户目录")).join(".codex")
        });
    if original_home.join("auth.json").exists() {
        std::os::unix::fs::symlink(
            original_home.join("auth.json"),
            codex_home.join("auth.json"),
        )
        .expect("仅私有符号链接复用登录");
    }
    std::fs::write(codex_home.join("config.toml"), format!(
        "model = \"gpt-6-astra\"\ncheck_for_update_on_startup = false\n[features]\napps = false\n[projects.{}]\ntrust_level = \"trusted\"\n",
        serde_json::to_string(&workspace).expect("安全引用临时路径"),
    )).expect("独立配置，不载入用户Hooks或MCP");
    let session = "01a09881-b00f-76f3-8ef3-38973481c02f";
    let timestamp = "2026-09-13T02:03:51.000Z";
    let history = [
        json!({"timestamp":timestamp,"type":"session_meta","payload":{
            "session_id":session,"id":session,"timestamp":timestamp,"cwd":workspace,"originator":"codex_cli_rs",
            "cli_version":"0.154.0","source":"cli","model_provider":"openai",
            "thread_source":"user","history_mode":"paginated",
            "context_window":{"window_id":"01a09881-b00f-76f3-8ef3-38a56f797e7f"}
        }}),
        json!({"timestamp":timestamp,"type":"event_msg","payload":{
            "type":"task_started", "turn_id":"01a09882-f711-7d80-80e4-45f50188f24e", "model_context_window":null,
            "collaboration_mode_kind":"default"
        }}),
        json!({"timestamp":timestamp,"type":"event_msg","payload":{
            "type":"item_completed", "thread_id":session, "turn_id":"01a09882-f711-7d80-80e4-45f50188f24e",
            "started_at_ms":1789265117000_i64,"completed_at_ms":1789265117000_i64,
            "item":{"type":"UserMessage", "id":"fixture-user", "client_id":null,
                "content":[{"type":"text","text":"fixture only","text_elements":[]}]}
        }}),
        json!({"timestamp":timestamp,"type":"event_msg","payload":{
            "type":"item_completed", "thread_id":session, "turn_id":"01a09882-f711-7d80-80e4-45f50188f24e",
            "started_at_ms":1789265117000_i64,"completed_at_ms":1789265117000_i64,
            "item":{"type":"AgentMessage", "id":"fixture-agent", "phase":"final_answer",
                "content":[{"type":"Text","text":"norma-resume-fixture-line\n".repeat(130)}]}
        }}),
        json!({"timestamp":timestamp,"type":"response_item","payload":{
            "type":"message", "id":null,"role":"user",
            "content":[{"type":"input_text","text":"fixture only"}]
        }}),
        json!({"timestamp":timestamp,"type":"response_item","payload":{
            "type":"message", "id":null,"role":"assistant","phase":"final_answer",
            "content":[{"type":"output_text","text":"norma-resume-fixture-line\n".repeat(130)}]
        }}),
        json!({"timestamp":timestamp,"type":"event_msg","payload":{
            "type":"task_complete", "turn_id":"01a09882-f711-7d80-80e4-45f50188f24e", "last_agent_message":"norma-resume-fixture-line"
        }}),
    ]
    .iter()
    .enumerate()
    .map(|(ordinal, value)| {
        let mut value = value.clone();
        value["ordinal"] = json!(ordinal);
        value.to_string()
    })
    .collect::<Vec<_>>()
    .join("\n")
        + "\n";
    std::fs::write(
        sessions.join(format!("rollout-2026-09-13T10-03-51-{session}.jsonl")),
        history,
    )
    .expect("仅写入人工构造的隔离历史");
    let mut runtime = Runtime::new_for_workspace(&workspace);
    let bin = std::env::var("NORMA_CODEX_BIN").unwrap_or_else(|_| "codex".into());
    let cwd = workspace.clone();
    runtime.set_pty_factory(Box::new(move |_, cols, rows, tx| {
        let command = PtyCommand::new(&bin)
            .args([
                "--sandbox",
                "read-only",
                "--ask-for-approval",
                "never",
                "-c",
                "disable_paste_burst=true",
            ])
            .env("CODEX_HOME", codex_home.to_string_lossy().into_owned())
            .with_cwd(Some(Path::new(&cwd)));
        Ok(Box::new(PtyHandle::spawn_command(command, cols, rows, tx)?))
    }));
    // 100×24是真CLI探针网格，640×400仅作卡片创建坐标，不参与行为判定。
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            100,
            24,
        )
        .expect("唯一真实Codex终端");
    let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait(&mut runtime, node, |r| {
            r.mobile_card_snapshots()[0].state == "ready"
        });
        let before = runtime.mobile_card_snapshots().remove(0);
        assert!(
            !screen(&runtime, node).contains("norma-resume-fixture-line"),
            "新卡不得提前含有恢复历史"
        );
        runtime
            .send_node_input(node, NodeInput::Text(format!("/resume {session}")))
            .expect("真实/resume指令");
        runtime
            .send_node_input(
                node,
                NodeInput::Key(KeyInput {
                    key: Key::Enter,
                    ctrl: false,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("提交本地恢复命令");
        wait(&mut runtime, node, |r| {
            screen(r, node).contains("norma-resume-fixture-line")
                && r.mobile_card_snapshots()[0].state == "ready"
        });
        let resumed = runtime.mobile_card_snapshots().remove(0);
        assert_eq!(resumed.state, "ready", "真实/resume完成后手机应就绪");
        assert_ne!(resumed.run_id, before.run_id, "恢复后旧运行身份必须失效");
        assert!(resumed.session_id.is_none(), "无Hook不得猜测会话ID");
        assert_eq!(
            runtime.validate_mobile_message(&resumed, "continue"),
            Ok(()),
            "无需发送初始化任务也能从手机输入"
        );
        assert_eq!(runtime.live_len(), 1, "不能替换成第二个执行器");
    }));
    runtime.kill_all();
    if let Err(error) = checked {
        std::panic::resume_unwind(error);
    }
}
