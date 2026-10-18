use std::time::Duration;
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, MobileTaskDeliveryStore, MobileTurnKey,
    MobileTurnOutcome, MobileTurnState, MobileTurnStore, TaskDeliveryContent,
};
use workspace::review::{ContentKind, FileChange, FileVersion, ProjectReview};

fn key(turn: &str) -> MobileTurnKey {
    MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: turn.into(),
    }
}

/// 项目成果跨卡片倒序分页，新增成果不混进旧游标，另一项目的同身份记录不可串入。
///
/// 数据构造：project-a 的21轮=首页20轮+尾页1轮，交替属于节点7/8；project-b复用turn-20身份。
/// 执行过程：真实writer持久化终态并冻结 → 项目首页 → 添加第22轮 → 用旧游标读尾页 → 单卡/另一项目/非法范围。
/// 预期结果：首页同时包含两卡，尾页仅turn-0；新增turn-21与另一项目指纹不混入，卡片过滤仍准确且无重叠。
#[test]
fn project_delivery_pages_span_cards_without_crossing_workspaces() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let db = dir.path().join("app.sqlite");
    let store = MobileTaskDeliveryStore::new(db.clone());
    let persist = |workspace: &str, index: usize| {
        let writer = BackgroundApprovalAudit::new(
            ApprovalAuditStore::new(dir.path().join(format!("{workspace}.sqlite")))
                .with_turn_store(MobileTurnStore::for_workspace(db.clone(), workspace.into())),
        )
        .expect("正式终态writer");
        let mut turn = key(&format!("turn-{index}"));
        turn.node_id = if index.is_multiple_of(2) { 7 } else { 8 };
        if turn.node_id == 8 {
            turn.profile = "Claude".into();
        }
        //5s为既有后台审计提交预算；测试只保存终态fixture，不运行Provider。
        writer
            .record_turn(MobileTurnOutcome {
                summary: None,
                key: turn.clone(),
                state: MobileTurnState::Completed,
            })
            .expect("入队")
            .recv_timeout(Duration::from_secs(5))
            .expect("回执")
            .expect("落盘");
        store
            .record(
                workspace,
                &turn,
                &TaskDeliveryContent::Available {
                    review: review(b"hello"),
                },
            )
            .expect("冻结成果")
    };
    for index in 0..21 {
        persist("project-a", index);
    }
    let foreign = persist("project-b", 20);
    let first = store.list("project-a", None, None).expect("项目首页");
    assert_eq!(first.len(), 20, "项目首页沿用20项上限");
    assert_eq!(first[0].outcome.key.turn_id, "turn-20", "最新成果应置顶");
    assert_eq!(first[0].outcome.key.node_id, 7, "首页必须包含Codex卡片7");
    assert_eq!(first[1].outcome.key.node_id, 8, "首页必须包含Claude卡片8");
    assert_eq!(
        first[19].outcome.key.turn_id, "turn-1",
        "第一页只包括最新20轮"
    );
    assert!(
        !first
            .iter()
            .any(|item| item.fingerprint == foreign.fingerprint),
        "同回合的其他项目不能混入"
    );
    assert!(
        !first
            .iter()
            .any(|item| item.outcome.key.turn_id == "turn-0"),
        "最早一轮必须留到下一页"
    );
    let latest = persist("project-a", 21);
    let older = store
        .list("project-a", None, Some(first[19].sequence))
        .expect("项目尾页");
    assert_eq!(older.len(), 1, "新成果不能移动已取得的旧游标");
    assert_eq!(older[0].outcome.key.turn_id, "turn-0", "尾页保留最早一轮");
    assert!(
        !older
            .iter()
            .any(|item| item.fingerprint == latest.fingerprint),
        "翻旧页不能插入新成果"
    );
    assert!(
        !older
            .iter()
            .any(|item| first.iter().any(|seen| seen.sequence == item.sequence)),
        "两页不能重复"
    );
    let card = store.list("project-a", Some(7), None).expect("单卡首页");
    assert_eq!(card.len(), 11, "0到20的偶数轮共11轮属于卡片7");
    assert_eq!(card[0].outcome.key.turn_id, "turn-20", "单卡仍按时间倒序");
    assert!(
        !card.iter().any(|item| item.outcome.key.node_id == 8),
        "单卡查询不能扩大范围"
    );
    let other = store.list("project-b", None, None).expect("另一项目");
    assert_eq!(other.len(), 1, "另一项目只保留自己的成果");
    assert_eq!(
        other[0].fingerprint, foreign.fingerprint,
        "同名回合按项目身份区分"
    );
    assert!(
        store
            .list("empty-project", None, None)
            .expect("空项目")
            .is_empty(),
        "未创建成果的项目没有历史"
    );
    assert!(
        store.list("project-a", Some(0), None).is_err(),
        "零节点不能当作全部卡片"
    );
    assert!(
        store.list("project-a", None, Some(0)).is_err(),
        "零游标必须拒绝"
    );
}
fn review(text: &[u8]) -> ProjectReview {
    ProjectReview {
        base_commit: None,
        conflicted: false,
        files: vec![FileChange {
            path: "result.txt".into(),
            status: "??".into(),
            before: None,
            index: None,
            after: Some(FileVersion {
                kind: ContentKind::Text,
                executable: false,
                bytes: text.to_vec(),
            }),
            diff: None,
            staged_diff: None,
        }],
    }
}

/// 概览只读取已完成回合的可打开文件引用，不读入冻结文件字节或推测目录内容。
///
/// 数据构造：同一真实回合含新增 result.txt 和已删除 old.txt，另一个项目有同名文件。
/// 执行过程：持久化终态与成果，按项目查询待看文件引用。
/// 预期结果：只得到当前项目的新增文件，且携带节点、回合与内容指纹。
#[test]
fn reviewable_file_refs_keep_task_source_and_skip_deleted_files() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let db = dir.path().join("app.sqlite");
    let store = MobileTaskDeliveryStore::new(db.clone());
    for workspace in ["project-a", "project-b"] {
        let writer = BackgroundApprovalAudit::new(
            ApprovalAuditStore::new(dir.path().join(format!("{workspace}.sqlite")))
                .with_turn_store(MobileTurnStore::for_workspace(db.clone(), workspace.into())),
        )
        .expect("终态 writer");
        writer
            .record_turn(MobileTurnOutcome {
                summary: None,
                key: key("turn-1"),
                state: MobileTurnState::Completed,
            })
            .expect("入队")
            .recv_timeout(Duration::from_secs(5))
            .expect("回执")
            .expect("落盘");
        let mut changed = review(b"hello");
        changed.files.push(FileChange {
            path: "old.txt".into(),
            status: " D".into(),
            before: Some(FileVersion {
                kind: ContentKind::Text,
                executable: false,
                bytes: b"old".to_vec(),
            }),
            after: None,
            index: None,
            diff: None,
            staged_diff: None,
        });
        store
            .record(
                workspace,
                &key("turn-1"),
                &TaskDeliveryContent::Available { review: changed },
            )
            .expect("冻结成果");
    }
    let files = store
        .reviewable_file_refs("project-a")
        .expect("读取文件引用");
    assert_eq!(files.len(), 1, "删除的文件与另一项目的文件不得混入");
    assert_eq!(files[0].path, "result.txt", "必须保留真实文件路径");
    assert_eq!(files[0].node_id, 7, "必须保留来源 Agent 节点");
    assert_eq!(
        files[0].profile, "Codex",
        "未加载项目仍须保留来源 Agent 类型"
    );
    assert_eq!(files[0].turn_id, "turn-1", "必须保留来源回合");
    assert_eq!(files[0].fingerprint.len(), 64, "内容指纹用于区分文件版本");
    assert_ne!(files[0].workspace_id, "project-b", "项目身份不能串线");
}

/// 同一路径后续变化时，旧标签仍应读取来源回合冻结的文件版本。
///
/// 数据构造：真实完成回合冻结 result.txt 的 v1 字节，来源引用携带指纹。
/// 执行过程：按项目、节点、指纹和路径读取冻结版本，并尝试错误指纹与错误项目。
/// 预期结果：只返回 v1 字节，不能将其他项目或错误版本误认为该成果。
#[test]
fn frozen_file_version_resolves_exact_task_source() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let db = dir.path().join("app.sqlite");
    let store = MobileTaskDeliveryStore::new(db.clone());
    let workspace = "project-a";
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("project.sqlite"))
            .with_turn_store(MobileTurnStore::for_workspace(db, workspace.into())),
    )
    .expect("应创建终态 writer");
    writer
        .record_turn(MobileTurnOutcome {
            summary: None,
            key: key("turn-v1"),
            state: MobileTurnState::Completed,
        })
        .expect("应入队")
        .recv_timeout(Duration::from_secs(5))
        .expect("应收到回执")
        .expect("应保存终态");
    let saved = store
        .record(
            workspace,
            &key("turn-v1"),
            &TaskDeliveryContent::Available {
                review: review(b"version one"),
            },
        )
        .expect("应冻结成果");
    let file = store
        .frozen_file_version(workspace, 7, &saved.fingerprint, "result.txt")
        .expect("应读取冻结版本")
        .expect("版本应存在");
    assert_eq!(file.bytes, b"version one", "必须返回来源回合的冻结字节");
    assert!(
        store
            .frozen_file_version("project-b", 7, &saved.fingerprint, "result.txt")
            .expect("跨项目查询应正常结束")
            .is_none(),
        "其他项目不得复用 A 的成果"
    );
    assert!(
        store
            .frozen_file_version(workspace, 7, "wrong", "result.txt")
            .expect("错误版本查询应正常结束")
            .is_none(),
        "错误指纹不得回退到当前文件"
    );
}

/// 交付固定到已持久化的真实回合，后续内容不能覆盖；失败标记与成果正文不混淆。
///
/// 数据构造：项目a节点7，turn-a真实完成，turn-b真实失败，turn-c没有终态；正文result.txt为hello五字节。
/// 执行过程：保存两条准确终态 → 冻结首轮交付 → 重复/改正文 → 保存失败标记 → 重开/跨项目/错会话/无终态查询。
/// 预期结果：首份正文与指纹固定；不同正文拒绝；失败记录无正文；错身份和无终态不可创建交付。
#[test]
fn delivery_is_frozen_scoped_and_requires_a_real_outcome() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let db = dir.path().join("app.sqlite");
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("project.sqlite")).with_turn_store(
            MobileTurnStore::for_workspace(db.clone(), "project-a".into()),
        ),
    )
    .expect("真实终态writer");
    //5s与现有后台审计测试一致，等commit，不以入队代替成功。
    writer
        .record_turn(MobileTurnOutcome {
            summary: None,
            key: key("turn-a"),
            state: MobileTurnState::Completed,
        })
        .expect("入队a")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执a")
        .expect("commit a");
    writer
        .record_turn(MobileTurnOutcome {
            summary: None,
            key: key("turn-b"),
            state: MobileTurnState::Failed,
        })
        .expect("入队b")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执b")
        .expect("commit b");
    let store = MobileTaskDeliveryStore::new(db.clone());
    let content = TaskDeliveryContent::Available {
        review: review(b"hello"),
    };
    let a = store
        .record("project-a", &key("turn-a"), &content)
        .expect("冻结交付a");
    assert_eq!(a.content, content, "保存原始成果正文");
    assert_eq!(
        a.outcome.state,
        MobileTurnState::Completed,
        "完成来自真实持久化事件"
    );
    assert_eq!(a.fingerprint.len(), 64, "SHA256指纹为32字节×2十六进制字符");
    assert_eq!(
        store
            .record("project-a", &key("turn-a"), &content)
            .expect("幂等"),
        a,
        "刷新不改变交付时间和内容"
    );
    assert!(
        store
            .record(
                "project-a",
                &key("turn-a"),
                &TaskDeliveryContent::Available {
                    review: review(b"later")
                }
            )
            .is_err(),
        "后续修改不能覆盖用户看到的成果"
    );
    let unavailable = TaskDeliveryContent::Unavailable {
        reason: "baseline_missing".into(),
    };
    let b = store
        .record("project-a", &key("turn-b"), &unavailable)
        .expect("明确无起点");
    assert_eq!(
        b.outcome.state,
        MobileTurnState::Failed,
        "失败回合不能变成完成"
    );
    assert_eq!(b.content, unavailable, "无法生成交付不伪造空正文");
    assert!(a.fingerprint != b.fingerprint, "不同回合和内容不能复用指纹");
    assert!(
        store.record("project-a", &key("turn-c"), &content).is_err(),
        "没有真实终态不能伪造交付"
    );
    let page = store
        .list("project-a", Some(7), None)
        .expect("按卡片读取摘要");
    assert_eq!(page.len(), 2, "两条真实交付均可查看");
    assert_eq!(
        page[0].outcome.key.turn_id, "turn-b",
        "倒序优先展示最新交付"
    );
    assert_eq!(
        page[0].status, "baseline_missing",
        "缺失起点不能冒充空文件交付"
    );
    assert_eq!(page[1].file_count, 1, "正文列表只返回文件数量");
    assert_eq!(page[1].fingerprint, a.fingerprint, "摘要指纹绑定已冻结正文");
    let older = store
        .list("project-a", Some(7), Some(page[0].sequence))
        .expect("游标翻页");
    assert_eq!(older.len(), 1, "翻页不能重复最新条目");
    assert_eq!(older[0].outcome.key.turn_id, "turn-a", "旧交付依然可查");
    assert!(
        store
            .list("project-a", Some(8), None)
            .expect("另一卡")
            .is_empty(),
        "另一卡不能误显示本卡成果"
    );
    assert!(
        store
            .list("project-b", Some(7), None)
            .expect("另一项目")
            .is_empty(),
        "摘要也必须按项目隔离"
    );
    let reopened = MobileTaskDeliveryStore::new(db);
    assert_eq!(
        reopened
            .get("project-a", &key("turn-a"))
            .expect("重读")
            .expect("存在"),
        a,
        "进程重开后仍是同一成果"
    );
    assert!(
        reopened
            .get("project-b", &key("turn-a"))
            .expect("另项目")
            .is_none(),
        "不能读出另一项目成果"
    );
    let mut wrong = key("turn-a");
    wrong.session_id = "session-b".into();
    assert!(
        reopened.get("project-a", &wrong).expect("错会话").is_none(),
        "准确run/turn仍不能冒领另一session"
    );
    assert!(
        reopened
            .get("project-a", &key("turn-c"))
            .expect("未观察回合")
            .is_none(),
        "未完成任务不留下交付占位"
    );
}
