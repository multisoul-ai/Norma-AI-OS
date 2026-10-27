use super::*;
use gpui::TestAppContext;
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// 场景：手机工作空间的模型会话独立于桌面项目实体，关闭召回也必须取消其在途回复。
/// 构造：同库和异库各一个真实 ProjectAgentRuntime，本地 HTTP 接收请求但暂不返回模型回复。
/// 执行：确认两个请求都已进入网络等待，再通过已订阅的 MemoryView 发出生产撤销事件。
/// 预期：同库请求明确取消，异库仍等待；没有实际工具副作用或真实云端访问。
#[test]
fn workspace_memory_recall_revokes_inflight_runtime_by_database() {
    revoke_inflight(false);
}

/// 场景：事件消费者停滞且普通请求队列已满时，隐私撤销不能丢失。
/// 构造：真实在途请求；填满 32 槽事件队列，阻塞下一次事件发送，再填满 16 槽请求。
/// 执行：发出生产召回撤销事件，保持队列拥塞，不消费事件来帮助取消。
/// 预期：同库请求仍及时取消，异库请求不受影响；旧排队请求不能重新启动。
#[test]
fn workspace_memory_recall_revokes_with_saturated_actor() {
    revoke_inflight(true);
}

fn revoke_inflight(saturate: bool) {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(root.path());
    let other_paths = storage::DatabasePaths::isolated_test(other.path());
    let tokio = shared_tokio_runtime().unwrap();
    let listener = tokio
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let config = agent::ProviderConfig::new(
        agent::ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let view = host.update_in(cx, |app, window, cx| {
        app.ensure_norma_memory(window, cx);
        app.norma_memory.clone().unwrap()
    });
    cx.run_until_parked();
    let receivers = [paths.clone(), other_paths].map(|paths| {
        let (runtime, _adapter, events) = ProjectAgentRuntime::spawn_scoped(
            "".into(),
            config.clone(),
            paths.clone(),
            None,
            true,
            None,
        )
        .unwrap();
        let response = runtime.submit(AgentCommand::UserTurn {
            input: "聊聊工作空间".into(),
            input_source: AgentInputSource::MobileDiscussion,
        });
        cx.update(|_, cx| {
            cx.default_global::<WorkspaceRuntimes>().0.insert(
                (paths.app_db_path().to_owned(), "workspace".into()),
                WorkspaceRuntime {
                    runtime,
                    scopes: Default::default(),
                },
            );
        });
        (response, events)
    });
    let sockets = tokio.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut sockets = Vec::new();
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut byte = [0];
                assert_eq!(
                    socket.read(&mut byte).await.unwrap(),
                    1,
                    "两个模型请求必须实际开始传输"
                );
                sockets.push(socket);
            }
            sockets
        })
        .await
        .expect("两个在途模型请求应及时到达")
    });
    if saturate {
        cx.update(|_, cx| {
            let runtime = &cx.global::<WorkspaceRuntimes>().0
                [&(paths.app_db_path().to_owned(), "workspace".into())]
                .runtime;
            for _ in 0..32 {
                let ack = runtime.submit(AgentCommand::CardActivity {
                    node_id: 1,
                    summary: "积压事件".into(),
                });
                tokio.block_on(async {
                    tokio::time::timeout(Duration::from_secs(2), ack.recv())
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap();
                });
            }
            let blocked = runtime.submit(AgentCommand::CardActivity {
                node_id: 1,
                summary: "等待事件槽".into(),
            });
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !runtime.request_tx.is_empty() {
                assert!(std::time::Instant::now() < deadline, "actor 应取走阻塞事件");
                std::thread::yield_now();
            }
            for _ in 0..16 {
                let _ = runtime.submit(AgentCommand::UserTurn {
                    input: "撤销前的排队消息".into(),
                    input_source: AgentInputSource::MobileDiscussion,
                });
            }
            assert!(runtime.request_tx.is_full(), "必须真实填满普通命令队列");
            assert!(blocked.try_recv().is_err(), "事件发送必须仍在阻塞 actor");
        });
    }
    view.update(cx, |_, cx| {
        cx.emit(crate::settings::norma::MemoryContextInvalidated)
    });
    cx.run_until_parked();
    let cancelled = tokio.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), receivers[0].0.recv()).await
    });
    assert!(
        matches!(cancelled, Ok(Ok(Err(_)))),
        "同库工作空间必须取消在途模型回复：{cancelled:?}"
    );
    assert!(
        matches!(
            receivers[1].0.try_recv(),
            Err(async_channel::TryRecvError::Empty)
        ),
        "异库请求应仍在等待，不能被误取消"
    );
    cx.update(|_, cx| {
        let runtimes = &cx.global::<WorkspaceRuntimes>().0;
        assert_eq!(runtimes.len(), 1, "只能移除同库旧运行时，保留异库运行时");
        assert!(
            runtimes.keys().all(|(db, _)| db != paths.app_db_path()),
            "撤销后不保留同库旧模型历史"
        );
        cx.default_global::<WorkspaceRuntimes>().0.clear();
    });
    drop(sockets);
}

/// 场景：session 锁不可用时，撤销无需等待锁释放即可结束已接收回合。
/// 构造：持有真实 NormaSession 锁；提交 generation 匹配的生产 spawn_turn。
/// 执行：保持锁不释放，取消运行时生命周期并读取响应。
/// 预期：两秒内明确关闭，不发送模型事件；释放锁也不能恢复该回合。
#[tokio::test]
async fn workspace_memory_recall_revokes_turn_waiting_for_session_lock() {
    let config = agent::ProviderConfig::new(
        agent::ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:1/v1",
        "fixture",
        Some("fixture".into()),
    );
    let (sender, _receiver) = async_channel::bounded(1);
    let session =
        agent::NormaSession::new("", &config, Arc::new(ChannelAgentCardAdapter { sender }))
            .unwrap();
    let session = Arc::new(tokio::sync::Mutex::new(session));
    let held = session.lock().await;
    let (events, event_rx) = async_channel::bounded(32);
    let runtime = TurnRuntime {
        session: session.clone(),
        events,
        dialogue_cancelled: Default::default(),
        generation: Arc::new(AtomicU64::new(1)),
        cancelled: CancellationToken::new(),
    };
    let (response, received) = async_channel::bounded(1);
    spawn_turn(
        &runtime,
        "旧输入".into(),
        AgentInputSource::MobileDiscussion,
        None,
        None,
        Vec::new(),
        response,
        1,
    );
    runtime.cancelled.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), received.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result,
        Err("Norma Session 已关闭".into()),
        "待锁回合也必须响应生命周期撤销"
    );
    drop(held);
    assert!(
        matches!(event_rx.try_recv(), Err(async_channel::TryRecvError::Empty)),
        "旧回合不能产生任何模型输出"
    );
}
