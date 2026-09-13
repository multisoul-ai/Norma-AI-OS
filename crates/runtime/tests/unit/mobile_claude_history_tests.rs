use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::json;

/// 读取路径只能来自当前运行的已验证 Hook，关闭后已取出的后台读取请求也必须失效。
///
/// 数据构造：一个 Claude echo PTY，session-a/run-a；临时 projects 目录有一条 3000 字符回复。
/// 执行过程：附着 Runtime → SessionStart 绑定 transcript → 旧运行替换路径失败 → 后台读取完整页 → kill_all → 再读。
/// 预期结果：准确绑定原文件且正文不裁剪；错误 run 无读取能力，关闭后的旧读取对象返回错误而非历史内容。
#[test]
fn claude_history_binds_verified_hook_and_revokes_inflight_read() {
    let directory = tempfile::tempdir().expect("测试目录");
    let (mut runtime, id) = super::super::launch_tests::runtime_with_card();
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused"), 80, 24)
        .expect("应预约");
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = TermLive::spawn_with_factory(
        launch.generation,
        PtyCommand::new("unused"),
        80,
        24,
        &factory,
    )
    .expect("应构造终端");
    runtime
        .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
        .expect("应附着");
    let path = directory.path().join("session-a.jsonl");
    let text = "完整回复".repeat(750); // 4 字 * 750 = 3000 字，超过原摘要 1200 字符。
    std::fs::write(&path,format!("{}\n",json!({"type":"assistant","uuid":"reply-a","sessionId":"session-a","cwd":"/tmp","message":{"role":"assistant","content":[{"type":"text","text":text}]}}))).expect("应构造会话文件");
    let event=parse_hook_event(HookSource::Claude,id,json!({"hook_event_name":"SessionStart","session_id":"session-a","_norma_control_run_id":"run-a","transcript_path":path})).expect("Hook JSON");
    assert!(
        runtime.observe_mobile_hook(&event),
        "准确的 SessionStart 应绑定路径"
    );
    let mut foreign = event.clone();
    foreign.control_run_id = Some("old-run".into());
    foreign.raw_json["transcript_path"] = json!("/tmp/elsewhere.jsonl");
    assert!(
        !runtime.observe_mobile_hook(&foreign),
        "旧运行不得替换历史路径"
    );
    assert!(
        runtime.mobile_claude_history(id, "old-run").is_err(),
        "错误运行不能读取"
    );
    let mut read = runtime
        .mobile_claude_history(id, "run-a")
        .expect("应获得延迟读取");
    // 只替换测试 Provider 的配置根；不修改进程 HOME/CLAUDE_CONFIG_DIR 或用户数据。
    read.projects_root = directory.path().into();
    let page = read.read(None, 20).expect("应读取完整历史");
    assert_eq!(
        page.messages[0].content[0]["text"], text,
        "完整回复不能被摘要截断"
    );
    assert_eq!(page.messages[0].id, "reply-a", "旧 Hook 不得串到其他文件");
    let mut delayed = runtime
        .mobile_claude_history(id, "run-a")
        .expect("应获得另一次延迟读取");
    delayed.projects_root = directory.path().into();
    runtime.kill_all();
    assert!(
        delayed.read(None, 20).is_err(),
        "窗口关闭后排队中的读操作也必须撤销"
    );
    assert!(
        runtime.mobile_claude_history(id, "run-a").is_err(),
        "关闭后不能再创建旧会话读取"
    );
}

/// 接受准确Hook后立即删卡，原会话文件引用仍须经既有writer保存，旧Hook不能创建记录。
///
/// 数据构造：一个echo终端、run-a/session-a启动身份；无模型调用，无正文复制。
/// 执行过程：登记准确启动 → 附着 → 错运行Hook → 当前SessionStart → 立即删卡 → 重开库读取。
/// 预期结果：错运行没有索引；原路径持久化且卡片已删除，不能靠运行中内存提供历史。
#[test]
fn claude_reference_survives_immediate_card_deletion() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let (mut runtime, id) = super::super::launch_tests::runtime_with_card();
    let audit_path = directory.path().join("project.sqlite");
    let audit = storage::ApprovalAuditStore::new(audit_path.clone());
    let identity = storage::MobileSession {
        key: storage::MobileSessionKey {
            node_id: id,
            profile: "Claude".into(),
            run_id: "run-a".into(),
            session_id: "session-a".into(),
        },
        workspace_root: PathBuf::from("/tmp"),
        title: "Claude".into(),
    };
    audit
        .record_session(&identity)
        .expect("模拟后台准备提交身份");
    let writer = super::super::claude::ClaudeApprovals::new(audit).expect("后台writer");
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused"), 80, 24)
        .expect("预约");
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = TermLive::spawn_with_factory(
        launch.generation,
        PtyCommand::new("unused"),
        80,
        24,
        &factory,
    )
    .expect("echo终端");
    runtime
        .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
        .expect("准确附着");
    runtime.mobile_runs.get_mut(&id).expect("运行").claude = Some(writer);
    let path = directory.path().join("session-a.jsonl");
    let event = parse_hook_event(HookSource::Claude,id,json!({"hook_event_name":"SessionStart","session_id":"session-a","_norma_control_run_id":"run-a","transcript_path":path})).expect("当前Hook");
    let mut wrong = event.clone();
    wrong.control_run_id = Some("wrong-run".into());
    assert!(!runtime.observe_mobile_hook(&wrong), "错误运行不能记录引用");
    assert!(runtime.observe_mobile_hook(&event), "准确Hook应可记录引用");
    runtime.remove_node(id);
    let store = storage::MobileTurnStore::new(audit_path);
    // 5s为既有本地writer预算；10ms只用于测试等待，生产前台不轮询SQLite。
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let saved = loop {
        let saved = store
            .session(&identity.key)
            .expect("重开查询")
            .expect("启动身份保留");
        if saved.transcript_path.is_some() || Instant::now() >= deadline {
            break saved;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(
        saved.transcript_path,
        Some(path),
        "删除后的后台队列仍须提交原引用"
    );
    assert!(runtime.document.node(id).is_none(), "持久化不能复活卡片");
    let mut wrong = identity.key;
    wrong.run_id = "wrong-run".into();
    assert!(
        store.session(&wrong).expect("错误运行查询").is_none(),
        "错误Hook不能补登记伪身份"
    );
}

/// 空会话的等待游标在文件首次写入后能继续读取，已读文件丢失不能伪装成空会话。
///
/// 数据构造：临时projects目录，绑定尚不存在的session-a.jsonl；随后写入一条独立已知正文。
///   20条是生产首屏页大小，实际构造1条消息，故has_more=false。
/// 执行过程：准确Hook绑定路径 → 空页 → 文件首次写入 → 用等待游标读取 → 删除文件 → 用真实游标重读。
/// 预期结果：首条正文完整出现、等待游标被真实文件游标替换；文件丢失和非法分页仍然报错。
#[test]
fn pending_claude_history_continues_when_first_record_is_written() {
    let directory = tempfile::tempdir().expect("隔离projects目录");
    let (mut runtime, node) = super::super::launch_tests::runtime_with_card();
    // 80×24为标准终端网格；仅创建内存echo后端，不启动真实Provider。
    let launch = runtime
        .mobile_agent_launch(node, Path::new("unused"), 80, 24)
        .expect("预约");
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = TermLive::spawn_with_factory(
        launch.generation,
        PtyCommand::new("unused"),
        80,
        24,
        &factory,
    )
    .expect("内存PTY");
    runtime
        .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
        .expect("附着");
    let path = directory.path().join("session-a.jsonl");
    let event = parse_hook_event(HookSource::Claude, node, json!({"hook_event_name":"SessionStart", "session_id":"session-a", "_norma_control_run_id":"run-a", "transcript_path":path})).expect("准确Hook");
    assert!(runtime.observe_mobile_hook(&event), "只绑定准确会话路径");
    let mut read = runtime
        .mobile_claude_history(node, "run-a")
        .expect("后台读取");
    read.projects_root = directory.path().into();
    let pending = read.read(None, 20).expect("尚未创建文件应返回空会话");
    assert!(pending.messages.is_empty(), "无文件不能编造正文");
    assert!(
        pending.cursor.is_waiting_for_file(),
        "等待游标不能冒用文件身份"
    );
    assert!(!pending.has_more, "无文件不能触发无限分页");
    // 0小于合法页大小下限1；即便空会话也不能跳过参数校验。
    assert!(read.read(None, 0).is_err(), "非法页大小必须拒绝");
    std::fs::write(&path, format!("{}\n", json!({"type":"user", "uuid":"first-message", "sessionId":"session-a", "cwd":"/tmp", "message":{"role":"user", "content":"你好"}}))).expect("首次Provider写入");
    let page = read
        .read(Some(pending.cursor), 20)
        .expect("应从第一条正文开始读取");
    assert_eq!(page.messages.len(), 1, "首条消息不能漏读或重复");
    assert_eq!(page.messages[0].content, json!("你好"), "准确保留首次正文");
    assert!(!page.cursor.is_waiting_for_file(), "必须换成真实文件游标");
    std::fs::remove_file(path).expect("模拟已读文件丢失");
    assert!(
        read.read(Some(page.cursor), 20).is_err(),
        "已读历史丢失不能被报告为空会话"
    );
    runtime.kill_all();
}
