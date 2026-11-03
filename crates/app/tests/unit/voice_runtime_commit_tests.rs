use super::*;
use crate::test_gpt_live_sse as sse;
use agent::{AgentRosterEntry, AgentTaskState, AgentTrackingMode};
use norma::protocol::AgentProfile;
use serde_json::json;
use std::time::Duration;

/// 场景：真实 ProjectAgentRuntime 连续收到正文和目标补答。
/// 构造：隔离数据库、真实 worker、回环模型；提交通道记录工具与本轮原话。
/// 执行：首轮产生澄清，再发送 Henry；整个过程走 UserTurn 的取代和 generation 逻辑。
/// 预期：已结束回合被新输入取代不能取消草稿；只提交一次原正文，补答身份独立。
#[test]
fn runtime_target_clarification_survives_completed_turn_replacement() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(directory.path());
    storage::AppStore::initialize(&paths).expect("隔离数据库");
    shared_tokio_runtime().expect("运行时").block_on(async {
        let (endpoint,server)=sse::serve(vec![json!({"role":"assistant","tool_calls":[{"index":0,"id":"guess","type":"function","function":{"name":"assign_agent_task","arguments":json!({"agent":"Bruno","task":"检查登录报错"}).to_string()}}]}),json!({"role":"assistant","content":"应该发给谁？"})]).await;
        let config=agent::ProviderConfig::new(agent::ProviderProtocol::OpenAiCompatible,endpoint,"fixture",Some("fixture".into()));
        let (runtime,adapter,_events)=ProjectAgentRuntime::spawn_scoped("fixture".into(),config,paths,Some(directory.path().to_path_buf()),false,None).expect("真实 worker");
        let roster=vec![(1,"Bruno",11),(2,"Henry",22)].into_iter().map(|(definition_id,name,node)| AgentRosterEntry {definition_id,name:name.into(),speech_alias:None,profile:AgentProfile::Codex,enabled:true,active_node_id:Some(node),tracking:AgentTrackingMode::Managed,busy:false,task_state:AgentTaskState::Ready}).collect();
        runtime.submit(AgentCommand::UpdateAgentRoster(roster)).recv().await.expect("目录确认").expect("目录更新");
        runtime.submit(AgentCommand::UpdateCurrentCanvasContext(agent::CurrentCanvasContext {project_name:"fixture".into(),workspace_root:"fixture".into(),total_cards:2,cards:vec![],conversation_tails:vec![]})).recv().await.expect("画布确认").expect("画布更新");
        let first=runtime.submit(AgentCommand::UserTurn {input:"检查登录报错".into(),input_source:AgentInputSource::SingleVoiceCommand {generation:1}}).recv().await.expect("首轮回执").expect("首轮应补问");
        assert!(first.iter().any(|event| matches!(event,AgentEvent::DialogueTextDelta {..})),"首轮应返回用户可见的补问");
        assert!(adapter.is_empty(),"目标不明时不能提交");
        assert_eq!(server.await.expect("模型结束").len(),2,"仅原规划回合调用模型");
        let host=tokio::spawn(async move {
            let request=tokio::time::timeout(Duration::from_secs(2),adapter.recv()).await.ok()?.ok()?;
            let observed=(request.command.clone(),request.context.user_input.clone());
            let _=request.result_tx.send(Ok(AdapterResult::accepted("fixture-received")));
            Some(observed)
        });
        let second=runtime.submit(AgentCommand::UserTurn {input:"Henry".into(),input_source:AgentInputSource::SingleVoiceCommand {generation:2}}).recv().await.expect("补答回执").expect("补答不应报错");
        assert!(second.iter().any(|event| matches!(event,AgentEvent::DialogueTextDelta {text} if text == "fixture-received")),"真实 worker 必须恢复草稿并提交，而不是误判 ScopeChanged：{second:?}");
        assert_eq!(host.await.expect("宿主结束"),Some((AdapterCommand::AssignAgentTask {agent:"Henry".into(),task:"检查登录报错".into()},"Henry".into())),"正文与补答来源须分别保留，且只写一次");
    });
}

/// 场景：两张同名卡片已明确选择第二张，宿主必须使用提交信封的稳定节点。
/// 构造：真实 App 与两张同定义 Codex 卡片，PTY 全部使用隔离 echo 后端。
/// 执行：同一生产前台执行器分别回复和新建任务，并在每次同步提交结束后检查绑定释放。
/// 预期：两个回执都只指向第二张卡片，不能回落到第一个同名节点。
#[test]
fn explicit_voice_card_binding_reaches_the_actual_reply_and_task_executors() {
    use gpui::TestAppContext;
    let directory = tempfile::tempdir().expect("隔离项目");
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(cx, |app, cx| {
        app.enter_canvas_for_test(directory.path(), cx)
    });
    super::prepare_shared_context(&host, cx);
    let (definition_id, first, second) = cx.update(|window, app_cx| {
        host.update(app_cx, |app, _| {
            let result = commands::execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Bruno".into()),
                    profile: norma::protocol::AgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("创建成员");
            let CardCommandResult::AgentCreated {
                definition_id,
                node_id: Some(first),
                ..
            } = result
            else {
                panic!("应创建真实卡片");
            };
            let definition = app
                .agent_definitions
                .definitions()
                .iter()
                .find(|d| d.id == definition_id)
                .expect("定义")
                .clone();
            let bounds = app
                .runtime
                .document
                .nodes()
                .iter()
                .find(|n| n.id == first)
                .expect("第一节点")
                .bounds;
            // 正常定义 API 会复用卡片；匿名节点恢复同一 definition_id 模拟历史重复实例。
            let second = app
                .runtime
                .spawn_agent_with_alias(
                    definition.runtime_profile,
                    Some("Bruno".into()),
                    bounds,
                    80,
                    24,
                )
                .expect("第二实例");
            if let canvas::CanvasNodeKind::Agent(spec) = &mut app
                .runtime
                .document
                .node_mut(second)
                .expect("第二节点")
                .kind
            {
                spec.definition_id = Some(definition_id);
            }
            assert_ne!(
                first, second,
                "夹具必须真有两个节点，不能误用会复用卡片的 API"
            );
            (definition_id, first, second)
        })
    });
    let binding = agent::voice_command::VoiceTargetBinding {
        definition_id,
        node_id: Some(second),
        name: "Bruno".into(),
    };
    for command in [
        AdapterCommand::ReplyAgent {
            agent: "Bruno".into(),
            message: "继续方案".into(),
        },
        AdapterCommand::AssignAgentTask {
            agent: "Bruno".into(),
            task: "检查登录报错".into(),
        },
    ] {
        let receipt = cx
            .update(|window, app_cx| {
                host.update(app_cx, |app, cx| {
                    voice_command::with_target(app, Some(&binding), |app| {
                        execute_norma_adapter_command_with_source(
                            app,
                            Some(window),
                            cx,
                            command,
                            AgentInputSource::SingleVoiceCommand { generation: 1 },
                        )
                    })
                })
            })
            .expect("绑定执行");
        assert_eq!(
            receipt.node_id,
            Some(second),
            "真实副作用回执必须指向用户选定的第二节点"
        );
        assert_ne!(receipt.node_id, Some(first), "不能因为同名而使用第一个节点");
        assert_eq!(
            host.read_with(cx, |app, _| app.voice_bound_node(definition_id)),
            None,
            "一次提交后必须释放临时绑定，不影响其他来源"
        );
    }
    host.update(cx, |app, _| {
        app.advance_published_agent_task(
            first,
            runtime::AgentTaskState::Done,
            norma::protocol::AgentTaskResult {
                availability: norma::protocol::AgentTaskResultAvailability::Unavailable,
                text: None,
                truncated: false,
                background_unsettled: false,
            },
        );
        assert!(
            app.agent_task_coordinator
                .as_ref()
                .expect("任务协调器")
                .tasks()
                .iter()
                .any(|task| task.target_node_id == second && task.state == "running"),
            "其他同名节点的完成事件不能结算选定节点的任务"
        );
        let mut roster = app.agent_roster_for_provider();
        for member in &mut roster {
            if member.definition_id == definition_id {
                member.active_node_id = Some(first);
                member.tracking = AgentTrackingMode::Managed;
            }
        }
        let coordinator = app.agent_task_coordinator.as_mut().expect("协调器");
        coordinator.sync_roster(roster).expect("另一个实例");
        let queued = coordinator
            .dispatch(
                vec![agent::AgentAssignment::new("Bruno", "另一个节点的排队任务")],
                false,
            )
            .expect("跨实例排队夹具");
        assert_eq!(
            queued[0].state,
            agent::AgentDispatchState::Queued,
            "必须实际形成等待中的另一节点任务"
        );
        app.advance_published_agent_task(
            second,
            runtime::AgentTaskState::Ended,
            norma::protocol::AgentTaskResult {
                availability: norma::protocol::AgentTaskResultAvailability::Unavailable,
                text: None,
                truncated: false,
                background_unsettled: false,
            },
        );
        assert!(
            app.agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .tasks()
                .iter()
                .any(|task| task.target_node_id == first && task.state == "running"),
            "关闭当前实例不能中断另一实例已经绑定的排队任务"
        );
    });
}

/// 场景：旧单次提交正在等待宿主回执，新语音回合已接替。
/// 构造：真实 worker 与两个快速路径请求；第一条回执故意延迟，第二条正常接收。
/// 执行：用目录更新确认 actor 已处理新回合，再释放旧回执并读取事件。
/// 预期：旧回合返回取代错误；事件队列只保留当前回合文本和一个完成事件。
#[test]
fn superseded_voice_commit_cannot_finish_the_new_reply_stream() {
    let directory = tempfile::tempdir().expect("隔离项目");
    let paths = storage::DatabasePaths::isolated_test(directory.path());
    storage::AppStore::initialize(&paths).expect("数据库");
    shared_tokio_runtime().expect("运行时").block_on(async {
        let config = agent::ProviderConfig::new(
            agent::ProviderProtocol::OpenAiCompatible,
            "http://127.0.0.1:1/v1",
            "fixture",
            Some("fixture".into()),
        );
        let (runtime, adapter, events) = ProjectAgentRuntime::spawn_scoped(
            "fixture".into(),
            config,
            paths,
            Some(directory.path().to_path_buf()),
            false,
            None,
        )
        .expect("worker");
        let roster: Vec<_> = vec![(1, "Bruno", 11), (2, "Henry", 22)]
            .into_iter()
            .map(|(definition_id, name, node)| AgentRosterEntry {
                definition_id,
                name: name.into(),
                speech_alias: None,
                profile: AgentProfile::Codex,
                enabled: true,
                active_node_id: Some(node),
                tracking: AgentTrackingMode::Managed,
                busy: false,
                task_state: AgentTaskState::Ready,
            })
            .collect();
        runtime
            .submit(AgentCommand::UpdateAgentRoster(roster.clone()))
            .recv()
            .await
            .expect("目录回执")
            .expect("更新");
        runtime
            .submit(AgentCommand::UpdateCurrentCanvasContext(
                agent::CurrentCanvasContext {
                    project_name: "fixture".into(),
                    workspace_root: "fixture".into(),
                    total_cards: 2,
                    cards: vec![],
                    conversation_tails: vec![],
                },
            ))
            .recv()
            .await
            .expect("画布回执")
            .expect("更新");
        let old = runtime.submit(AgentCommand::UserTurn {
            input: "Bruno，新建任务，检查登录".into(),
            input_source: AgentInputSource::SingleVoiceCommand { generation: 1 },
        });
        let old_request = tokio::time::timeout(Duration::from_secs(2), adapter.recv())
            .await
            .expect("第一条到宿主")
            .expect("请求");
        let current = runtime.submit(AgentCommand::UserTurn {
            input: "Henry，新建任务，检查登录".into(),
            input_source: AgentInputSource::SingleVoiceCommand { generation: 2 },
        });
        runtime
            .submit(AgentCommand::UpdateAgentRoster(roster))
            .recv()
            .await
            .expect("actor 确认")
            .expect("更新");
        let _ = old_request.result_tx.send(Err(AdapterError::Cancelled));
        assert!(
            tokio::time::timeout(Duration::from_secs(2), old.recv())
                .await
                .expect("旧回合及时退出")
                .expect("旧结果")
                .is_err(),
            "取代后的旧提交不能作为正常完成返回"
        );
        let request = tokio::time::timeout(Duration::from_secs(2), adapter.recv())
            .await
            .expect("当前请求不受阻")
            .expect("请求");
        let _ = request
            .result_tx
            .send(Ok(AdapterResult::accepted("current-received")));
        current
            .recv()
            .await
            .expect("当前回执")
            .expect("当前回合成功");
        let mut text = Vec::new();
        let mut finished = 0;
        while let Ok(event) = events.try_recv() {
            if !event.is_current(None) {
                continue;
            }
            match event.event {
                AgentEvent::DialogueTextDelta { text: value } => text.push(value),
                AgentEvent::DialogueStreamFinished => finished += 1,
                _ => {}
            }
        }
        assert_eq!(text, vec!["current-received"], "旧反馈不能串入新回复");
        assert_eq!(
            finished, 1,
            "只能有当前回合的完成事件，旧回合不能提前结束新回复"
        );
    });
}
