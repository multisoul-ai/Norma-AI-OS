use super::*;
use crate::app::background_project::PreparedProject;
use crate::settings::voice::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState};
use agent::{MemoryRevision, MemoryScopeAlias};
use gpui::{AppContext as _, TestAppContext, VisualTestContext};
use std::time::{Duration, Instant};
#[path = "../support/gpt_live_sse.rs"]
mod sse;

/// 场景：手机读取的项目或工作树记忆被设置页纠正后，下一轮不能再发送旧工具结果和回答。
/// 构造：隔离数据库、真实工作空间与移动授权、生产消息入口和四次本地 SSE 请求。
/// 执行：模型调用 workspace_memory 读取正文，稳定版本再聊一轮，外部发布纠正后再次提问。
/// 预期：初次确实读取项目正文；稳定版本保留历史；修改后旧正文和旧回答均不再发送。
#[test]
fn workspace_memory_revision_invalidates_project_facts_in_http_requests() {
    exercise_workspace_memory(false);
}

/// 场景：模型历史已清理后，持久化群聊里的旧回答不能借 recent_shared_facts 再次进入请求。
/// 构造：沿用真实授权与 HTTP 夹具，逐轮写入 WorkspaceChatStore 并读取生产 recent_context。
/// 执行：先读项目记忆并保存回答，再纠正记忆，第三轮注入持久化记录。
/// 预期：旧模型正文不自动成为共享事实，原始用户消息仍保留，界面账本不被删除。
#[test]
fn workspace_memory_revision_cannot_restore_facts_from_persisted_chat() {
    exercise_workspace_memory(true);
}

fn exercise_workspace_memory(persist_history: bool) {
    for scope in [MemoryScopeAlias::Project, MemoryScopeAlias::Worktree] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
        let database = paths.app_db_path().to_owned();
        let project = dir.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let store = storage::AppStore::initialize(&paths).unwrap();
        let catalog = store.desktop_workspace_store();
        let space = catalog.create("Memory").unwrap();
        let area = catalog.add_project(&space.id, &project).unwrap();
        let project_id = store
            .workspace_identity_store()
            .resolve_or_create(&area.root, "Project")
            .unwrap()
            .workspace_id;
        let token = "a".repeat(64);
        store
            .mobile_access_store()
            .register_all_projects(
                "phone",
                "iPhone",
                &token,
                storage::MobilePermission::Control,
            )
            .unwrap();
        let mut memory = storage::MemoryAccess::new(paths.clone(), Some(project));
        let saved = memory
            .settings_save(
                scope,
                MemoryRevision::INITIAL,
                vec![("preference".into(), "OLD-PROJECT-BODY".into())],
                "种子",
                "seed",
            )
            .unwrap();
        let args = json!({"area_id":area.id,"action":"read","scope":scope,"id":"preference"});
        let tokio = shared_tokio_runtime().unwrap();
        let (endpoint, server) = tokio.block_on(sse::serve(vec![
            json!({"role":"assistant","tool_calls":[{"index":0,"id":"read-project","type":"function",
                "function":{"name":"workspace_memory","arguments":args.to_string()}}]}),
            json!({"role":"assistant","content":"OLD-PROJECT-ANSWER"}),
            json!({"role":"assistant","content":"OLD-PROJECT-ANSWER"}),
            json!({"role":"assistant","content":"CURRENT-PROJECT-ANSWER"}),
        ]));
        let prepared = PreparedProject::load_for_desktop(paths.clone(), area.clone()).unwrap();
        let mut test = TestAppContext::single();
        let (host, cx) =
            test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let session = cx.new(|cx| {
            let mut app = MultisoulApp::from_background_project(prepared, cx);
            app.desktop_host = Some(host.downgrade());
            app
        });
        host.update_in(cx, |app, window, cx| {
            let mut saved = SavedVoiceSettings::default();
            saved.openai_profile.base_url = Some(endpoint);
            saved.openai_profile.model = Some("fixture".into());
            saved.openai_profile.api_key = Some("fixture".into());
            app.voice_settings =
                VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());
            app.desktop_workspace.active_id = Some(space.id.clone());
            app.desktop_workspace.areas = vec![area.clone()];
            app.desktop_workspace
                .sessions
                .insert((space.id.clone(), area.id.clone()), session);
            crate::workspace::desktop::sessions::register(
                database.clone(),
                window.window_handle(),
                cx.entity(),
                cx,
            );
        });
        let areas = vec![WorkspaceArea { area, project_id }];
        let chat = storage::WorkspaceChatStore::new(database.clone());
        for turn in 0..3 {
            if turn == 2 {
                memory
                    .settings_save(
                        scope,
                        saved.revision,
                        vec![("preference".into(), "NEW-PROJECT-BODY".into())],
                        "纠正",
                        "correct",
                    )
                    .unwrap();
            }
            let text = if turn == 0 {
                "HISTORICAL-USER-REQUEST 查询当前项目偏好"
            } else {
                "查询当前项目偏好"
            };
            let operation = format!("op-{turn}");
            let history = if persist_history {
                chat.receive(
                    "phone",
                    "mac",
                    &space.id,
                    &operation,
                    &json!({"sender":"user","text":text}).to_string(),
                )
                .unwrap();
                assert!(chat.claim("phone", &operation).unwrap(), "每轮只能接受一次");
                chat.recent_context("phone", &operation).unwrap()
            } else {
                json!([])
            };
            let receiver = cx
                .update(|_, cx| {
                    submit_workspace_turn(
                        database.clone(),
                        areas.clone(),
                        "phone".into(),
                        token.clone(),
                        WorkspaceChatMessage {
                            desktop_workspace_id: space.id.clone(),
                            operation_id: format!("op-{turn}"),
                            message_id: format!("message-{turn}"),
                            text: text.into(),
                            intent: WorkspaceMessageIntent::Discuss,
                            target: None,
                            idea_id: None,
                        },
                        history,
                        cx,
                    )
                })
                .unwrap();
            let response = receive(&receiver, cx).unwrap();
            assert_eq!(response["state"], "replied", "真实工作空间入口应完成对话");
            if persist_history {
                if turn == 0 {
                    chat.record_event(
                        "phone",
                        &operation,
                        "task_result",
                        &json!({
                            "sender":"Norma", "text":"OLD-PROJECT-ANSWER 的任务总结",
                            "task_ref":"receipt-17", "state":"succeeded"
                        })
                        .to_string(),
                    )
                    .unwrap();
                }
                chat.finish("phone", &operation, "succeeded", &response.to_string())
                    .unwrap();
            }
        }
        let requests = tokio.block_on(server).unwrap();
        assert_eq!(requests.len(), 4, "首轮工具续轮加两轮对话共四次 HTTP 请求");
        let messages = |index: usize| requests[index]["messages"].to_string();
        assert!(
            messages(1).contains("OLD-PROJECT-BODY"),
            "项目记忆必须真实经工具返回"
        );
        assert!(
            messages(2).contains("OLD-PROJECT-BODY"),
            "版本稳定时保留工具历史"
        );
        assert!(
            messages(2).contains("OLD-PROJECT-ANSWER"),
            "版本稳定时保留旧轮回答"
        );
        assert!(
            !messages(3).contains("OLD-PROJECT-BODY"),
            "项目修订后旧工具正文不得重入模型"
        );
        assert!(
            !messages(3).contains("OLD-PROJECT-ANSWER"),
            "项目修订后旧回答不得重入模型"
        );
        if persist_history {
            assert!(
                messages(3).contains("HISTORICAL-USER-REQUEST"),
                "原始用户消息仍保留来源"
            );
            assert!(
                messages(3).contains("receipt-17"),
                "结构化任务回执不得随模型正文一起丢弃"
            );
            let receipt = chat
                .receipt("phone", "mac", &space.id, "op-0")
                .unwrap()
                .unwrap();
            assert!(
                receipt
                    .result
                    .unwrap()
                    .to_string()
                    .contains("OLD-PROJECT-ANSWER"),
                "模型失效不能删除界面账本"
            );
        }
        cx.update(|_, cx| cx.default_global::<WorkspaceRuntimes>().0.clear());
    }
}

fn receive(
    receiver: &async_channel::Receiver<Result<Value, String>>,
    cx: &mut VisualTestContext,
) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        match receiver.try_recv() {
            Ok(value) => return value,
            Err(async_channel::TryRecvError::Closed) => panic!("工作空间回合提前关闭"),
            Err(async_channel::TryRecvError::Empty) => {}
        }
        assert!(Instant::now() < deadline, "工作空间回合必须及时完成");
        std::thread::sleep(Duration::from_millis(1));
    }
}
