use super::*;
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, MemoryScopeAlias, ToolContext,
};
use futures::future::BoxFuture;

struct NoTools;
impl AgentCardAdapter for NoTools {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("测试不执行工具".into())) })
    }
}

async fn prepare_turn(
    chat: &Arc<ChatRuntime>,
    requests: &async_channel::Receiver<ContextRequest>,
    turn: u64,
) {
    let chat = chat.clone();
    let (status, _received) = async_channel::unbounded();
    let task = tokio::spawn(chat.respond(
        format!("本轮-{turn}"),
        turn,
        CancellationToken::new(),
        status,
    ));
    // Production asks for fresh environment only after loading memory and preparing history.
    let ContextRequest::Model(reply) =
        tokio::time::timeout(std::time::Duration::from_secs(2), requests.recv())
            .await
            .expect("必须实际经过生产上下文准备")
            .unwrap();
    drop(reply); // End at the environment boundary, before any network/TTS work.
    assert!(
        task.await.unwrap().is_err(),
        "夹具应在环境交接边界终止，不伪造网络成功"
    );
}

/// 场景：豆包持续对话在另一个写入者纠正记忆后，不得沿用引用旧事实的回复。
/// 构造：隔离记忆库、真实 MemoryAdapter、ChatRuntime 历史和无硬件音频队列。
/// 执行：先准备一轮以建立版本，再纠正记忆并准备下一轮；在环境交接处终止，避免网络。
/// 预期：未变更时保留历史；纠正后清除旧事实并保留当前用户输入，后续稳定轮不再清空。
#[tokio::test]
async fn doubao_memory_revision_clears_cached_reply() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut access = storage::MemoryAccess::new(paths.clone(), None);
    let saved = access
        .settings_save(
            MemoryScopeAlias::Personal,
            agent::MemoryRevision::INITIAL,
            vec![("preference".into(), "旧事实".into())],
            "原始事实",
            "seed",
        )
        .unwrap();
    let host = crate::app::memory::adapter(Arc::new(NoTools), paths.clone(), None);
    let (context, requests) = async_channel::unbounded();
    let (bus, _wake) = crate::app::voice_window_inset::VoiceWindowInsetEventBus::new();
    let playback = Arc::new(crate::app::doubao::playback::start_playback_with(
        || Box::new(crate::app::voice::playback::RecordingAssistantAudioSink::new()),
        bus.sink(crate::app::voice_window_inset::TtsPlaybackSource::Doubao),
    ));
    let chat = Arc::new(ChatRuntime {
        client: Client::new(),
        key: "fixture".into(),
        voice: ::tts::providers::doubao::default_voice(),
        context,
        history: Mutex::new(vec![
            json!({"role":"user", "content":"旧问句"}),
            json!({"role":"assistant", "content":"OLD-FACT"}),
        ]),
        memory_epoch: Default::default(),
        memory_revisions: Default::default(),
        tools: Some(Arc::new(NormaToolRuntime::new(host))),
        project: None,
        database: paths.app_db_path().to_owned(),
        session: "fixture".into(),
        playback: playback.clone(),
    });
    prepare_turn(&chat, &requests, 1).await;
    assert!(
        chat.history
            .lock()
            .await
            .iter()
            .any(|v| v["content"] == "OLD-FACT"),
        "首轮未发生版本变化，应保留历史"
    );
    access
        .settings_save(
            MemoryScopeAlias::Personal,
            saved.revision,
            vec![("preference".into(), "新事实".into())],
            "纠正事实",
            "correct",
        )
        .unwrap();
    prepare_turn(&chat, &requests, 2).await;
    let history = chat.history.lock().await.clone();
    assert!(
        !history.iter().any(|v| v["content"] == "OLD-FACT"),
        "新请求不能再带已纠正的旧回答"
    );
    assert_eq!(
        history,
        vec![json!({"role":"user", "content":"本轮-2"})],
        "只保留当前输入，旧整轮一起失效"
    );
    prepare_turn(&chat, &requests, 3).await;
    assert_eq!(
        chat.history.lock().await.len(),
        2,
        "版本稳定后不能每轮清空历史"
    );
    playback.end();
    tokio::time::timeout(std::time::Duration::from_secs(2), playback.wait_stopped())
        .await
        .expect("无硬件队列应及时停止");
}
