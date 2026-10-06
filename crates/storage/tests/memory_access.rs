use agent::{AgentInputSource, MemoryCommand, MemoryScopeAlias, ToolContext};
use storage::{DatabasePaths, MemoryAccess};

fn context(input: &str, turn: &str) -> ToolContext {
    ToolContext {
        project_id: String::new(),
        turn_id: turn.into(),
        user_input: input.into(),
        input_source: AgentInputSource::Other,
        contextual_delete_targets: vec![],
        tool_call_id: "memory-call".into(),
        operation_id: Some(format!("{turn}:memory-call")),
        cancellation: Default::default(),
    }
}

/// 记住的中文偏好有真实来源，重开后可读取；个人会话不能访问项目范围。
///
/// 数据构造：
///   - 临时独立数据根，个人范围，无项目授权。
///   - 原话为“请记住：我喜欢简洁中文”；正文为相同偏好。
///
/// 执行过程：
///   1. 激活空个人记忆，写入 preferences。
///   2. 新建访问会话，读取已发布主题及来源。
///
/// 预期结果：
///   - 正断言：中文正文持久存在，原话可经来源查询返回。
///   - 负断言：无项目不能访问 project。
#[test]
fn explicit_memory_is_persistent_with_source() {
    let temp = tempfile::tempdir().expect("应创建独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = MemoryAccess::new(paths.clone(), None);
    access.activate().expect("应激活空个人记忆");
    let saved = access
        .execute(
            MemoryCommand::Write {
                scope: MemoryScopeAlias::Personal,
                id: "preferences".into(),
                markdown: "请用简洁中文交流。".into(),
            },
            &context("请记住：我喜欢简洁中文", "session-a:turn-1"),
        )
        .expect("明确请求应持久保存");
    assert!(saved.revision.number() > 0, "保存必须返回真实发布版本");
    let mut reopened = MemoryAccess::new(paths, None);
    reopened.activate().expect("应在新会话激活目录");
    let read = reopened
        .execute(
            MemoryCommand::Read {
                scope: MemoryScopeAlias::Personal,
                id: "preferences".into(),
                offset: 0,
            },
            &context("我的交流偏好是什么？", "session-b:turn-1"),
        )
        .expect("新会话应读取已保存主题");
    assert_eq!(
        read.markdown.as_deref(),
        Some("请用简洁中文交流。"),
        "重开不得丢失中文正文"
    );
    let source = reopened
        .source_details(&read.sources)
        .expect("应核验真实来源");
    assert_eq!(
        source[0].text, "请记住：我喜欢简洁中文",
        "来源必须是用户实际原话"
    );
    let snapshot = reopened
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("应获取人工管理快照");
    assert!(
        snapshot
            .entries()
            .any(|item| item.id.as_str() == "preferences"),
        "已明确保存的偏好应存在"
    );
    assert!(
        reopened
            .settings_snapshot(MemoryScopeAlias::Project)
            .is_err(),
        "个人会话不得获得项目范围"
    );
}

/// 是否写入由模型通过 function call 决定，宿主不按关键词判断意图；只有 save 开关能拒绝。
///
/// 数据构造：
///   - 五个主题 id 各配一句措辞各异的原话，均不以“记住”等触发词开头：
///     口语前缀、句尾触发、含“是否/能不能”、英文冒号、移动端 @mobile-turn 包装。
///   - 每次写入使用独立 turn，故每轮工具预算 8192 字节足够。
///
/// 执行过程：
///   1. 激活空个人记忆。
///   2. 依次以各原话写入对应主题。
///   3. 关闭 save 后再写入第六个主题。
///
/// 预期结果：
///   - 正断言：五个主题全部保存，且来源保留各自原话。
///   - 负断言：save=false 时写入被拒，第六个主题不存在。
#[test]
fn write_depends_on_model_call_not_phrasing_but_respects_save_switch() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = MemoryAccess::new(paths, None);
    access.activate().expect("激活");
    let utterances = [
        ("polite", "嗯，以后请记住我喜欢简洁中文"),
        ("suffix", "我的构建命令是 cargo build，记住它"),
        ("words", "记住：提交前是否通过 clippy 都要先跑一遍"),
        ("english", "Remember: I prefer concise replies"),
        (
            "mobile",
            "@mobile-turn:op-1\n你正在与用户讨论这个工作空间。\n用户消息：请记住我用 Mac",
        ),
    ];
    for (index, (id, text)) in utterances.iter().enumerate() {
        let saved = access
            .execute(
                MemoryCommand::Write {
                    scope: MemoryScopeAlias::Personal,
                    id: (*id).into(),
                    markdown: format!("内容 {index}"),
                },
                &context(text, &format!("s:turn-{index}")),
            )
            .unwrap_or_else(|error| panic!("{text:?} 应由模型决定写入而非被关键词拒绝：{error:?}"));
        assert!(saved.saved, "{id} 应返回 saved=true");
        let read = access
            .execute(
                MemoryCommand::Read {
                    scope: MemoryScopeAlias::Personal,
                    id: (*id).into(),
                    offset: 0,
                },
                &context("读取", &format!("r:turn-{index}")),
            )
            .expect("读取刚保存的主题");
        let sources = access.source_details(&read.sources).expect("来源");
        assert_eq!(sources[0].text, *text, "{id} 来源应保留当轮输入");
    }
    let snapshot = access
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("快照");
    for (id, _) in utterances {
        assert!(
            snapshot.entries().any(|item| item.id.as_str() == id),
            "{id} 应已持久保存"
        );
    }
    access
        .set_preferences(agent::MemoryPreferences {
            recall: true,
            save: false,
        })
        .expect("关闭保存");
    let rejected = access.execute(
        MemoryCommand::Write {
            scope: MemoryScopeAlias::Personal,
            id: "after-off".into(),
            markdown: "不应保存".into(),
        },
        &context("请记住：不应保存", "s:turn-9"),
    );
    assert!(rejected.is_err(), "save=false 时必须拒绝写入");
    let snapshot = access
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("快照");
    assert!(
        !snapshot
            .entries()
            .any(|item| item.id.as_str() == "after-off"),
        "被拒绝的主题不得出现"
    );
}

/// 旧主题必须先读，人工纠正后过期对话不能覆盖；关闭开关不妨碍人工编辑。
///
/// 数据构造：个人主题从“中文”经设置页改成“英文”；两个服务在版本 1 打开。
/// 执行过程：1. 人工发布中文。2. 对话只 list 后更新遭拒；read 后设置页发布英文。
///   3. 对话用旧基线写“法文”发生冲突。4. 关闭读取与保存后人工改成“中文”。
/// 预期结果：英文存在且法文不存在；关闭后工具拒绝，人工管理正常。
#[test]
fn stale_dialogue_cannot_overwrite_manual_correction() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut manual = MemoryAccess::new(paths.clone(), None);
    let initial = manual
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("空快照");
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            initial.revision(),
            vec![("language".into(), "中文".into())],
            "手动新增",
            "edit-1",
        )
        .expect("人工新增");
    let mut dialogue = MemoryAccess::new(paths, None);
    dialogue.activate().expect("加载版本 1");
    let write = || MemoryCommand::Write {
        scope: MemoryScopeAlias::Personal,
        id: "language".into(),
        markdown: "法文".into(),
    };
    assert!(
        dialogue
            .execute(write(), &context("请纠正记忆：法文", "s:1"))
            .is_err(),
        "旧主题未经 read 不可覆盖"
    );
    dialogue
        .execute(
            MemoryCommand::Read {
                scope: MemoryScopeAlias::Personal,
                id: "language".into(),
                offset: 0,
            },
            &context("查看偏好", "s:2"),
        )
        .expect("读取旧主题");
    let (version, _) = manual
        .settings_read(MemoryScopeAlias::Personal, "language")
        .expect("读取草稿基线");
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            version,
            vec![("language".into(), "英文".into())],
            "手动纠正",
            "edit-2",
        )
        .expect("人工纠正");
    assert!(
        dialogue
            .execute(write(), &context("请纠正记忆：法文", "s:3"))
            .is_err(),
        "过期写入必须冲突"
    );
    let (version, entry) = manual
        .settings_read(MemoryScopeAlias::Personal, "language")
        .expect("最新内容");
    assert_eq!(entry.markdown, "英文", "最新人工纠正必须保留");
    assert_ne!(entry.markdown, "法文", "冲突不得覆盖最新正文");
    manual
        .set_preferences(agent::MemoryPreferences {
            recall: false,
            save: false,
        })
        .expect("关闭开关");
    assert!(
        dialogue
            .execute(
                MemoryCommand::List {
                    scope: MemoryScopeAlias::Personal,
                    offset: 0
                },
                &context("查看记忆", "s:4")
            )
            .is_err(),
        "读取关闭应拒绝旧会话"
    );
    assert!(
        dialogue
            .execute(write(), &context("请记住：法文", "s:5"))
            .is_err(),
        "保存关闭应拒绝明确请求"
    );
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            version,
            vec![("language".into(), "中文".into())],
            "人工管理不受开关影响",
            "edit-3",
        )
        .expect("人工保存仍可用");
}

/// 读取旧主题后仅刷新目录，不能把旧正文作为新版本覆盖；中文搜索与原子导入正常。
///
/// 数据构造：主题 id=notes；两次人工编辑分别为“简洁中文”与“详细英文”。
///   无效导入含一个有效主题和一个空正文，整组必须失败。
/// 执行过程：1. 对话读取旧 notes。2. 设置纠正后对话 list。3. 尝试旧正文写入。
///   4. 中文无空格搜索。5. 无效多文件导入后检查当前库。
/// 预期结果：新版本存在、旧值和无效导入不存在；搜索命中中文正文。
#[test]
fn refreshing_catalog_does_not_authorize_stale_topic_overwrite() {
    let temp = tempfile::tempdir().expect("独立根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut manual = MemoryAccess::new(paths.clone(), None);
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            agent::MemoryRevision::INITIAL,
            vec![("notes".into(), "简洁中文".into())],
            "新增",
            "e1",
        )
        .expect("新增");
    let mut chat = MemoryAccess::new(paths, None);
    chat.activate().expect("激活");
    chat.execute(
        MemoryCommand::Read {
            scope: MemoryScopeAlias::Personal,
            id: "notes".into(),
            offset: 0,
        },
        &context("读记忆", "t:1"),
    )
    .expect("读取");
    let found = chat
        .execute(
            MemoryCommand::Search {
                scope: MemoryScopeAlias::Personal,
                query: "简洁中文".into(),
                offset: 0,
            },
            &context("搜索", "t:2"),
        )
        .expect("搜索");
    assert_eq!(
        found.entries[0].id.as_str(),
        "notes",
        "无空格中文必须匹配正文"
    );
    let (version, _) = manual
        .settings_read(MemoryScopeAlias::Personal, "notes")
        .expect("草稿");
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            version,
            vec![("notes".into(), "详细英文".into())],
            "纠正",
            "e2",
        )
        .expect("纠正");
    chat.execute(
        MemoryCommand::List {
            scope: MemoryScopeAlias::Personal,
            offset: 0,
        },
        &context("看目录", "t:3"),
    )
    .expect("刷新目录");
    assert!(
        chat.execute(
            MemoryCommand::Write {
                scope: MemoryScopeAlias::Personal,
                id: "notes".into(),
                markdown: "简洁中文".into()
            },
            &context("请记住：简洁中文", "t:4")
        )
        .is_err(),
        "目录刷新不能冒充读取了新正文"
    );
    let (version, entry) = manual
        .settings_read(MemoryScopeAlias::Personal, "notes")
        .expect("当前");
    assert_eq!(entry.markdown, "详细英文", "纠正后的正文应保留");
    assert!(
        manual
            .settings_save(
                MemoryScopeAlias::Personal,
                version,
                vec![
                    ("valid".into(), "有效".into()),
                    ("invalid".into(), "".into())
                ],
                "导入",
                "e3"
            )
            .is_err(),
        "一项无效必须拒绝整组"
    );
    let snapshot = manual
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("快照");
    assert!(
        !snapshot.entries().any(|item| item.id.as_str() == "valid"),
        "有效项也不能部分发布"
    );
    assert!(
        !snapshot.entries().any(|item| item.id.as_str() == "invalid"),
        "空项不应发布"
    );
}

/// 取消不写入；有界正文提供正确续读，版本变化后续读失败而非拼接两个版本。
///
/// 数据构造：“中文偏好”重复 1000 次，共 4×3×1000=12000 字节，超过单工具 8192 字节预算。
/// 执行过程：1. 已取消请求写 ghost。2. 人工保存长文。3. 读取首块并检查 JSON 大小。
///   4. 人工发布“新内容”，使用旧 offset 续读。
/// 预期结果：ghost 不存在；首块有续读且不超预算；旧续读必须拒绝。
#[test]
fn cancelled_write_and_versioned_long_read() {
    let temp = tempfile::tempdir().expect("独立根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = MemoryAccess::new(paths.clone(), None);
    access.activate().expect("激活");
    let cancelled = context("请记住：不存在", "s:1");
    cancelled.cancellation.cancel();
    assert!(
        access
            .execute(
                MemoryCommand::Write {
                    scope: MemoryScopeAlias::Personal,
                    id: "ghost".into(),
                    markdown: "不存在".into()
                },
                &cancelled
            )
            .is_err(),
        "取消后不应写入"
    );
    access
        .settings_save(
            MemoryScopeAlias::Personal,
            agent::MemoryRevision::INITIAL,
            vec![("long".into(), "中文偏好".repeat(1000))],
            "长文编辑",
            "long-edit",
        )
        .expect("长文保存");
    let read = access
        .execute(
            MemoryCommand::Read {
                scope: MemoryScopeAlias::Personal,
                id: "long".into(),
                offset: 0,
            },
            &context("读取", "s:2"),
        )
        .expect("首块");
    let offset = read.next_offset.expect("长文应有续读位置");
    assert!(
        serde_json::to_vec(&read).expect("编码").len() <= agent::MEMORY_TOOL_BYTES,
        "整个结果不得超预算"
    );
    assert!(
        read.markdown.as_ref().expect("正文").len() < 12000,
        "首块不能冒充全文"
    );
    let mut manual = MemoryAccess::new(paths, None);
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            read.revision,
            vec![("long".into(), "新内容".into())],
            "纠正",
            "long-edit-2",
        )
        .expect("更新长文");
    assert!(
        access
            .execute(
                MemoryCommand::Read {
                    scope: MemoryScopeAlias::Personal,
                    id: "long".into(),
                    offset
                },
                &context("续读", "s:3")
            )
            .is_err(),
        "不同版本不能拼接"
    );
    assert!(
        !access
            .settings_snapshot(MemoryScopeAlias::Personal)
            .expect("快照")
            .entries()
            .any(|entry| entry.id.as_str() == "ghost"),
        "取消主题不得出现"
    );
}
