use super::test_support::{accept, with_cards};
use agent::card_instance::CardInstanceAssignment;
use agent::card_instance::CardInstanceStage;
use agent::card_instance::CardInstanceTracking;
use canvas::AgentProfile;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 用户输入与审批阻止新队列，原 pending/reply 输入占用也不能被新队列绕过。
/// 数据构造：第二卡的用户草稿 1 次写入、审批 1 状态；空间任务 1 项待发送。
/// 执行过程：用户先写草稿，接收任务并推进；用户 Enter 后设审批再次推进；恢复 Thinking 后推进。
/// 预期结果：草稿和审批期间仅有用户自己的正文/Enter；Thinking 可发送新任务，非目标零输入。
#[test]
fn user_draft_and_approval_remain_protected_before_delivery() {
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            app.runtime
                .bind_discovered_agent_session(nodes[1], "session");
            app.runtime
                .send_node_input(nodes[1], runtime::NodeInput::Text("用户草稿".into()))
                .expect("用户输入");
            accept(
                app,
                nodes[1],
                "queued",
                "新任务",
                CardInstanceTracking::Managed,
                None,
            );
            app.advance_card_instance_tasks_at(now);
            assert_eq!(
                app.card_instance_receipt("queued").expect("任务").stage,
                CardInstanceStage::WaitingInput,
                "草稿时等待输入"
            );
            app.runtime
                .send_node_input(
                    nodes[1],
                    runtime::NodeInput::Key(terminal::KeyInput {
                        key: terminal::Key::Enter,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    }),
                )
                .expect("用户提交");
            app.runtime
                .apply_agent_task_update(runtime::AgentTaskUpdate {
                    node_id: nodes[1],
                    profile: AgentProfile::OpenCode,
                    provider_event: "PermissionRequest".into(),
                    to: runtime::AgentTaskState::WaitingApproval { tool_name: None },
                    task_title: None,
                    session_id: None,
                });
            app.advance_card_instance_tasks_at(now + Duration::from_secs(1));
        });
        assert_eq!(
            writes[1].lock().expect("输入").len(),
            2,
            "草稿和审批仅用户正文+Enter，不混入新任务"
        );
        app.update(cx, |app, _| {
            app.runtime
                .apply_agent_task_update(runtime::AgentTaskUpdate {
                    node_id: nodes[1],
                    profile: AgentProfile::OpenCode,
                    provider_event: "PreToolUse".into(),
                    to: runtime::AgentTaskState::Thinking,
                    task_title: None,
                    session_id: None,
                });
            app.advance_card_instance_tasks_at(now + Duration::from_secs(2));
            app.advance_card_instance_tasks_at(now + Duration::from_millis(2101));
        });
        assert_eq!(
            writes[1].lock().expect("输入").len(),
            4,
            "A 仍 Thinking 时新任务合法提交，不等终态"
        );
        assert!(writes[0].lock().expect("首卡").is_empty(), "非目标零输入");
    });
}

/// 取消权限在正文之后撤销时，Enter 必须停下并标为待核对，不能安全重试。
/// 数据构造：1 个内存授权 gate，正文写入 1 次；100 ms Enter 前将 gate 置 false。
/// 执行过程：guarded accept，推进正文，撤销授权，再推进到 Enter 时间。
/// 预期结果：OutcomeUnknown、retry_safe=false，只有原正文；后续 B 不被自动输入。
#[test]
fn permission_gate_revocation_after_paste_stops_enter_with_unknown_outcome() {
    use std::sync::atomic::{AtomicBool, Ordering};
    with_cards(|app, cx, nodes, writes| {
        let gate = Arc::new(AtomicBool::new(true));
        let now = Instant::now();
        app.update(cx, |app, _| {
            let target = app
                .runtime
                .workspace_card_target("workspace", "area", 1, nodes[1])
                .expect("目标");
            app.accept_guarded_card_instance_assignment(
                "workspace",
                "area",
                1,
                CardInstanceAssignment {
                    assignment_id: "guarded".into(),
                    target,
                    name_at_acceptance: "Betty".into(),
                    definition_id: None,
                    text: "撤权前正文".into(),
                    tracking: CardInstanceTracking::Untracked,
                    force_untracked: false,
                },
                gate.clone(),
            )
            .expect("有效授权接收");
            app.advance_card_instance_tasks_at(now);
        });
        gate.store(false, Ordering::Release);
        app.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            let receipt = app.card_instance_receipt("guarded").expect("原引用");
            assert_eq!(
                receipt.stage,
                CardInstanceStage::OutcomeUnknown,
                "已写正文但未 Enter 应待核对"
            );
            assert!(
                !receipt.failure.expect("失败依据").retry_safe,
                "不能盲目安全重试正文"
            );
            let target = app
                .runtime
                .workspace_card_target("workspace", "area", 1, nodes[1])
                .expect("原卡 target");
            app.accept_card_instance_assignment(
                "workspace",
                "area",
                1,
                CardInstanceAssignment {
                    assignment_id: "b".into(),
                    target,
                    name_at_acceptance: "Betty".into(),
                    definition_id: None,
                    text: "后续正文".into(),
                    tracking: CardInstanceTracking::Untracked,
                    force_untracked: true,
                },
            )
            .expect("即使用户明确再发，也要受未知输入保护");
            app.advance_card_instance_tasks_at(now + Duration::from_secs(1));
        });
        assert_eq!(
            *writes[1].lock().expect("目标"),
            vec!["撤权前正文".as_bytes().to_vec()],
            "撤权后不能发 Enter、清行或后续正文"
        );
        assert!(
            writes[0].lock().expect("首卡").is_empty(),
            "另一卡片也零输入"
        );
    });
}

/// 无可靠跟踪且可能忙的卡片只能在用户明确授权继续发送后接收下一项，force 不结束旧任务。
/// 数据构造：第二卡一项 untracked 任务已提交，随后构造同目标的新任务，默认 force=false。
/// 执行过程：经公开入口接收/发送首项，预检和接收默认次项，再显式 force 后接收并推进。
/// 预期结果：默认拒绝是零写入且不产生回执；显式授权后仅多一次正文与 Enter，首项仍 Submitted。
#[test]
fn public_preflight_requires_explicit_force_for_busy_untracked_card() {
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            accept(
                app,
                nodes[1],
                "first",
                "首项",
                CardInstanceTracking::Untracked,
                None,
            );
            let target = app
                .runtime
                .workspace_card_target("workspace", "area", 1, nodes[1])
                .expect("原实例");
            let mut second = CardInstanceAssignment {
                assignment_id: "second".into(),
                target,
                name_at_acceptance: "Betty".into(),
                definition_id: None,
                text: "次项".into(),
                tracking: CardInstanceTracking::Untracked,
                force_untracked: false,
            };
            let error = app
                .preflight_card_instance_assignment("workspace", "area", 1, &second)
                .expect_err("整组接收前即可发现无授权");
            assert_eq!(
                error.code, "untracked_requires_force",
                "明确需用户继续发送授权"
            );
            assert!(error.zero_input_proven, "纯预检必须零写入");
            assert_eq!(
                app.card_instance_receipt("first").expect("首项").stage,
                CardInstanceStage::Accepted,
                "等待中的未终态也必须与正式接收一致拒绝次项"
            );
            assert!(
                app.accept_card_instance_assignment("workspace", "area", 1, second.clone())
                    .is_err(),
                "不能绕过预检入口"
            );
            assert_eq!(
                app.card_instance_receipt("second"),
                None,
                "拒绝不产生接收回执"
            );
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            assert!(
                app.preflight_card_instance_assignment("workspace", "area", 1, &second)
                    .is_err(),
                "首项提交后也继续需要显式授权"
            );
            second.force_untracked = true;
            app.accept_card_instance_assignment("workspace", "area", 1, second)
                .expect("明确授权后可接收");
            app.advance_card_instance_tasks_at(now + Duration::from_millis(102));
            app.advance_card_instance_tasks_at(now + Duration::from_millis(203));
            assert_eq!(
                app.card_instance_receipt("first").expect("首项").stage,
                CardInstanceStage::Submitted,
                "继续发送不虚构首项完成"
            );
            assert_eq!(
                app.card_instance_receipt("second").expect("次项").stage,
                CardInstanceStage::Submitted,
                "无 Hook 时只承诺已提交"
            );
        });
        assert_eq!(
            writes[1].lock().expect("目标输入").len(),
            4,
            "两项仅各正文+Enter，拒绝项无额外输入"
        );
        assert!(
            writes[0].lock().expect("首卡").is_empty(),
            "强制发送仍不能改投其他卡片"
        );
    });
}
