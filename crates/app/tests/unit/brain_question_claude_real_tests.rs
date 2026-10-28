use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext};
use runtime::{NodeInput, NodeView, Runtime};

fn screen(app: &MultisoulApp, node: u64) -> String {
    let Some(NodeView::Term { snapshot, .. }) = app.runtime.node_view(node) else {
        return String::new();
    };
    snapshot
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn tick(source: &Entity<MultisoulApp>, cx: &mut VisualTestContext) {
    cx.run_until_parked();
    source.update(cx, |app, cx| {
        app.runtime.drain_all();
        app.agent_hook_server
            .drain_into_runtime(&mut app.runtime, Some("native-claude"));
        app.flush_question_receipts(cx);
    });
    std::thread::sleep(Duration::from_millis(20));
}

/// 场景：真实Claude默认原TUI经生产Hook HTTP入口、App授权及持久回执完成原题。
/// 构造：隔离App数据库、当前已信任仓库、新建默认Runtime与真实CLI；无PTY工厂/模拟Hook。
/// 执行：等原SessionStart和空prompt，要求一次AskUserQuestion，通过采用入口提交Green并对账。
/// 预期：原session/call被消费、原模型输出NATIVE_REPLY、SQLite Answered；无新派发且回收自有PTY。
/// 此测试没有真实Norma窗口，只作为生产传输/模型纵向证据，不替代desktop-e2e。
#[test]
#[ignore = "requires configured Claude 2.1.296 and freshly built sibling multisoul-agent-hook"]
fn question_default_native_claude_consumes_authorized_original_answer() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let node = source.update(cx, |app, _| {
        app.runtime = Runtime::new_for_workspace(&cwd);
        let mut config = app
            .agent_hook_server
            .runtime_config()
            .expect("真实HTTP Hook入口");
        config.workspace_id = "native-claude-workspace".into();
        config.workspace_route_id = "native-claude-route".into();
        config.norma_socket = dir
            .path()
            .join("unused.sock")
            .to_string_lossy()
            .into_owned();
        app.runtime.set_agent_hook_config(Some(config));
        app.shared_context_scope = Some(ContextScope {
            workspace_id: "native-claude-workspace".into(),
            area_id: "native-claude-area".into(),
            project: ContextProject::Project {
                project_id: "native-claude".into(),
                worktree: cwd.to_string_lossy().into_owned(),
            },
            task_id: None,
            conversation_id: None,
        });
        app.runtime
            .spawn_agent(
                canvas::AgentProfile::Claude,
                canvas::CanvasRect::new(0., 0., 900., 600.),
                135,
                45,
            )
            .unwrap()
    });
    // Always stop this test's original PTY, including failed assertions. Never touch user sessions.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            tick(&source, cx);
            if source.read_with(cx, |app, _| {
                app.runtime.agent_session_id(node).is_some() && screen(app, node).contains('❯')
            }) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "默认Claude必须由原Hook报告会话并进入prompt：{}",
                source.read_with(cx, |app, _| screen(app, node))
            );
        }
        source.update(cx, |app, _| {
            app.runtime.send_agent_task_text(node,"This is a bounded native question integration test. Do not read files, run commands or change anything. Call AskUserQuestion exactly once to ask Which label should the native test use? Offer Blue and Green. Wait for my answer, then reply exactly NATIVE_REPLY: followed by the chosen label.").unwrap();
        });
        let paste_settle = Instant::now() + Duration::from_millis(250);
        while Instant::now() < paste_settle {
            tick(&source, cx);
        }
        source.update(cx, |app, _| {
            app.runtime
                .send_node_input(
                    node,
                    NodeInput::Key(terminal::KeyInput {
                        key: terminal::Key::Enter,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    }),
                )
                .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(100);
        let question = loop {
            tick(&source, cx);
            if let Some(question) = source.read_with(cx, |app, _| {
                app.runtime
                    .questions()
                    .into_iter()
                    .find(|snapshot| snapshot.state == QuestionState::Pending)
            }) {
                break question.question;
            }
            assert!(
                Instant::now() < deadline,
                "生产App必须接到真实原题：{}",
                source.read_with(cx, |app, _| screen(app, node))
            );
        };
        assert_eq!(question.items.len(), 1, "模型只生成一道原题");
        let item = &question.items[0];
        let green = item
            .options
            .iter()
            .find(|option| option.label == "Green")
            .expect("真实Green选项");
        let answers = vec![QuestionAnswer {
            key: item.key.clone(),
            selected: vec![green.key.clone()],
            text: None,
        }];
        source.update(cx, |app, cx| {
            let permit = app
                .authorize_question_answer(&question.target, answers)
                .unwrap();
            app.begin_authorized_answer(permit, cx).unwrap().detach();
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            tick(&source, cx);
            let done = source.read_with(cx, |app, _| {
                app.runtime
                    .question(&question.target)
                    .is_some_and(|snapshot| snapshot.state == QuestionState::Answered)
                    && screen(app, node).contains("NATIVE_REPLY: Green")
                    && !app.brain.questions.receipt_write_pending
                    && app.brain.questions.ready_receipts.is_empty()
            });
            if done {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "原题必须消费并得到真实模型确认：{}",
                source.read_with(cx, |app, _| screen(app, node))
            );
        }
        let receipt = QuestionDeliveryStore::new(database)
            .get(&serde_json::to_value(&question.target).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            receipt.state,
            QuestionDeliveryState::Answered,
            "真实原call消费保存为Answered"
        );
        source.read_with(cx, |app, _| {
            assert!(
                app.agent_task_coordinator
                    .as_ref()
                    .is_none_or(|coordinator| coordinator.tasks().is_empty()),
                "采用答案没有派发新任务"
            );
        });
        println!(
            "NATIVE_CLAUDE_CONSUMED target={} delivery={}",
            serde_json::to_string(&question.target).unwrap(),
            receipt.delivery_id
        );
    }));
    source.update(cx, |app, _| app.runtime.kill_all());
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
