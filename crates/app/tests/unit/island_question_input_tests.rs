use super::*;
use crate::app::OverviewQuestion;
use gpui::{div, prelude::*, AppContext, Entity, Focusable, Render, TestAppContext};
use norma::protocol::{
    AgentProfile, PendingQuestion, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};

struct Inputs(Entity<super::super::controller::IslandController>);
impl Render for Inputs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = &self
            .0
            .read(cx)
            .editing
            .values()
            .next()
            .expect("mounted editor")
            .editor;
        div().flex().flex_col().children(
            (0..editor.item_count()).map(|i| ui::component::input::Input::new(editor.input(i))),
        )
    }
}

fn row() -> OverviewQuestion {
    OverviewQuestion {
        area_id: "a".into(),
        agent: "Iris".into(),
        available: true,
        delegated: false,
        can_delegate: false,
        suggestion: None,
        snapshot: norma::protocol::QuestionSnapshot {
            question: PendingQuestion {
                target: QuestionRef {
                    workspace_id: "w".into(),
                    project_id: "p".into(),
                    node_id: 1,
                    provider: AgentProfile::Claude,
                    run_id: "r".into(),
                    connection_id: "c".into(),
                    session_id: "s".into(),
                    turn_id: None,
                    request_id: QuestionRequestId::String("q".into()),
                    revision: 1,
                },
                items: (0..2)
                    .map(|i| QuestionItem {
                        key: format!("q{i}"),
                        provider_id: None,
                        prompt: "Choose".into(),
                        options: vec![QuestionOption {
                            key: "blue".into(),
                            label: "Blue".into(),
                            description: None,
                        }],
                        multiple: true,
                        allow_custom: true,
                        secret: false,
                    })
                    .collect(),
            },
            state: norma::protocol::QuestionState::Pending,
            delivery_id: None,
            error: None,
            evidence: None,
        },
    }
}

/// 场景：同题两个自由输入框逆序切换焦点，同时多选自由文本可输入后清空。
/// 数据：两道多选题，各有Blue选项和自由回复；真实Root/Input与生产EditingQuestion订阅。
/// 执行：渲染后先聚焦第二框再第一框；填写第二题文字并清空，再检查实际draft。
/// 预期：迟到Blur不清新Focus；清空自定义文本后保留Blue且答案仍有效。
#[test]
fn island_question_inputs_keep_new_focus_and_clear_empty_custom_answer() {
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut owner = None;
    let row = row();
    let id = alert_key(999, "w", "a", &row.snapshot.question.target);
    let (_, cx) = test.add_window_view(|window, cx| {
        let model = super::super::model(cx);
        let controller = cx.new(|cx| super::super::controller::IslandController::new(model, cx));
        controller.update(cx, |state, cx| {
            let mut editor = QuestionEditor::new(&row, window, cx);
            editor.choose(0, "blue");
            editor.choose(1, "blue");
            state.editing.insert(
                id.clone(),
                EditingQuestion::mount(
                    IslandTarget {
                        source: 999,
                        workspace: "w".into(),
                        area: "a".into(),
                    },
                    editor,
                    11,
                    window,
                    cx,
                ),
            );
            state.event(11, IslandEvent::Expand, cx);
        });
        owner = Some(controller.clone());
        let view = cx.new(|cx| {
            cx.observe(&controller, |_, _, cx| cx.notify()).detach();
            Inputs(controller)
        });
        ui::component::Root::new(view, window, cx)
    });
    let owner = owner.expect("输入宿主存在");
    let inputs = owner.read_with(cx, |s, _| {
        [
            s.editing[&id].editor.input(0).clone(),
            s.editing[&id].editor.input(1).clone(),
        ]
    });
    // GPUI dispatches Focus/Blur only for an active window, including TestPlatform.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    for input in [&inputs[1], &inputs[0]] {
        cx.update(|window, cx| {
            window.focus(&input.read(cx).focus_handle(cx));
            window.draw(cx).clear();
        });
        cx.run_until_parked();
    }
    owner.read_with(cx, |state, _| {
        assert_eq!(
            state.focused_question.as_ref().map(|(_, input)| *input),
            Some(inputs[0].entity_id()),
            "后到旧Blur不能清掉第一输入的新焦点"
        );
    });
    cx.update(|window, cx| inputs[1].update(cx, |input, cx| input.set_value("extra", window, cx)));
    cx.run_until_parked();
    owner.read_with(cx, |state, cx| {
        assert_eq!(
            state.editing[&id].editor.answers(cx)[1].text.as_deref(),
            Some("extra"),
            "实际Change须选择自定义文本"
        );
    });
    cx.update(|window, cx| inputs[1].update(cx, |input, cx| input.set_value("  ", window, cx)));
    cx.run_until_parked();
    owner.read_with(cx, |state, cx| {
        let answers = state.editing[&id].editor.answers(cx);
        assert_eq!(answers[1].selected, ["blue"], "清自由文字不能丢多选项");
        assert!(answers[1].text.is_none(), "空白自由回复应恢复纯选项答案");
        assert!(
            row.snapshot.question.validate_answers(&answers).is_ok(),
            "纯选项回复仍能发送"
        );
    });
}

/// 场景：系统恢复岛窗口及旧Input焦点，不代表用户开始手动回答。
/// 数据：真实原生问题及已授权的幕僚委托，生产编辑订阅与真实Input控件。
/// 执行：恢复输入焦点、切走窗口、再激活窗口，不点击输入也不键入文字。
/// 预期：原题仍委托给幕僚；只有真实编辑意图才能撤销，审批点击不会触发输入副作用。
#[test]
fn island_question_restored_focus_does_not_revoke_delegation() {
    use crate::app::MultisoulApp;
    let dir = tempfile::tempdir().expect("隔离问题数据");
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut source = None;
    let mut input = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths.clone(),
            )
        });
        let row = app.update(cx, |app, cx| {
            let raw = serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
            ))
            .expect("原生问题");
            let (target, _, _) =
                crate::test_brain_question::make_question_from_raw(app, cx, dir.path(), raw);
            app.authorize_question_delegation(&target)
                .expect("允许幕僚回答");
            app.question_overview_rows("a").remove(0)
        });
        assert!(row.delegated, "数据必须确实包含生效委托");
        let host = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths.clone(),
            )
        });
        host.update(cx, |host, _| {
            host.desktop_workspace
                .sessions
                .insert(("w".into(), "a".into()), app.clone());
        });
        super::super::actions::register_source(&host, cx);
        let model = cx.new(|_| super::super::IslandModel::default());
        let controller = cx.new(|cx| super::super::controller::IslandController::new(model, cx));
        controller.update(cx, |state, cx| {
            let route = IslandTarget {
                source: host.entity_id().as_u64(),
                workspace: "w".into(),
                area: "a".into(),
            };
            let editor = QuestionEditor::new(&row, window, cx);
            input = Some(editor.input(0).clone());
            state.editing.insert(
                alert_key(route.source, "w", "a", editor.target()),
                EditingQuestion::mount(route, editor, 11, window, cx),
            );
            state.event(11, IslandEvent::Expand, cx);
        });
        source = Some((app, host));
        let view = cx.new(|_| Inputs(controller));
        ui::component::Root::new(view, window, cx)
    });
    let (source, _host) = source.expect("真实路由宿主");
    let input = input.expect("已挂载Input");
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.focus(&input.read(cx).focus_handle(cx));
        window.draw(cx).clear();
    });
    cx.deactivate_window();
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    source.read_with(cx, |app, _| {
        assert!(
            app.question_overview_rows("a")[0].delegated,
            "系统Focus恢复不能撤销原题的委托"
        );
    });
}

/// 场景：展开面板确认题目后，投影尚未移除原题，下一帧执行输入挂载。
/// 数据：真实QuestionEditor及旧问题快照，第二个不同revision的问题仍待处理。
/// 执行：初次挂载两个编辑器；确认第一题，保留原模型并重新挂载。
/// 预期：第一题不复活、第二题仍可编辑；渲染数据与徽标同步过滤。
#[test]
fn island_confirmed_question_cannot_remount_from_stale_projection() {
    use super::super::{
        controller::{counts, IslandController},
        IslandModel, IslandProject, IslandWorkspace,
    };
    struct MountHost(Entity<IslandController>);
    impl Render for MountHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    let row = row();
    let id = alert_key(1, "w", "a", &row.snapshot.question.target);
    let mut next = row.clone();
    next.snapshot.question.target.revision += 1;
    let next_id = alert_key(1, "w", "a", &next.snapshot.question.target);
    let overview = norma::mobile::RunOverview {
        running: vec![],
        idle: vec![],
        attention: vec![],
        unloaded_projects: vec![],
    };
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let (view, cx) = test.add_window_view(|_, cx| {
        let model = cx.new(|_| {
            let mut model = IslandModel::default();
            model.workspaces = vec![IslandWorkspace {
                database: "db.sqlite".into(),
                id: "w".into(),
                name: "Workspace".into(),
                projects: vec![IslandProject {
                    source: 1,
                    area_id: "a".into(),
                    name: "Project".into(),
                    root: "/project".into(),
                    loaded: true,
                    cached_overview: overview.clone(),
                    overview,
                    approvals: vec![],
                    questions: vec![row, next],
                }],
            }];
            model
        });
        let controller = cx.new(|cx| IslandController::new(model, cx));
        controller.update(cx, |state, cx| state.event(11, IslandEvent::Expand, cx));
        MountHost(controller)
    });
    view.update_in(cx, |view, window, cx| {
        view.0.update(cx, |state, cx| {
            state.mount_questions(11, window, cx);
            assert_eq!(state.editing.len(), 2, "确认前真实挂载两题");
            state.editing.remove(&id);
            state.action_feedback(id.clone(), true, cx);
            state.mount_questions(11, window, cx);
            assert!(
                !state.editing.contains_key(&id),
                "迟到快照不能重建已答题编辑器"
            );
            assert!(
                state.editing.contains_key(&next_id),
                "其他未答题仍保持可编辑"
            );
            let visible = state.visible_workspaces(cx);
            assert_eq!(
                visible[0].projects[0].questions.len(),
                1,
                "渲染不能包含旧题"
            );
            assert_eq!(counts(&visible), (0, 1), "徽标与可见题目一致");
            assert_eq!(
                state.model.read(cx).workspaces[0].projects[0]
                    .questions
                    .len(),
                2,
                "模型仍确实保留迟到旧快照"
            );
        });
    });
}
