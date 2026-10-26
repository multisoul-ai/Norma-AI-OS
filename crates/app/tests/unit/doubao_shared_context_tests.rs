use super::*;
use agent::{
    context::*, AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, ToolContext,
};
use futures::future::BoxFuture;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Frames {
    reads: AtomicUsize,
    large: bool,
    escaped: bool,
    invalidated: Option<Arc<AtomicBool>>,
}
impl AgentCardAdapter for Frames {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("read-only fixture".into())) })
    }
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Box::pin(async move {
            assert_eq!(
                context.turn_id, "session:turn-7",
                "资料与工具必须使用同一轮次身份"
            );
            assert_eq!(
                context.input_source,
                agent::AgentInputSource::DoubaoVoice,
                "不能把豆包原话记成 Dock 输入"
            );
            if self
                .invalidated
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst))
            {
                return Err(AdapterError::Rejected("context_scope_mismatch".into()));
            }
            let version = self.reads.fetch_add(1, Ordering::SeqCst) as u64 + 1;
            let revision = version + 1; // revision 1 is the same immutable current input.
            let scope = ContextScope {
                workspace_id: "w".into(),
                area_id: "a".into(),
                project: ContextProject::Project {
                    project_id: "p".into(),
                    worktree: "/project".into(),
                },
                task_id: None,
                conversation_id: None,
            };
            let reference = ContextRef {
                scope: scope.clone(),
                event_id: format!("event-{revision}"),
                revision,
            };
            let text = if self.large {
                "\"".repeat(REQUEST_INPUT_BYTES)
            } else {
                format!("目标版本{version}")
            };
            let mut state = WorkingState {
                goal: Some(ContextFact {
                    reference: reference.clone(),
                    text,
                }),
                ..Default::default()
            };
            let mut events = vec![];
            if self.escaped {
                state.goal.as_mut().unwrap().text = "\"".repeat(8192);
                for n in 0..2 {
                    state.constraints.insert(
                        format!("rule-{n}"),
                        ContextConstraint {
                            reference: reference.clone(),
                            text: "\"".repeat(8192),
                            enforcement: ConstraintEnforcement::Communicated,
                        },
                    );
                }
                for n in 0..15 {
                    let mut source = reference.clone();
                    source.event_id = format!("report-{n}");
                    events.push(RecordedContextEvent {
                        reference: source.clone(),
                        event: ContextEvent {
                            event_id: source.event_id.clone(),
                            scope: scope.clone(),
                            source: ContextSource {
                                origin: ContextOrigin::Brain,
                                stream_id: "reports".into(),
                                sequence: n + 1,
                            },
                            created_at_ms: 1,
                            related: vec![],
                            payload: ContextPayload::Report {
                                text: format!("q{}", "\"".repeat(8191)),
                            },
                        },
                    });
                }
            }
            let input = ContextRef {
                scope: scope.clone(),
                event_id: "input-7".into(),
                revision: 1,
            };
            Ok(Some(ContextFrame {
                input,
                snapshot: ContextSnapshot {
                    scope,
                    revision,
                    state,
                    events,
                },
            }))
        })
    }
}

fn turn() -> ToolContext {
    ToolContext {
        project_id: "/project".into(),
        turn_id: "session:turn-7".into(),
        user_input: "当前目标是什么".into(),
        input_source: agent::AgentInputSource::DoubaoVoice,
        contextual_delete_targets: vec![],
        tool_call_id: String::new(),
        operation_id: None,
        cancellation: Default::default(),
    }
}
fn environment() -> ContextRequests {
    let (requests, incoming): (ContextRequests, _) = async_channel::bounded(1);
    tokio::spawn(async move {
        while let Ok(ContextRequest::Model(reply)) = incoming.recv().await {
            let _ = reply.send((
                agent::CurrentCanvasContext {
                    project_name: "当前项目".into(),
                    workspace_root: "/project".into(),
                    total_cards: 0,
                    cards: vec![],
                    conversation_tails: vec![],
                },
                vec![],
                None,
            ));
        }
    });
    requests
}

/// 场景：豆包同一回合调用工具后，后续模型请求要读最新共享状态。
/// 构造：连续读取返回版本1和2；原生历史仅一条当前用户消息。
/// 执行：两次调用生产请求装配入口，检查实际 wire JSON 的角色、版本及历史。
/// 预期：每次恰好一条独立资料消息，正文不进 system 或持久历史，第二次不保留版本1。
#[tokio::test]
async fn doubao_shared_context_refreshes_each_request_without_polluting_history() {
    let adapter = Arc::new(Frames {
        reads: AtomicUsize::new(0),
        large: false,
        escaped: false,
        invalidated: None,
    });
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = environment();
    let turn = turn();
    let original = vec![json!({"role":"user","content":turn.user_input})];
    let mut history = original.clone();
    for version in [1, 2] {
        let body =
            request_with_shared_context(&context, "", &mut history, Some(&runtime), &turn, false)
                .await
                .unwrap();
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3, "system、共享资料和当前原话各一条");
        assert_eq!(
            messages[1]["role"], "user",
            "共享资料不能提升为 system 指令"
        );
        let data: Value = serde_json::from_str(messages[1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            data["working_state"][0]["revision"],
            version + 1,
            "每次请求应重新获取快照"
        );
        assert_eq!(
            data["current_input"]["event_id"], "input-7",
            "同回合来源身份不能随刷新改变"
        );
        assert!(
            messages[1]["content"]
                .as_str()
                .unwrap()
                .contains(&format!("目标版本{version}")),
            "资料应使用本次有效目标"
        );
        assert!(
            !messages[0]["content"]
                .as_str()
                .unwrap()
                .contains("目标版本"),
            "数据不能拼入系统指令"
        );
        assert_eq!(messages[2], original[0], "当前输入必须原样保留");
        assert_eq!(history, original, "共享资料不能积累到原生历史");
        assert!(
            body.to_string().len() <= REQUEST_INPUT_BYTES,
            "完整请求序列化后须符合预算"
        );
        if version == 2 {
            assert!(
                !body.to_string().contains("目标版本1"),
                "不能保留旧快照的目标"
            );
        }
    }
    assert_eq!(
        adapter.reads.load(Ordering::SeqCst),
        2,
        "两次请求各读取一次状态"
    );
}

/// 场景：必需工作状态在 JSON 转义后超出豆包窗口，不能删掉规则后继续请求。
/// 构造：目标含 request_budget 个双引号，外层转义后至少翻倍；当前输入很短。
/// 执行：通过同一生产装配入口创建 wire JSON。
/// 预期：返回预算错误，原生当前输入保留，不返回不完整请求。
#[tokio::test]
async fn doubao_shared_context_rejects_oversized_required_state() {
    let runtime = NormaToolRuntime::new(Arc::new(Frames {
        reads: AtomicUsize::new(0),
        large: true,
        escaped: false,
        invalidated: None,
    }));
    let mut history = vec![json!({"role":"user","content":"当前目标是什么"})];
    let original = history.clone();
    let result = request_with_shared_context(
        &environment(),
        "",
        &mut history,
        Some(&runtime),
        &turn(),
        false,
    )
    .await;
    assert!(
        result.is_err(),
        "必需状态超限必须停止请求，不能丢弃限制继续回答"
    );
    assert_eq!(history, original, "超限失败不能截断当前用户输入");
}

/// 场景：可选资料有大量 JSON 转义时，必需目标仍放得下，不能误报窗口不足。
/// 构造：目标及两条限制各 8192 个引号，15 条相关报告各 8192 字节；基础提示 20KB。
/// 执行：调用正式豆包请求装配入口，按完整外层 JSON 测量。
/// 预期：保留全部有效限制与当前输入，按需裁掉部分报告，生成窗口内的请求。
#[tokio::test]
async fn doubao_shared_context_trims_escaped_optional_reports_before_required_state() {
    let runtime = NormaToolRuntime::new(Arc::new(Frames {
        reads: AtomicUsize::new(0),
        large: false,
        escaped: true,
        invalidated: None,
    }));
    let mut turn = turn();
    turn.user_input = "q".into();
    let original = vec![json!({"role":"user", "content":"q"})];
    let mut history = original.clone();
    let body = request_with_shared_context(
        &environment(),
        &"x".repeat(20_000),
        &mut history,
        Some(&runtime),
        &turn,
        false,
    )
    .await
    .expect("必需状态和原话约 121KB，必须能装进 439808B 窗口");
    let data: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        data["working_state"][0]["state"]["constraints"]
            .as_object()
            .unwrap()
            .len(),
        2,
        "两条有效限制均须保留"
    );
    let reports = data["brain_materials"].as_array().unwrap().len();
    assert!(
        reports > 0 && reports < 15,
        "应选入一部分相关报告，而非全部或全部丢弃"
    );
    assert!(
        body.to_string().len() <= REQUEST_INPUT_BYTES,
        "按完整 JSON 的真实转义大小验预算"
    );
    assert_eq!(history, original, "原话和原生历史保持不变");
}

/// 场景：等待环境快照期间来源工作区失效，不能发送旧 frame 与新环境的组合。
/// 构造：Model 环境响应将来源失效标志置位；共享资料接口随后拒绝该来源。
/// 执行：调用正式请求装配入口，切换点严格发生在环境响应之前，无 sleep。
/// 预期：最后的共享来源校验拒绝请求，不能返回已混入旧状态的 wire body。
#[tokio::test]
async fn doubao_shared_context_revalidates_source_after_environment_await() {
    let invalidated = Arc::new(AtomicBool::new(false));
    let runtime = NormaToolRuntime::new(Arc::new(Frames {
        reads: AtomicUsize::new(0),
        large: false,
        escaped: false,
        invalidated: Some(invalidated.clone()),
    }));
    let (requests, incoming): (ContextRequests, _) = async_channel::bounded(1);
    tokio::spawn(async move {
        let Ok(ContextRequest::Model(reply)) = incoming.recv().await else {
            return;
        };
        invalidated.store(true, Ordering::SeqCst);
        let _ = reply.send((
            agent::CurrentCanvasContext {
                project_name: "切换后的项目".into(),
                workspace_root: "/project".into(),
                total_cards: 0,
                cards: vec![],
                conversation_tails: vec![],
            },
            vec![],
            None,
        ));
    });
    let mut history = vec![json!({"role":"user","content":"当前目标是什么"})];
    let result =
        request_with_shared_context(&requests, "", &mut history, Some(&runtime), &turn(), false)
            .await;
    assert!(
        result.is_err(),
        "等待环境时 scope 已失效，最后的来源检查必须拒绝请求"
    );
}
