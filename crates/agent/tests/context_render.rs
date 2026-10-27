use agent::context::*;

/// 场景：固定状态提供来源小索引；无关报告正文和原始用户输入不复制到资料消息。
/// 构造：一条用户输入和未播出的报告，预算足以容纳固定资料。
/// 执行：生成供模型读取的共享资料。
/// 预期：保留当前输入引用与版本；没有把未播报告说成用户听过，也没有重复输入正文。
#[test]
fn rendered_frame_is_attributed_data_without_duplicating_user_input() {
    let scope = ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    };
    let input = ContextRef {
        scope: scope.clone(),
        event_id: "input".into(),
        revision: 1,
    };
    let report = ContextRef {
        scope: scope.clone(),
        event_id: "report".into(),
        revision: 2,
    };
    let mut state = WorkingState::default();
    state.reports.insert(
        "report".into(),
        ContextReport {
            reference: report.clone(),
            text: "无关但很长的汇报正文".into(),
            delivery: ReportDelivery::Skipped,
        },
    );
    let frame = ContextFrame {
        input: input.clone(),
        snapshot: ContextSnapshot {
            scope,
            revision: 2,
            state,
            events: vec![],
        },
    };
    let data = render_context_frame(
        &frame,
        ContextConsumer::Legacy,
        "t1",
        "只聊今天的天气",
        8192,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&data).unwrap();
    assert_eq!(
        value["current_input"],
        serde_json::to_value(input).unwrap(),
        "输入通过引用定位"
    );
    assert_eq!(
        value["working_state"][0]["recent_reports"][0]["delivery"], "skipped",
        "不能伪造已播放"
    );
    assert!(!data.contains("无关但很长"), "正文不能进入固定资料");
    assert!(!data.contains("只聊今天"), "原生用户消息不会复制进资料");
    assert!(
        render_context_frame(&frame, ContextConsumer::Legacy, "t1", "天气", 100).is_err(),
        "固定资料超预算明确失败"
    );
}

/// 场景：真实宿主快照包含刚保存的用户输入；输入预算已在供应商请求层计算。
/// 构造：2 KiB 的用户输入事件与同一 current_input 引用，资料只需不到 1 KiB。
/// 执行：在 1 KiB 资料预算内渲染。
/// 预期：不会重复拼接当前原话，也不会为不输出的原话重复计费。
#[test]
fn real_input_event_is_not_retrieved_or_budgeted_again() {
    let scope = ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    };
    let input = ContextRef {
        scope: scope.clone(),
        event_id: "input".into(),
        revision: 1,
    };
    let text = "q".repeat(2048);
    let record = RecordedContextEvent {
        reference: input.clone(),
        event: ContextEvent {
            event_id: "input".into(),
            scope: scope.clone(),
            source: ContextSource {
                origin: ContextOrigin::Voice,
                stream_id: "s".into(),
                sequence: 1,
            },
            created_at_ms: 1,
            related: vec![],
            payload: ContextPayload::UserInput { text: text.clone() },
        },
    };
    let frame = ContextFrame {
        input,
        snapshot: ContextSnapshot {
            scope,
            revision: 1,
            state: WorkingState::default(),
            events: vec![record],
        },
    };
    let data = render_context_frame(&frame, ContextConsumer::Legacy, "turn", &text, 1024)
        .expect("当前输入已经由外层计费，资料预算足够");
    assert!(!data.contains(&text), "真实当前输入事件不能再次命中检索");
    assert!(data.len() < 1024, "实际输出满足资料预算");
}
