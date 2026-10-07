use super::*;
use agent::{MemoryCommand, MemoryScopeAlias};

struct NoProjectTools;
impl AgentCardAdapter for NoProjectTools {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("个人会话无项目工具".into())) })
    }
}

fn context(turn: &str) -> ToolContext {
    ToolContext {
        project_id: String::new(),
        turn_id: turn.into(),
        user_input: "请合并重复中文偏好".into(),
        input_source: agent::AgentInputSource::Other,
        contextual_delete_targets: vec![],
        tool_call_id: "memory-call".into(),
        operation_id: Some(turn.into()),
        cancellation: Default::default(),
    }
}

/// 合并旧记忆后，同一对话及另一活跃适配器的下一轮目录不得继续注入已经删除的主题。
///
/// 数据构造：独立数据根；old-language 与 language 两份中文偏好；两个适配器缓存初始目录。
/// 执行过程：1. 首轮获取记忆上下文。2. 完整读取两份主题。3. 原子合并并删除旧主题。
///   4. 两个适配器再次获取记忆上下文。
/// 预期结果：两份下一轮目录有 language，无 old-language；首轮目录确实包含 old-language。
#[tokio::test]
async fn successful_merge_refreshes_the_next_turn_catalog() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let mut manual = storage::MemoryAccess::new(paths.clone(), None);
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            agent::MemoryRevision::INITIAL,
            vec![
                ("old-language".into(), "中文".into()),
                ("language".into(), "简洁中文".into()),
            ],
            "历史重复偏好",
            "seed",
        )
        .expect("初始化重复主题");
    let other = adapter(Arc::new(NoProjectTools), paths.clone(), None);
    let other_initial = other
        .memory_context()
        .await
        .expect("另一活跃会话上下文")
        .expect("已启用记忆");
    assert!(
        other_initial.catalog.contains("old-language"),
        "另一会话必须实际缓存旧主题"
    );
    let adapter = adapter(Arc::new(NoProjectTools), paths, None);
    let initial = adapter
        .memory_context()
        .await
        .expect("首轮上下文")
        .expect("已启用记忆");
    assert!(
        initial.catalog.contains("old-language"),
        "首轮必须实际缓存重复旧主题"
    );
    for (id, turn) in [
        ("old-language", "s:read-old"),
        ("language", "s:read-current"),
    ] {
        adapter
            .execute(
                AdapterCommand::Memory(MemoryCommand::Read {
                    scope: MemoryScopeAlias::Personal,
                    id: id.into(),
                    offset: 0,
                }),
                &context(turn),
            )
            .await
            .expect("完整读取合并主题");
    }
    let command = serde_json::from_value(serde_json::json!({
        "action":"write", "scope":"personal", "id":"language",
        "markdown":"简洁中文", "supersedes":["old-language"]
    }))
    .expect("合并工具参数");
    let saved = adapter
        .execute(AdapterCommand::Memory(command), &context("s:merge"))
        .await
        .expect("合并保存");
    assert!(saved.memory.expect("记忆回执").saved, "必须已实际发布合并");
    let next = adapter
        .memory_context()
        .await
        .expect("下一轮上下文")
        .expect("已启用记忆");
    assert!(next.catalog.contains("language.md"), "保留主题应继续注入");
    assert!(
        !next.catalog.contains("old-language"),
        "已删除旧主题不能留在持续会话的缓存目录"
    );
    let other_next = other
        .memory_context()
        .await
        .expect("另一会话下一轮上下文")
        .expect("已启用记忆");
    assert!(
        other_next.catalog.contains("language.md"),
        "其他活跃会话应注入保留主题"
    );
    assert!(
        !other_next.catalog.contains("old-language"),
        "其他活跃会话也不能注入已删除主题"
    );
    // recall_epoch=0：合并只刷新目录，没有发生关闭读取，不能撤销普通对话历史。
    assert_eq!(
        other_next.recall_epoch, 0,
        "合并不能撤销其他活跃会话的普通对话历史"
    );
}
