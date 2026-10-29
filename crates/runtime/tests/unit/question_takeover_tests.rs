use super::*;
use norma::agent_hook::QuestionHookStatus;

/// 场景：UI/CLI快照区分答案在Hook缓冲、部分写出、完整写出与传输中断。
/// 构造：原问题占有并投递到Hook，但尚未写socket。
/// 执行：读公开questions快照，依次取出响应、完整写出；另一次取消缓冲响应。
/// 预期：Ready/Responding仅显示Submitting，Responded才Submitted，取消后Unknown。
#[test]
fn public_question_snapshot_only_reports_submitted_after_complete_write() {
    for cancelled in [false, true] {
        let (mut runtime, ticket, target) = setup();
        let submission = runtime
            .claim_question_answer(
                &target,
                vec![QuestionAnswer {
                    key: "question-0".into(),
                    selected: vec!["option-1".into()],
                    text: None,
                }],
                "delivery",
            )
            .unwrap()
            .unwrap();
        runtime.deliver_native_question(&submission).unwrap();
        let state = |runtime: &Runtime| {
            runtime
                .questions()
                .into_iter()
                .find(|q| q.question.target == target)
                .unwrap()
                .state
        };
        assert_eq!(
            state(&runtime),
            QuestionState::Submitting,
            "缓冲中不能声称已经提交"
        );
        ticket.take_response(Instant::now()).unwrap();
        assert_eq!(
            state(&runtime),
            QuestionState::Submitting,
            "取出缓冲也不能证明完整写出"
        );
        if cancelled {
            ticket.cancel();
            assert_eq!(
                state(&runtime),
                QuestionState::Unknown,
                "取消部分传输保留结果待确认"
            );
        } else {
            ticket
                .with_response_write(Instant::now(), || Ok(true))
                .unwrap();
            assert_eq!(
                state(&runtime),
                QuestionState::Submitted,
                "完整写出后才报告提交完成"
            );
        }
        assert!(
            runtime.has_pending_question(target.node_id),
            "无消费证据仍然保护原问题"
        );
    }
}

/// 场景：用户从概览转到原卡片，必须撤销未完成代答，仍保留原问题输入保护。
/// 构造：原Hook分别停在Pending、准备中、Ready、已完整写出四个阶段。
/// 执行：调用生产接管入口，再尝试迟到提交和普通任务写入。
/// 预期：前两阶段撤销资格，Ready转待确认，已写出保留Submitted；均不允许再次代答。
#[test]
fn takeover_revokes_unsent_delivery_and_preserves_inflight_evidence() {
    for phase in 0..4 {
        let (mut runtime, ticket, target) = setup();
        let submission = (phase > 0).then(|| {
            runtime
                .claim_question_answer(
                    &target,
                    vec![QuestionAnswer {
                        key: "question-0".into(),
                        selected: vec!["option-1".into()],
                        text: None,
                    }],
                    "delivery",
                )
                .unwrap()
                .unwrap()
        });
        if phase >= 2 {
            runtime
                .deliver_native_question(submission.as_ref().unwrap())
                .unwrap();
        }
        if phase == 3 {
            ticket.take_response(Instant::now()).unwrap();
            ticket
                .with_response_write(Instant::now(), || Ok(true))
                .unwrap();
        }
        runtime.release_question_to_native(&target).unwrap();
        assert!(
            !runtime.native_question_available(&target),
            "人工接管后不得再次代答，phase={phase}"
        );
        assert!(
            runtime.has_pending_question(target.node_id),
            "接管不等于回答完成，phase={phase}"
        );
        assert!(
            runtime
                .send_agent_task_text(target.node_id, "new task")
                .is_err(),
            "接管不能放行普通任务"
        );
        assert_eq!(
            ticket.status(Instant::now()),
            if phase == 3 {
                QuestionHookStatus::Responded
            } else {
                QuestionHookStatus::Cancelled
            },
            "撤销未写出响应但保留已写出证据"
        );
        assert_eq!(
            runtime.question(&target).unwrap().state,
            match phase {
                0 | 1 => QuestionState::Pending,
                2 => QuestionState::Unknown,
                _ => QuestionState::Submitted,
            },
            "接管保留精确传输状态"
        );
        if let Some(submission) = submission {
            assert!(
                runtime.deliver_native_question(&submission).is_err(),
                "迟到后台工作不能覆盖人工输入"
            );
        }
    }
}
