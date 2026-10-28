use canvas::{AgentProfile, CanvasRect};
use norma::protocol::{
    PendingQuestion, QuestionAnswer, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime};

fn setup() -> (Runtime, PendingQuestion) {
    let mut runtime = Runtime::new_for_workspace("/tmp/question-test");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "workspace".into(),
        workspace_route_id: "route".into(),
        norma_socket: "/tmp/question-test.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Ready,
        task_title: None,
        session_id: Some("session".into()),
    });
    let question = PendingQuestion {
        target: QuestionRef {
            workspace_id: "workspace".into(),
            project_id: "project".into(),
            node_id: node,
            provider: norma::protocol::AgentProfile::Claude,
            run_id: runtime.mobile_agent_run_id(node).unwrap().into(),
            connection_id: "connection".into(),
            session_id: "session".into(),
            turn_id: None,
            request_id: QuestionRequestId::String("tool-1".into()),
            revision: 1,
        },
        items: vec![QuestionItem {
            key: "0".into(),
            provider_id: None,
            prompt: "Which label?".into(),
            options: vec![QuestionOption {
                key: "green".into(),
                label: "Green".into(),
                description: None,
            }],
            multiple: false,
            allow_custom: true,
            secret: false,
        }],
    };
    runtime.bind_question_connection(&question.target).unwrap();
    (runtime, question)
}

fn answers() -> Vec<QuestionAnswer> {
    vec![QuestionAnswer {
        key: "0".into(),
        selected: vec!["green".into()],
        text: None,
    }]
}

/// 场景：同题重复提交只占有一次，旧连接与复用的 request ID 不可命中新题。
/// 构造：真实 Runtime/录制 PTY、已绑定会话和一个单选问题；第二连接复用 tool-1。
/// 执行：登记两次、两次提交，再替换连接并尝试旧引用。
/// 预期：只有首次获得发送资格，状态不提前变 Answered；旧引用被拒绝，新题独立待答。
#[test]
fn delivery_is_exclusive_and_connection_bound() {
    let (mut runtime, question) = setup();
    runtime.register_question(question.clone()).unwrap();
    runtime.register_question(question.clone()).unwrap();
    assert_eq!(runtime.questions().len(), 1, "重放同一请求不能产生两个待办");
    let first = runtime
        .claim_question_answer(&question.target, answers(), "delivery-1")
        .unwrap();
    assert!(first.is_some(), "首次占有必须取得唯一发送资格");
    assert!(
        runtime
            .claim_question_answer(&question.target, answers(), "delivery-2")
            .unwrap()
            .is_none(),
        "双击不能再次发送"
    );
    assert_eq!(
        runtime.question(&question.target).unwrap().state,
        norma::protocol::QuestionState::Submitting,
        "占有不等于答案被消费"
    );
    let mut next = question.clone();
    next.target.connection_id = "connection-2".into();
    runtime.bind_question_connection(&next.target).unwrap();
    runtime.register_question(next.clone()).unwrap();
    assert!(
        runtime
            .claim_question_answer(&question.target, answers(), "late")
            .is_err(),
        "旧连接不得命中同ID新题"
    );
    assert_eq!(
        runtime.question(&next.target).unwrap().state,
        norma::protocol::QuestionState::Pending,
        "新连接的同ID问题仍待答"
    );
}

/// 场景：真实待答期间普通任务输入与迟到 Stop 不得清除问题；人工键盘仍可接管。
/// 构造：已绑定的 Claude 单选问题，再发送一个来自同会话的 Stop/Done。
/// 执行：登记问题、提交普通任务、投递迟到状态，读取公开任务投影和问题表。
/// 预期：普通任务被阻止、问题保持 Pending/WaitingQuestion，原卡片人工输入仍可使用。
#[test]
fn pending_question_survives_late_status_and_blocks_task_input() {
    let (mut runtime, question) = setup();
    let node = question.target.node_id;
    runtime.register_question(question.clone()).unwrap();
    assert!(
        runtime.send_agent_task_text(node, "new task").is_err(),
        "普通任务不能落到问答控件"
    );
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "Stop".into(),
        to: AgentTaskState::Done,
        task_title: None,
        session_id: Some("session".into()),
    });
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::WaitingQuestion),
        "迟到Stop不证明问题已回答"
    );
    assert_eq!(
        runtime.question(&question.target).unwrap().state,
        norma::protocol::QuestionState::Pending,
        "独立问题表保持未决"
    );
    assert!(
        runtime
            .send_node_input(node, runtime::NodeInput::Text("human".into()))
            .is_ok(),
        "人工在原卡片仍能操作"
    );
}

/// 场景：写入或断连都不能当作已回答，恢复对账必须核对同题、同答案。
/// 构造：同一问题取得发送资格；一次无消费证据的提交、一次错答案和一次真实工具回执。
/// 执行：提交前复核、标记发送、断连、重复claim、最后提交消费证据。
/// 预期：断连停在Unknown且不重发，错误答案拒绝，匹配回执才Answered并恢复Provider最新状态。
#[test]
fn delivery_requires_matching_consumption_and_never_retries_unknown() {
    let (mut runtime, question) = setup();
    let node = question.target.node_id;
    runtime.register_question(question.clone()).unwrap();
    let submission = runtime
        .claim_question_answer(&question.target, answers(), "delivery")
        .unwrap()
        .unwrap();
    runtime.start_question_delivery(&submission).unwrap();
    assert_eq!(
        runtime.question(&question.target).unwrap().state,
        norma::protocol::QuestionState::Submitted,
        "开始传输不等于消费"
    );
    runtime
        .question_delivery_unknown(&submission, "transport disconnected")
        .unwrap();
    assert!(
        runtime
            .claim_question_answer(&question.target, answers(), "retry")
            .unwrap()
            .is_none(),
        "不明结果不得重发"
    );
    let wrong = vec![QuestionAnswer {
        key: "0".into(),
        selected: vec![],
        text: Some("Blue".into()),
    }];
    assert!(
        runtime
            .confirm_question_answer(&submission, &wrong, "tool result")
            .is_err(),
        "错答案不能确认"
    );
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "PostToolUse".into(),
        to: AgentTaskState::Thinking,
        task_title: None,
        session_id: Some("session".into()),
    });
    runtime
        .confirm_question_answer(&submission, &answers(), "PostToolUse tool-1 Green")
        .unwrap();
    assert_eq!(
        runtime.question(&question.target).unwrap().state,
        norma::protocol::QuestionState::Answered,
        "同题同答案消费回执才是成功"
    );
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Thinking),
        "恢复最新Provider状态，不能硬设Done"
    );
    assert!(
        !runtime.has_pending_question(node),
        "确认后解除普通任务输入阻塞"
    );
}

/// 场景：版本更新、节点关闭和连接中断不能留下可用旧提交资格。
/// 构造：同request的修订2更换题目，另构造已进入传输的未确认请求。
/// 执行：更新修订后提交旧引用；传输后重绑连接并登记同题；最后删除节点。
/// 预期：旧修订不可答，不明投递对账前不能重发；节点关闭使全部旧资格失效。
#[test]
fn changed_revision_and_uncertain_reconnect_require_reconciliation() {
    let (mut runtime, question) = setup();
    runtime.register_question(question.clone()).unwrap();
    let mut revised = question.clone();
    revised.target.revision = 2;
    revised.items[0].prompt = "Which NEW label?".into();
    runtime.register_question(revised.clone()).unwrap();
    assert!(
        runtime
            .claim_question_answer(&question.target, answers(), "old")
            .unwrap()
            .is_none(),
        "旧修订必须失效"
    );
    let submission = runtime
        .claim_question_answer(&revised.target, answers(), "current")
        .unwrap()
        .unwrap();
    runtime.start_question_delivery(&submission).unwrap();
    let mut replay = revised.clone();
    replay.target.connection_id = "reconnected".into();
    runtime.bind_question_connection(&replay.target).unwrap();
    assert!(
        runtime.register_question(replay).is_err(),
        "已经可能提交的原请求必须先对账，不能因连接换代再次回答"
    );
    assert!(
        runtime.has_pending_question(revised.target.node_id),
        "不明结果继续阻止新任务写入原问答"
    );
    runtime.remove_node(revised.target.node_id);
    assert!(
        runtime.start_question_delivery(&submission).is_err(),
        "节点关闭后旧资格无效"
    );
    assert!(
        !runtime.has_pending_question(revised.target.node_id),
        "已删除节点没有可操作问答"
    );
}

/// 场景：另一个 session 的迟到 Stop 不能替换当前问答的会话归属。
/// 构造：原生卡片的 session 正在等待 tool-1，随后收到 stale-session 的 Done。
/// 执行：通过公开状态入口投递迟到事件，再尝试派发普通任务。
/// 预期：原 session、待答投影和输入阻塞均保持，迟到事件没有绑定副作用。
#[test]
fn foreign_session_status_cannot_bypass_question() {
    let (mut runtime, question) = setup();
    let node = question.target.node_id;
    runtime.register_question(question).unwrap();
    let outcome = runtime.apply_agent_task_update_with_outcome(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "Stop".into(),
        to: AgentTaskState::Done,
        task_title: None,
        session_id: Some("stale-session".into()),
    });
    assert!(
        !outcome.session_binding_changed,
        "旧事件不能改绑正在等待回答的会话"
    );
    assert_eq!(
        runtime.agent_session_id(node).as_deref(),
        Some("session"),
        "原问题仍属于原会话"
    );
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::WaitingQuestion),
        "旧会话Done不能解除等待"
    );
    assert!(
        runtime.send_agent_task_text(node, "next task").is_err(),
        "任务输入不能穿透待答阻塞"
    );
}

/// 场景：连接更换使未发出的旧题过期时，不需要另一条事件才能恢复投影。
/// 构造：底层 Ready、上层 WaitingQuestion 的原生卡片与新连接代数。
/// 执行：仅重绑连接，不登记新问题、不发送普通状态。
/// 预期：旧题过期、没有待答，卡片立即恢复 Ready。
#[test]
fn reconnect_refreshes_projection_without_followup_event() {
    let (mut runtime, question) = setup();
    runtime.register_question(question.clone()).unwrap();
    let mut replacement = question.target.clone();
    replacement.connection_id = "new-connection".into();
    runtime.bind_question_connection(&replacement).unwrap();
    assert!(
        !runtime.has_pending_question(replacement.node_id),
        "未发送旧题应失效"
    );
    assert_eq!(
        runtime.agent_task_state(replacement.node_id),
        Some(AgentTaskState::Ready),
        "连接重绑须立即刷新投影"
    );
}

/// 场景：关闭节点或恢复画布后，旧消费回执不能复活已清理的状态。
/// 构造：已发送的问答和包含其节点的画布快照，分别执行删除与恢复。
/// 执行：生命周期变化后送入原提交凭据和匹配答案。
/// 预期：迟到确认被拒绝，节点没有任务投影；旧绑定和问题记录被撤销。
#[test]
fn late_consumption_cannot_restore_removed_or_reloaded_node_state() {
    for restore in [false, true] {
        let (mut runtime, question) = setup();
        let node = question.target.node_id;
        runtime.register_question(question.clone()).unwrap();
        let submission = runtime
            .claim_question_answer(&question.target, answers(), "delivery")
            .unwrap()
            .unwrap();
        runtime.start_question_delivery(&submission).unwrap();
        if restore {
            let snapshot = runtime.collect_canvas_snapshot();
            runtime.restore_canvas_snapshot(snapshot);
        } else {
            runtime.remove_node(node);
        }
        assert!(
            runtime
                .confirm_question_answer(&submission, &answers(), "late tool result")
                .is_err(),
            "旧运行实例的回执必须失效"
        );
        assert_eq!(
            runtime.agent_task_state(node),
            None,
            "迟到回执不能重建任务状态"
        );
        assert!(runtime.questions().is_empty(), "关闭和恢复须清理活问答登记");
    }
}

/// 场景：超过历史容量后，同连接已回答的请求不能再次取得发送资格。
/// 构造：同一个活会话内依次登记并确认256道不同请求（当前registry上限）。
/// 执行：尝试登记第257题后重放第一题，再次claim。
/// 预期：可以拒绝新题或安全压缩，但第一题绝不能再次可发送。
#[test]
fn history_capacity_never_resurrects_answered_request() {
    let (mut runtime, original) = setup();
    for index in 0..256 {
        let mut question = original.clone();
        question.target.request_id = QuestionRequestId::String(format!("tool-{index}"));
        runtime.register_question(question.clone()).unwrap();
        let submission = runtime
            .claim_question_answer(&question.target, answers(), &format!("delivery-{index}"))
            .unwrap()
            .unwrap();
        runtime.start_question_delivery(&submission).unwrap();
        runtime
            .confirm_question_answer(&submission, &answers(), "tool result")
            .unwrap();
    }
    let mut extra = original.clone();
    extra.target.request_id = QuestionRequestId::String("tool-extra".into());
    let _ = runtime.register_question(extra);
    let mut replay = original;
    replay.target.request_id = QuestionRequestId::String("tool-0".into());
    let _ = runtime.register_question(replay.clone());
    let claim = runtime.claim_question_answer(&replay.target, answers(), "duplicate");
    assert!(
        !matches!(claim, Ok(Some(_))),
        "容量清理不能丢失已经回答的防重证据"
    );
}

/// 场景：会话发现、恢复和原生Hook不能从侧路改绑正在等待的问题。
/// 构造：三个独立原生Runtime各登记同一题，分别尝试另一session发现、空恢复与旧Stop。
/// 执行：逐个调用公开入口，再检查原会话身份和普通任务输入。
/// 预期：三个入口都保留原绑定与待答，不能使新任务写进原问答控件。
#[test]
fn discovery_hydration_and_native_hook_preserve_waiting_session() {
    for entrypoint in ["discovery", "hydration", "native-hook"] {
        let (mut runtime, question) = setup();
        let node = question.target.node_id;
        runtime.register_question(question).unwrap();
        match entrypoint {
            "discovery" => {
                runtime.bind_discovered_agent_session(node, "stale-session");
            }
            "hydration" => {
                runtime.hydrate_agent_session_bindings(&[]);
            }
            _ => {
                let event = norma::agent_hook::parse_hook_event(
                    norma::agent_hook::HookSource::Claude,
                    node,
                    serde_json::json!({"hook_event_name":"Stop","session_id":"stale-session"}),
                )
                .unwrap();
                runtime.observe_mobile_hook(&event);
            }
        }
        assert_eq!(
            runtime.agent_session_id(node).as_deref(),
            Some("session"),
            "{entrypoint}不能替换问答会话"
        );
        assert!(
            runtime.has_pending_question(node),
            "{entrypoint}不能隐式关闭问题"
        );
        assert!(
            runtime.send_agent_task_text(node, "next").is_err(),
            "{entrypoint}不能解除输入阻塞"
        );
    }
}
