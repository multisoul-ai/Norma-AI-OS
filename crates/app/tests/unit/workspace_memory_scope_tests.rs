use super::*;
use agent::{MemoryRevision, MemoryScopeAlias};

struct NoTools;
impl AgentCardAdapter for NoTools {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("此测试不读取正文".into())) })
    }
}

/// 场景：工作空间改为另一项目，恰好同版本号也必须失效；仅重排项目不应清历史。
/// 构造：两个独立目录各发布一版项目记忆，共用生产 workspace_adapter。
/// 执行：读 A 的版本，再切 B；随后以 A/B 和 B/A 读取同一集合。
/// 预期：A 与 B 版本号相同但来源身份不同；重排稳定；所有默认目录均不拼入项目内容。
#[tokio::test]
async fn workspace_memory_revisions_include_identity_and_ignore_area_order() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    for root in [&a, &b] {
        std::fs::create_dir(root).unwrap();
        let mut access = storage::MemoryAccess::new(paths.clone(), Some(root.clone()));
        access
            .settings_save(
                MemoryScopeAlias::Project,
                MemoryRevision::INITIAL,
                vec![("secret-project-topic".into(), "PRIVATE-PROJECT-BODY".into())],
                "种子",
                &root.display().to_string(),
            )
            .unwrap();
    }
    let roots = Arc::new(Mutex::new(vec![a.clone()]));
    let adapter = workspace_adapter(Arc::new(NoTools), paths, roots.clone());
    let first = adapter.memory_context().await.unwrap().unwrap();
    *roots.lock().unwrap() = vec![b.clone()];
    let second = adapter.memory_context().await.unwrap().unwrap();
    assert_eq!(
        first
            .revisions
            .iter()
            .map(|(_, revision)| *revision)
            .collect::<Vec<_>>(),
        second
            .revisions
            .iter()
            .map(|(_, revision)| *revision)
            .collect::<Vec<_>>(),
        "两个不同项目必须刻意使用相同发布版本，才能覆盖身份碰撞"
    );
    assert_ne!(
        first.revisions, second.revisions,
        "不能用 project/worktree 别名和数字版本冒充项目身份"
    );
    *roots.lock().unwrap() = vec![a.clone(), b.clone()];
    let ordered = adapter.memory_context().await.unwrap().unwrap();
    *roots.lock().unwrap() = vec![b, a];
    let reordered = adapter.memory_context().await.unwrap().unwrap();
    assert_eq!(
        ordered.revisions, reordered.revisions,
        "相同项目集合的排列变化不能误清历史"
    );
    for context in [first, second, ordered, reordered] {
        assert!(
            !context.catalog.contains("secret-project-topic"),
            "项目主题仍需按需工具读取"
        );
        assert!(
            !context.catalog.contains("PRIVATE-PROJECT-BODY"),
            "默认请求不能拼入项目正文"
        );
    }
}
