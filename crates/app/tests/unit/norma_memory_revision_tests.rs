use super::*;
use agent::{MemoryScopeAlias, NormaSession, ProviderConfig, ProviderProtocol};
use serde_json::json;
#[path = "../support/gpt_live_sse.rs"]
mod sse;

struct NoTools;
impl AgentCardAdapter for NoTools {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("测试不调用工具".into())) })
    }
}

/// 场景：另一写入者纠正或删除记忆后，持续会话不得把旧目录和引用旧事实的回答再次发送。
/// 构造：隔离 MemoryAccess/MemoryStore、生产 MemoryAdapter/NormaSession 和四轮本地 SSE。
/// 执行：两轮读取同一版本，随后分别通过设置纠正、底层发布删除；再执行两轮。
/// 预期：版本未变时保留历史；变化后新目录生效且旧回答消失；随后的正常历史继续保留。
#[tokio::test]
async fn memory_revision_invalidates_old_facts_in_actual_model_requests() {
    for delete in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let mut access = storage::MemoryAccess::new(paths.clone(), None);
        let old = "---\nname: OLD-CATALOG-FACT\ndescription: old\n---\n旧事实";
        let saved = access
            .settings_save(
                MemoryScopeAlias::Personal,
                agent::MemoryRevision::INITIAL,
                vec![
                    ("preference".into(), old.into()),
                    ("retained".into(), "保留主题".into()),
                ],
                "原始事实",
                "seed",
            )
            .unwrap();
        let (endpoint, server) = sse::serve(vec![
            json!({"role":"assistant","content":"OLD-ANSWER-FACT"}),
            json!({"role":"assistant","content":"OLD-ANSWER-FACT"}),
            json!({"role":"assistant","content":"CURRENT-ANSWER-FACT"}),
            json!({"role":"assistant","content":"OK"}),
        ])
        .await;
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fixture".into()),
        );
        let host = adapter(Arc::new(NoTools), paths.clone(), None);
        let mut session = NormaSession::new("", &config, host).unwrap();
        session.turn("请查询偏好").await.unwrap();
        session.turn("继续解释").await.unwrap();
        if delete {
            storage::MemoryStore::open(&paths)
                .unwrap()
                .commit(
                    &agent::MemoryScope::Personal,
                    saved.revision,
                    &[agent::MemoryMutation::Delete {
                        id: agent::MemoryEntryId::parse("preference").unwrap(),
                    }],
                )
                .unwrap();
        } else {
            access
                .settings_save(
                    MemoryScopeAlias::Personal,
                    saved.revision,
                    vec![(
                        "preference".into(),
                        "---\nname: NEW-CATALOG-FACT\ndescription: new\n---\n新事实".into(),
                    )],
                    "纠正事实",
                    "correct",
                )
                .unwrap();
        }
        session.turn("现在的偏好呢").await.unwrap();
        session.turn("再解释一下").await.unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 4, "四轮请求均应实际到达本地模型端");
        let messages = |index: usize| requests[index]["messages"].to_string();
        assert!(
            messages(1).contains("OLD-ANSWER-FACT"),
            "未变更时必须保留原对话"
        );
        assert!(
            messages(0).contains("OLD-CATALOG-FACT"),
            "必须确实注入过旧目录"
        );
        assert!(
            !messages(2).contains("OLD-ANSWER-FACT"),
            "纠正或删除后旧回答不能重新进入模型"
        );
        assert!(
            !messages(2).contains("OLD-CATALOG-FACT"),
            "跨适配器缓存不能继续注入旧目录"
        );
        assert_eq!(
            messages(2).contains("NEW-CATALOG-FACT"),
            !delete,
            "纠正载入新目录，删除则不存在"
        );
        assert!(
            messages(3).contains("CURRENT-ANSWER-FACT"),
            "新版本稳定后的历史应继续保留"
        );
    }
}

/// 场景：暖缓存只检查修订元数据，不应逐轮重读所有主题正文。
/// 构造：真实持久化个人主题并预热生产适配器，临时移走该修订的 topics。
/// 执行：复用暖缓存，再显式 read 和冷启动；恢复 topics 后再次 read。
/// 预期：暖缓存版本/目录不变，真正正文访问仍校验完整性，恢复后可读。
#[tokio::test]
async fn warm_memory_revision_check_does_not_read_topic_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut access = storage::MemoryAccess::new(paths.clone(), None);
    let saved = access
        .settings_save(
            MemoryScopeAlias::Personal,
            agent::MemoryRevision::INITIAL,
            vec![("preference".into(), "简洁中文".into())],
            "偏好",
            "seed",
        )
        .unwrap();
    let host = adapter(Arc::new(NoTools), paths.clone(), None);
    let initial = host.memory_context().await.unwrap().unwrap();
    let topics = storage::MemoryStore::open(&paths)
        .unwrap()
        .root()
        .join(format!(
            "personal/revisions/{:020}/topics",
            saved.revision.number()
        ));
    let moved = topics.with_file_name("topics-fixture-hidden");
    std::fs::rename(&topics, &moved).unwrap();
    let warm = host
        .memory_context()
        .await
        .expect("暖缓存版本检查不应读取 topics")
        .unwrap();
    assert_eq!(warm.revisions, initial.revisions, "版本没有改变");
    assert_eq!(warm.catalog, initial.catalog, "继续使用已验证的目录缓存");
    let command = || {
        AdapterCommand::Memory(agent::MemoryCommand::Read {
            scope: MemoryScopeAlias::Personal,
            id: "preference".into(),
            offset: 0,
        })
    };
    let context = ToolContext {
        project_id: String::new(),
        turn_id: "s:read".into(),
        user_input: "查偏好".into(),
        input_source: agent::AgentInputSource::Other,
        contextual_delete_targets: vec![],
        tool_call_id: "read".into(),
        operation_id: None,
        cancellation: Default::default(),
    };
    assert!(
        host.execute(command(), &context).await.is_err(),
        "显式读取必须检查正文完整性"
    );
    assert!(
        adapter(Arc::new(NoTools), paths, None)
            .memory_context()
            .await
            .is_err(),
        "冷启动不可绕过正文完整性检查"
    );
    std::fs::rename(&moved, &topics).unwrap();
    assert!(
        host.execute(command(), &context).await.is_ok(),
        "恢复后显式读取成功"
    );
}
