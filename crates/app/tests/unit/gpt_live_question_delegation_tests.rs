use super::super::bridge::LiveBridge;
use super::*;
#[path = "../support/brain_question.rs"]
mod support;

/// 场景：GPT Live 的可信话语委派必须回答当前题，不得另进普通任务规划。
/// 构造：隔离 App/原 Claude 问题和 LiveBridge；覆盖独立命令、闲聊后命令、分片、否定、取消。
/// 执行：真实 bridge 认领用户转写，再调用生产 dispatch，检查宿主授权及执行回执。
/// 预期：独立/分片命令产生单题 grant；混合原话、否定与取消无 grant；旧命令不拦新任务。
#[test]
fn live_question_delegation_uses_current_user_turn_and_returns_a_receipt() {
    for scenario in [
        "plain",
        "chat",
        "split",
        "negative",
        "interrupted_negative",
        "cancelled",
        "followup_task",
        "restate",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
        let mut test = gpui::TestAppContext::single();
        let (source, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        source.update_in(cx, |app, window, cx| {
            let raw = serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
            ))
            .unwrap();
            let (target, _, _) = support::make_question_from_raw(app, cx, dir.path(), raw);
            app.runtime.focus.exit_to_canvas();
            app.runtime.selection.select_only(target.node_id);
            app.install_question_brain_for_test();
            let (generation, cancellation) = app.gpt_live.begin();
            app.gpt_live.context_scope =
                Some(app.shared_context_service().unwrap().scope().clone());
            let mut bridge = LiveBridge::new("question-call".into(), cancellation.clone());
            let fragment = |id: &str, speaker, text: &str, start| live_voice::TranscriptFragment {
                event_id: id.into(),
                speaker,
                delta: text.into(),
                start_ms: start,
                end_ms: start + 100.,
            };
            if matches!(scenario, "chat" | "interrupted_negative" | "restate") {
                bridge
                    .transcript(fragment(
                        "greeting",
                        live_voice::Speaker::User,
                        if scenario != "interrupted_negative" {
                            "你好"
                        } else {
                            "不要"
                        },
                        0.,
                    ))
                    .unwrap();
                bridge
                    .transcript(fragment(
                        "greeting-reply",
                        live_voice::Speaker::Assistant,
                        "你好",
                        200.,
                    ))
                    .unwrap();
            }
            let text = if scenario == "negative" {
                "不要这题你直接回答"
            } else if scenario == "split" {
                "这题你"
            } else {
                "这题你直接回答"
            };
            bridge
                .transcript(fragment("command", live_voice::Speaker::User, text, 500.))
                .unwrap();
            if scenario == "split" {
                bridge
                    .transcript(fragment(
                        "command-tail",
                        live_voice::Speaker::User,
                        "直接回答",
                        600.,
                    ))
                    .unwrap();
            }
            bridge
                .delegation(live_voice::DelegationNotice {
                    id: "delegate".into(),
                    offset_ms: 800.,
                })
                .unwrap();
            let work = bridge.claim_next().expect("用户转写已组成可信委派");
            let scope = LiveAdapterScope {
                project: dir.path().display().to_string(),
                generation,
                timeline: bridge.timeline.clone(),
                snapshot: work.snapshot.clone(),
            };
            if scenario == "cancelled" {
                cancellation.cancel();
            }
            let (sent, received) = async_channel::bounded(1);
            start(app, work, scope, sent, window, cx);
            let accepted = matches!(scenario, "plain" | "split" | "followup_task");
            assert_eq!(
                app.question_delegated_for_test(&target),
                accepted,
                "{scenario} 必须按完整当前原话授权"
            );
            let completion = received
                .try_recv()
                .expect("本地委托或拒绝应立即返回，不依赖模型凭据");
            if matches!(scenario, "followup_task" | "restate") {
                assert!(
                    bridge
                        .complete(&completion.work, completion.report)
                        .is_some(),
                    "首轮回执完成并结算原话"
                );
                let next_text = if scenario == "followup_task" {
                    "运行测试"
                } else {
                    "这题你直接回答"
                };
                bridge
                    .transcript(fragment(
                        "next",
                        live_voice::Speaker::User,
                        next_text,
                        1000.,
                    ))
                    .unwrap();
                bridge
                    .delegation(live_voice::DelegationNotice {
                        id: "next-delegate".into(),
                        offset_ms: 1200.,
                    })
                    .unwrap();
                let work = bridge.claim_next().expect("新原话可独立委派");
                assert_eq!(
                    work.snapshot.user_input, next_text,
                    "已结算的命令不可混入当前输入"
                );
                let scope = LiveAdapterScope {
                    project: dir.path().display().to_string(),
                    generation,
                    timeline: bridge.timeline.clone(),
                    snapshot: work.snapshot.clone(),
                };
                let (sent, received) = async_channel::bounded(1);
                start(app, work, scope, sent, window, cx);
                let completion = received.try_recv().expect("无凭据或本地授权均即时回执");
                if scenario == "followup_task" {
                    assert!(
                        matches!(
                            completion.report.planner,
                            Err(PlannerFailure::Configuration)
                        ),
                        "普通新任务应走正常凭据检查，不能被历史委托拦截"
                    );
                    assert!(
                        completion.report.receipts.is_empty(),
                        "历史命令不可伪造本轮问答拒绝"
                    );
                    assert!(
                        !app.question_delegated_for_test(&target),
                        "新输入已撤销旧题授权"
                    );
                } else {
                    assert!(completion.report.effects, "拒绝混合原话后完整重述应能授权");
                    assert!(
                        app.question_delegated_for_test(&target),
                        "新完整命令成功授权当前题"
                    );
                }
                return;
            }
            if accepted {
                assert!(
                    completion.report.effects,
                    "创建单题授权是副作用，不能重排旧委派"
                );
                assert!(
                    completion.report.message().contains("已委托"),
                    "只报告委托回执，不伪称已回答"
                );
                if scenario == "split" {
                    assert!(
                        bridge
                            .complete(&completion.work, completion.report)
                            .is_some(),
                        "当前回执可播报委托"
                    );
                    bridge
                        .transcript(fragment("late", live_voice::Speaker::User, "吗", 900.))
                        .unwrap();
                } else {
                    bridge
                        .transcript(fragment("late", live_voice::Speaker::User, "吗", 600.))
                        .unwrap();
                    assert!(
                        bridge
                            .complete(&completion.work, completion.report)
                            .is_none(),
                        "修订后的旧回执不口播"
                    );
                }
                assert!(
                    !app.question_delegated_for_test(&target),
                    "迟到尾字撤销未消费授权，complete后同样生效"
                );
                assert!(
                    bridge.claim_next().is_none(),
                    "已有授权的旧委派不能重排绑定下一题"
                );
            } else if matches!(scenario, "chat" | "interrupted_negative") {
                assert!(
                    completion.report.message().contains("尚未授权"),
                    "不能用助手插话截掉已知用户前缀"
                );
            }
            assert!(
                app.agent_task_coordinator
                    .as_ref()
                    .is_none_or(|c| c.tasks().is_empty()),
                "单题委托不派普通任务"
            );
        });
    }
}
