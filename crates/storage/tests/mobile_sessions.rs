use std::{path::PathBuf, time::Duration};
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, MobileSession, MobileSessionKey, MobileTurnStore,
};

fn session(run: &str) -> MobileSession {
    MobileSession {
        key: MobileSessionKey {
            node_id: 7,
            profile: "Claude".into(),
            run_id: run.into(),
            session_id: "session-a".into(),
        },
        workspace_root: PathBuf::from("/tmp/project-a"),
        title: "修复登录".into(),
    }
}

/// 会话索引在任务完成前落盘，不依赖画布节点，重复登记不能更换身份。
///
/// 数据构造：共享AppDB中项目A的run-a、run-b复用同一session；项目B没有登记。
/// 执行过程：登记A → 重复登记 → 错节点/Provider/会话登记 → 重开查询 → 登记第二run。
/// 预期结果：A原始身份保持且第二run独立存在；B、错误节点没有记录，终态表没有假完成事件。
#[test]
fn session_index_is_durable_scoped_and_immutable() {
    let dir = tempfile::tempdir().expect("独立目录");
    let path = dir.path().join("app.sqlite");
    let store = MobileTurnStore::for_workspace(path.clone(), "workspace-a".into());
    let original = session("run-a");
    store.record_session(&original).expect("启动身份提交");
    store.record_session(&original).expect("相同身份幂等");
    let mut conflict = original.clone();
    conflict.key.node_id = 8; // 7是原节点，8是另一个节点，不能认领相同run。
    assert!(
        store.record_session(&conflict).is_err(),
        "另一节点不能认领运行"
    );
    assert!(
        store.session(&conflict.key).expect("错节点查询").is_none(),
        "不能返回其他节点身份"
    );
    conflict = original.clone();
    conflict.key.profile = "Codex".into();
    assert!(
        store.record_session(&conflict).is_err(),
        "Provider不能在同run中替换"
    );
    conflict = original.clone();
    conflict.key.session_id = "session-b".into();
    assert!(
        store.record_session(&conflict).is_err(),
        "不同会话不能覆盖准确绑定"
    );
    let reopened = MobileTurnStore::for_workspace(path.clone(), "workspace-a".into());
    assert_eq!(
        reopened
            .session(&original.key)
            .expect("重新读取")
            .expect("原身份存在")
            .session,
        original,
        "重开必须恢复完整身份与标题"
    );
    reopened
        .record_session(&session("run-b"))
        .expect("恢复同会话的新运行独立登记");
    assert!(
        reopened
            .session(&session("run-b").key)
            .expect("第二run")
            .is_some(),
        "新运行不能覆盖旧run"
    );
    assert!(
        reopened
            .session(&session("run-a").key)
            .expect("原run")
            .is_some(),
        "原运行不能随重开删除"
    );
    assert!(
        MobileTurnStore::for_workspace(path, "workspace-b".into())
            .sessions(None)
            .expect("项目B")
            .is_empty(),
        "跨项目不能枚举会话"
    );
    assert_eq!(
        reopened.latest_sequence().expect("终态水位"),
        0,
        "登记会话不代表任务完成"
    );
}

/// Claude文件引用只能追加到已登记身份；后台writer提交后才可读取，路径不得替换。
///
/// 数据构造：项目审批库和AppDB分离；session-a.jsonl为准确文件，另一目录同名文件为冲突。
/// 执行过程：后台启动登记 → Hook路径入队 → 等待commit → 重复/替换/错会话 → 重开AppDB。
/// 预期结果：只有AppDB保存原文件引用；冲突和错误会话失败，原引用不丢失，不产生正文副本。
#[test]
fn transcript_reference_uses_existing_writer_and_cannot_be_rebound() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let app_path = dir.path().join("app.sqlite");
    let project_path = dir.path().join("project.sqlite");
    let store = MobileTurnStore::for_workspace(app_path, "workspace-a".into());
    let audit = ApprovalAuditStore::new(project_path.clone()).with_turn_store(store.clone());
    let original = session("run-a");
    audit.record_session(&original).expect("后台准备先提交身份");
    let writer = BackgroundApprovalAudit::new(audit).expect("复用串行writer");
    let path = dir.path().join("session-a.jsonl");
    // 5s是既有SQLite回执预算；入队成功不代替commit证据。
    let budget = Duration::from_secs(5);
    writer
        .bind_session_transcript(original.key.clone(), path.clone())
        .expect("路径入队")
        .recv_timeout(budget)
        .expect("路径回执")
        .expect("路径提交");
    writer
        .bind_session_transcript(original.key.clone(), path.clone())
        .expect("重试入队")
        .recv_timeout(budget)
        .expect("重试回执")
        .expect("相同引用幂等");
    writer
        .bind_session_transcript(
            original.key.clone(),
            dir.path().join("other/session-a.jsonl"),
        )
        .expect("冲突入队")
        .recv_timeout(budget)
        .expect("冲突回执")
        .expect_err("相同身份禁止换路径");
    assert!(
        writer
            .bind_session_transcript(original.key.clone(), dir.path().join("session-b.jsonl"))
            .is_err(),
        "文件名必须绑定准确会话"
    );
    writer
        .bind_session_transcript(session("unknown-run").key, path.clone())
        .expect("未登记入队")
        .recv_timeout(budget)
        .expect("未登记回执")
        .expect_err("路径不能自动创建会话身份");
    drop(writer);
    assert_eq!(
        store
            .session(&original.key)
            .expect("重读")
            .expect("有身份")
            .transcript_path,
        Some(path),
        "失败不得覆盖原始路径"
    );
    assert!(
        MobileTurnStore::new(project_path)
            .sessions(None)
            .expect("项目库")
            .is_empty(),
        "配置AppDB后不得额外镜像到项目库"
    );
}

/// 历史索引以有界倒序页读取，重复登记不改变位置，游标不漏掉较旧会话。
///
/// 数据构造：21个run = 首页面长20 + 下一页1；不保存任何聊天正文。
/// 执行过程：按run-0至run-20登记 → 重复run-0 → 读取首页 → 最后sequence读取续页。
/// 预期结果：首页20项且从run-20开始，run-0只在续页出现；负游标域拒绝且末页无重复。
#[test]
fn session_history_pages_without_duplicates() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = MobileTurnStore::new(dir.path().join("project.sqlite"));
    for number in 0..21 {
        store
            .record_session(&session(&format!("run-{number}")))
            .expect("登记");
    }
    store.record_session(&session("run-0")).expect("旧身份幂等");
    let first = store.sessions(None).expect("首页");
    assert_eq!(first.len(), 20, "首页不能无界枚举");
    assert_eq!(first[0].session.key.run_id, "run-20", "最新登记在第一项");
    assert!(
        !first.iter().any(|row| row.session.key.run_id == "run-0"),
        "旧身份重试不能挤掉新记录"
    );
    let next = store
        .sessions(Some(first.last().expect("首页非空").sequence))
        .expect("续页");
    assert_eq!(next.len(), 1, "21减20后应只剩一项");
    assert_eq!(next[0].session.key.run_id, "run-0", "续页必须包含最旧身份");
    assert!(
        store
            .sessions(Some(next[0].sequence))
            .expect("末页")
            .is_empty(),
        "末页不能重复旧身份"
    );
    assert!(
        store.sessions(Some(u64::MAX)).is_err(),
        "超过SQLite有符号范围的游标必须拒绝"
    );
}

/// 原生事件保存同一会话与完成结果，旧回合/其他项目不得认领；改名不改历史身份。
/// 数据构造：workspace-a中run-native/session-a/turn-a；workspace-b为空，foreign使用session-b。
/// 执行过程：登记会话 → 卡片改名后登记完成 → 重复完成 → 错会话提交 → 重开查表。
/// 预期结果：原会话标题保留；turn-a准确存在；foreign和workspace-b中不存在该结果。
#[test]
fn native_events_preserve_session_identity_and_record_exact_turn() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let store = MobileTurnStore::for_workspace(path.clone(), "workspace-a".into());
    let mut original = session("run-native");
    store
        .record_native_event(&original, None, None)
        .expect("原生启动");
    let outcome = storage::MobileTurnOutcome {
        summary: None,
        key: storage::MobileTurnKey {
            node_id: original.key.node_id,
            profile: original.key.profile.clone(),
            run_id: original.key.run_id.clone(),
            session_id: original.key.session_id.clone(),
            turn_id: "turn-a".into(),
        },
        state: storage::MobileTurnState::Completed,
    };
    original.title = "新名称".into();
    store
        .record_native_event(&original, None, Some(&outcome))
        .expect("准确完成");
    store
        .record_native_event(&original, None, Some(&outcome))
        .expect("重复幂等");
    let mut foreign = outcome.clone();
    foreign.key.session_id = "session-b".into();
    assert!(
        store
            .record_native_event(&original, None, Some(&foreign))
            .is_err(),
        "错会话必须拒绝"
    );
    assert_eq!(
        store.get(&outcome.key).expect("原结果"),
        Some(outcome.clone()),
        "完整结果必须落盘"
    );
    assert!(
        store.get(&foreign.key).expect("错会话").is_none(),
        "错会话不能生成结果"
    );
    assert!(
        MobileTurnStore::for_workspace(path, "workspace-b".into())
            .get(&outcome.key)
            .expect("另一项目")
            .is_none(),
        "不能串项目"
    );
    assert_eq!(
        store
            .session(&original.key)
            .expect("会话")
            .expect("仍存在")
            .session
            .title,
        "修复登录",
        "卡片改名不能重写已保存的会话身份"
    );
}
