use super::*;
use std::io::Write;
use tokio_util::sync::CancellationToken;

struct FileEffect {
    path: std::path::PathBuf,
    wrote: async_channel::Sender<()>,
    release: async_channel::Receiver<()>,
    fail_after_effect: bool,
}

impl AgentCardAdapter for FileEffect {
    fn execute<'a>(&'a self, _: AdapterCommand, _: &'a ToolContext) -> BoxFuture<'a, Outcome> {
        Box::pin(async move {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .expect("隔离副作用文件应可打开");
            writeln!(file, "created-node-42").expect("实际写入副作用");
            self.wrote.send(()).await.expect("应通知已产生副作用");
            self.release.recv().await.expect("应允许返回真实回执");
            if self.fail_after_effect {
                Err(AdapterError::Failed("副作用后的通信失败".into()))
            } else {
                Ok(AdapterResult::succeeded("已创建测试卡片").with_node_id(42))
            }
        })
    }
}

fn context() -> ToolContext {
    ToolContext {
        project_id: "isolated-project".into(),
        turn_id: "stable-intent:gpt-live:r2".into(),
        user_input: "创建一个终端".into(),
        input_source: agent::AgentInputSource::GptLiveVoice,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "changing-model-call-id".into(),
        operation_id: Some("stable-intent:gpt-live:r2:step-1".into()),
        cancellation: CancellationToken::new(),
    }
}

/// AC06/AC07：只读查询不进入副作用账本，也不消耗后续修订的写操作序号。
/// 构造：计数的只读Adapter、两个revision，复用相同step-1。
/// 执行：两次读取任务；检查实际调用、账本文件与effects标志。
/// 预期：允许刷新读取两次，无副作用账本或effects；原文授权由Bridge独立保留。
#[tokio::test]
async fn gpt_live_durable_read_only_queries_do_not_seal_a_write_intent() {
    struct ReadTasks(AtomicUsize);
    impl AgentCardAdapter for ReadTasks {
        fn execute<'a>(
            &'a self,
            command: AdapterCommand,
            _: &'a ToolContext,
        ) -> BoxFuture<'a, Outcome> {
            Box::pin(async move {
                assert!(
                    matches!(command, AdapterCommand::FindAgentTasks { .. }),
                    "夹具只接受只读查询"
                );
                self.0.fetch_add(1, Ordering::AcqRel);
                Ok(AdapterResult::succeeded("读取了当前任务"))
            })
        }
    }
    let directory = tempfile::tempdir().expect("隔离目录");
    let database = directory.path().join("reads.sqlite");
    let delegate = Arc::new(ReadTasks(AtomicUsize::new(0)));
    let adapter = LiveDurableAdapter::new(
        delegate.clone(),
        database.clone(),
        "isolated-project".into(),
        "stable-intent".into(),
        "owner".into(),
    );
    for _ in 0..2 {
        adapter
            .execute(
                AdapterCommand::FindAgentTasks {
                    task_id: None,
                    agent: None,
                    states: Vec::new(),
                },
                &context(),
            )
            .await
            .expect("只读刷新");
    }
    assert_eq!(
        delegate.0.load(Ordering::Acquire),
        2,
        "只读结果必须可刷新，不能被旧持久回执替换"
    );
    assert!(!adapter.has_effects(), "只读回执不能封存未执行的写授权");
    assert!(!database.exists(), "只读查询不需要持久副作用认领");
    assert_eq!(adapter.receipts().len(), 2, "事实回执仍要供Live引用");
}

/// AC07：调用方在真实文件副作用发生后被中断，后台仍须落账并在重启后返回原node。
/// 数据构造：私有临时库、实际append文件，以及阻塞在写入之后的Adapter。
/// 执行过程：执行→等文件写入→取消调用方future→放行回执→重建owner并重放同操作。
/// 预期结果：只写一行；持久回执保留node42；模型call_id变化不能造成再次执行。
#[tokio::test]
async fn gpt_live_durable_receipt_survives_dropped_caller_and_owner_restart() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let directory = tempfile::tempdir().expect("隔离目录");
        let database = directory.path().join("app.sqlite");
        let effect = directory.path().join("effect.txt");
        let (wrote, written) = async_channel::bounded(1);
        let (release, wait) = async_channel::bounded(1);
        let delegate: Arc<dyn AgentCardAdapter> = Arc::new(FileEffect {
            path: effect.clone(),
            wrote,
            release: wait,
            fail_after_effect: false,
        });
        let adapter = Arc::new(LiveDurableAdapter::new(
            delegate.clone(),
            database.clone(),
            "isolated-project".into(),
            "stable-intent".into(),
            "owner-before".into(),
        ));
        let caller = tokio::spawn({
            let adapter = adapter.clone();
            async move {
                adapter
                    .execute(AdapterCommand::CreateTerminal, &context())
                    .await
            }
        });
        written.recv().await.expect("实际文件副作用已发生");
        assert!(
            adapter.is_pending(),
            "副作用已经发生但未有回执时必须保留执行中票据"
        );
        assert!(
            adapter.receipts().is_empty(),
            "空回执列表不能被宿主误当成确定未执行"
        );
        let (pending, receipts) = adapter.completion_snapshot();
        assert!(
            pending && receipts.is_empty(),
            "一致完成快照必须把已执行但未返回的操作保留为pending"
        );
        caller.abort();
        let _ = caller.await;
        release.send(()).await.expect("允许实际回执返回");
        let original = loop {
            let results = adapter.receipts();
            if let Some(result) = results.first() {
                break result.clone();
            }
            tokio::task::yield_now().await;
        };
        assert_eq!(
            original,
            Ok(AdapterResult::succeeded("已创建测试卡片").with_node_id(42)),
            "取消调用方也要保存真实结果"
        );
        let restarted = LiveDurableAdapter::new(
            delegate,
            database,
            "isolated-project".into(),
            "stable-intent".into(),
            "owner-after".into(),
        );
        let mut replay = context();
        replay.tool_call_id = "new-model-id".into();
        replay.turn_id = "other-delegation:gpt-live:r5".into();
        replay.operation_id = Some("other-delegation:gpt-live:r5:step-1".into());
        let result = restarted
            .execute(AdapterCommand::CreateTerminal, &replay)
            .await
            .expect("重放应返回原回执");
        assert_eq!(result.node_id, Some(42), "重启不能丢失实际node身份");
        assert_eq!(
            std::fs::read_to_string(effect).expect("副作用文件"),
            "created-node-42\n",
            "跨owner重复操作只能产生一行真实副作用"
        );
        assert!(written.try_recv().is_err(), "重放不能再次进入实际Adapter");
    })
    .await
    .expect("中断落账与重放应有界完成");
}

/// AC07：真实副作用后Failed代表unknown，模型换call_id/step也不能重做。
/// 数据构造：会写真实文件再Failed的Adapter，持久包装和真实ToolCallPolicy。
/// 执行过程：第一步创建终端失败；同一回合用新ID/step再次调用创建终端。
/// 预期结果：unknown锁住后续工具，文件只有一行，不把失败误当确定未执行。
#[tokio::test]
async fn gpt_live_durable_unknown_stops_new_steps_after_partial_effect() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let effect = directory.path().join("effect.txt");
    let (wrote, _written) = async_channel::bounded(2);
    let (release, wait) = async_channel::bounded(2);
    release.send(()).await.expect("放行第一个结果");
    release
        .send(())
        .await
        .expect("避免旧实现在第二次执行时挂起");
    let delegate: Arc<dyn AgentCardAdapter> = Arc::new(FileEffect {
        path: effect.clone(),
        wrote,
        release: wait,
        fail_after_effect: true,
    });
    let durable = Arc::new(LiveDurableAdapter::new(
        delegate,
        directory.path().join("app.sqlite"),
        "isolated-project".into(),
        "stable-intent".into(),
        "owner".into(),
    ));
    let runtime = agent::NormaToolRuntime::new(durable);
    let policy = agent::ToolCallPolicy::default();
    let mut request = context();
    let first = policy
        .execute(&runtime, "create_terminal", serde_json::json!({}), &request)
        .await;
    request.tool_call_id = "second-call".into();
    request.operation_id = Some(format!("{}:step-2", request.turn_id));
    let second = policy
        .execute(&runtime, "create_terminal", serde_json::json!({}), &request)
        .await;
    assert!(
        first.is_err() && second.is_err(),
        "unknown及后续工具都应阻止模型宣称成功"
    );
    assert!(
        policy.unknown_seen(),
        "部分副作用后的失败必须触发unknown闸门"
    );
    assert_eq!(
        std::fs::read_to_string(effect).expect("实际副作用文件"),
        "created-node-42\n",
        "新step不能绕过unknown重复执行"
    );
}
