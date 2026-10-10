use super::test_support::{accept, with_cards};
use agent::card_instance::{CardInstanceAssignment, CardInstanceStage, CardInstanceTracking};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 实际派工只写冻结的第二张卡片，即使两卡共用配置或目标完全没有 definition。
/// 数据构造：2 张 OpenCode 卡片均无 definition；2 个独立 PTY 记录，目标为第 2 张；Enter 延时 100 ms。
/// 执行过程：通过生产 accept 接收第二张的任务，推进正文，再推进至 101 ms 发送 Enter。
/// 预期结果：第二张收到唯一正文和 CR(13)，第一张零输入；无 definition 也返回 Submitted 而非完成。
#[test]
fn instance_dispatch_writes_only_the_existing_target_without_a_definition() {
    let writes = Arc::new(Mutex::new(Vec::<Arc<Mutex<Vec<Vec<u8>>>>>::new()));
    let observed = writes.clone();
    let factory: runtime::PtyFactory = Box::new(move |_, _, _, tx| {
        let card = Arc::new(Mutex::new(Vec::new()));
        observed.lock().expect("应登记 PTY").push(card.clone());
        Ok(Box::new(terminal::TestPtyBackend::recording(tx, card)))
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("临时项目");
    let (app, cx) =
        gpui.add_window_view(|_, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let (first, second) = app.update(cx, |app, _| {
        let first = app
            .runtime
            .spawn_agent(
                AgentProfile::OpenCode,
                CanvasRect::new(0.0, 0.0, 640.0, 400.0),
                80,
                24,
            )
            .expect("首卡");
        let second = app
            .runtime
            .spawn_agent(
                AgentProfile::OpenCode,
                CanvasRect::new(700.0, 0.0, 640.0, 400.0),
                80,
                24,
            )
            .expect("目标卡");
        (first, second)
    });
    let now = Instant::now();
    let accepted = app
        .update(cx, |app, _| {
            let target = app
                .runtime
                .workspace_card_target("workspace", "area", 1, second)
                .expect("冻结第二张");
            app.accept_card_instance_assignment(
                "workspace",
                "area",
                1,
                CardInstanceAssignment {
                    assignment_id: "operation-assignment".into(),
                    target,
                    name_at_acceptance: "Betty".into(),
                    definition_id: None,
                    text: "第二张的独有正文".into(),
                    tracking: CardInstanceTracking::Untracked,
                    force_untracked: false,
                },
            )
        })
        .expect("现有无定义卡片应接收");
    assert_eq!(
        accepted.stage,
        CardInstanceStage::Accepted,
        "接收不能宣称 PTY 已提交"
    );
    app.update(cx, |app, _| {
        app.advance_card_instance_tasks_at(now);
        app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
    });
    let submitted = app
        .update(cx, |app, _| {
            app.card_instance_receipt("operation-assignment")
        })
        .expect("引用可查询");
    assert_eq!(submitted.target.card.node_id, second, "回执保留第二張地址");
    assert_ne!(
        submitted.target.card.node_id, first,
        "定义或卡片顺序不能改投首卡"
    );
    assert_eq!(
        submitted.stage,
        CardInstanceStage::Submitted,
        "只有 Enter 成功才 Submitted，无 Hook 不猜完成"
    );
    assert_eq!(submitted.bound_turn, None, "无 Provider 证据不能伪造 turn");
    let writes = writes.lock().expect("读取 PTY");
    assert!(
        writes[0].lock().expect("首卡记录").is_empty(),
        "非目标卡片零输入"
    );
    assert_eq!(
        *writes[1].lock().expect("目标记录"),
        vec!["第二张的独有正文".as_bytes().to_vec(), vec![13]],
        "目标只收正文和 CR(0x0d=13)"
    );
    assert_eq!(writes.len(), 2, "派工不得自动新建第三个 PTY");
}

/// 两卡共享 definition 时生产适配器仍只向选中的第二实例写入。
/// 数据构造：既有两个 PTY，两个 AgentSpec 都绑定同一个新 definition；仅派第二张 1 项任务。
/// 执行过程：绑定同配置，冻结第二卡，accept/advance 正文和 100 ms 后 Enter。
/// 预期结果：首卡 0 输入，第二卡正文+Enter；任务不新建卡片或由定义顺序重新选人。
#[test]
fn shared_definition_is_configuration_and_never_selects_the_first_card() {
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            let store = app
                .persistence
                .project_store()
                .expect("已打开项目")
                .agent_definition_store();
            let definition = store
                .create(&storage::NewAgentDefinition {
                    name: "Alan".into(),
                    runtime_profile: AgentProfile::OpenCode,
                    workflow_description: String::new(),
                })
                .expect("测试配置");
            app.agent_definitions.reload();
            for node in nodes {
                let canvas::CanvasNodeKind::Agent(spec) =
                    &mut app.runtime.document.node_mut(node).expect("既有卡片").kind
                else {
                    panic!("需 Agent 卡片")
                };
                spec.definition_id = Some(definition.id);
            }
            accept(
                app,
                nodes[1],
                "shared-definition",
                "第二实例正文",
                CardInstanceTracking::Untracked,
                Some(definition.id),
            );
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
        });
        assert!(
            writes[0].lock().expect("首卡").is_empty(),
            "共用 definition 不能投往第一卡"
        );
        assert_eq!(
            *writes[1].lock().expect("目标"),
            vec!["第二实例正文".as_bytes().to_vec(), vec![13]],
            "目标既有 PTY 才收到输入"
        );
        let target = app
            .update(cx, |app, _| app.card_instance_receipt("shared-definition"))
            .expect("引用");
        assert_eq!(target.target.card.node_id, nodes[1], "结果仍关联第二实例");
    });
}
