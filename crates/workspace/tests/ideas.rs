use workspace::ideas::{
    normalize_body, transition, IdeaError, IdeaEvent, IdeaState, MAX_IDEA_CHARS,
};

/// 想法正文入库前必须规整：去首尾空白、拒绝空白、超长拒绝而不是静默截断。
///
/// 数据构造：带空白的正常文本；纯空白；恰好 500 字符；501 字符（用中文字符，验证按字符而不是字节计数）。
/// 执行过程：逐个调用 normalize_body。
/// 预期结果：
///   - 正断言：正常文本被 trim；500 字符通过。
///   - 负断言：空白报 Empty；501 字符报 TooLong，不返回被截断的文本。
#[test]
fn body_is_trimmed_and_bounded_without_silent_truncation() {
    assert_eq!(
        normalize_body("  给设置页加深色主题\n").as_deref(),
        Ok("给设置页加深色主题"),
        "首尾空白应被去掉，正文保持原样"
    );
    assert_eq!(
        normalize_body(" \n\t "),
        Err(IdeaError::Empty),
        "纯空白不应生成想法"
    );
    let at_limit = "想".repeat(MAX_IDEA_CHARS);
    assert_eq!(
        normalize_body(&at_limit).as_deref(),
        Ok(at_limit.as_str()),
        "恰好 500 个字符应通过（按字符而非 1500 字节计数）"
    );
    let over = "想".repeat(MAX_IDEA_CHARS + 1);
    assert_eq!(
        normalize_body(&over),
        Err(IdeaError::TooLong),
        "501 个字符应被拒绝，而不是被截断后接受"
    );
}

/// 派发主路径：待办 → 派发中 → 已指派 → 已完成。
///
/// 数据构造：从 Todo 起依次施加 BeginDispatch、DispatchSucceeded、Complete。
/// 预期结果：
///   - 正断言：状态依次为 Dispatching、Assigned、Done。
///   - 负断言：Done 之后任何事件都不再改变状态。
#[test]
fn dispatch_happy_path_reaches_done_and_done_is_terminal() {
    let dispatching =
        transition(IdeaState::Todo, IdeaEvent::BeginDispatch).expect("待办应可开始派发");
    assert_eq!(
        dispatching,
        IdeaState::Dispatching,
        "开始派发后应处于派发中"
    );
    let assigned = transition(dispatching, IdeaEvent::DispatchSucceeded).expect("派发中应可成功");
    assert_eq!(assigned, IdeaState::Assigned, "派发成功后应为已指派");
    let done = transition(assigned, IdeaEvent::Complete).expect("已指派应可完成");
    assert_eq!(done, IdeaState::Done, "任务完成后应为已完成");
    for event in [
        IdeaEvent::BeginDispatch,
        IdeaEvent::Retract,
        IdeaEvent::Dismiss,
        IdeaEvent::Restore,
    ] {
        assert_eq!(
            transition(IdeaState::Done, event),
            Err(IdeaError::InvalidTransition),
            "已完成是终态，不应响应 {event:?}"
        );
    }
}

/// 失败回退与重复派发保护。
///
/// 数据构造：派发中失败；已指派后撤回（任务取消/失败重试）；对非待办状态再次开始派发。
/// 预期结果：
///   - 正断言：派发失败与撤回都回到 Todo。
///   - 负断言：Dispatching/Assigned 上再次 BeginDispatch 被拒绝（双击只成功一次的领域依据）。
#[test]
fn failure_and_retract_return_to_todo_but_double_dispatch_is_rejected() {
    assert_eq!(
        transition(IdeaState::Dispatching, IdeaEvent::DispatchFailed),
        Ok(IdeaState::Todo),
        "派发失败应回到待办"
    );
    assert_eq!(
        transition(IdeaState::Assigned, IdeaEvent::Retract),
        Ok(IdeaState::Todo),
        "取消或重试时已指派应可撤回到待办"
    );
    for state in [IdeaState::Dispatching, IdeaState::Assigned] {
        assert_eq!(
            transition(state, IdeaEvent::BeginDispatch),
            Err(IdeaError::InvalidTransition),
            "{state:?} 不能再次开始派发"
        );
    }
}

/// 删除与撤销：只有待办能删除，删除后可恢复；派发中的想法不能被删除。
///
/// 预期结果：
///   - 正断言：Todo → Dismissed → Todo。
///   - 负断言：Dispatching 与 Assigned 上 Dismiss 被拒绝；Todo 上 Restore 被拒绝。
#[test]
fn only_todo_can_be_dismissed_and_dismissed_can_be_restored() {
    assert_eq!(
        transition(IdeaState::Todo, IdeaEvent::Dismiss),
        Ok(IdeaState::Dismissed),
        "待办应可删除"
    );
    assert_eq!(
        transition(IdeaState::Dismissed, IdeaEvent::Restore),
        Ok(IdeaState::Todo),
        "已删除应可撤销回待办"
    );
    for state in [IdeaState::Dispatching, IdeaState::Assigned] {
        assert_eq!(
            transition(state, IdeaEvent::Dismiss),
            Err(IdeaError::InvalidTransition),
            "{state:?} 正在或已经派发，不能被直接删除"
        );
    }
    assert_eq!(
        transition(IdeaState::Todo, IdeaEvent::Restore),
        Err(IdeaError::InvalidTransition),
        "待办没有可恢复的删除"
    );
}

/// 状态与来源在存储中的文本编码必须稳定且可往返。
///
/// 预期结果：
///   - 正断言：所有状态 as_str → parse 往返相等，编码为固定字面量。
///   - 负断言：未知文本解析为 None，而不是默认某个状态。
#[test]
fn state_text_encoding_round_trips_and_rejects_unknown() {
    let expected = [
        (IdeaState::Todo, "todo"),
        (IdeaState::Dispatching, "dispatching"),
        (IdeaState::Assigned, "assigned"),
        (IdeaState::Done, "done"),
        (IdeaState::Dismissed, "dismissed"),
    ];
    for (state, text) in expected {
        assert_eq!(state.as_str(), text, "{state:?} 的存储编码应固定");
        assert_eq!(
            IdeaState::parse(text),
            Some(state),
            "{text} 应能解析回 {state:?}"
        );
    }
    assert_eq!(
        IdeaState::parse("unknown"),
        None,
        "未知状态不应被静默当作任何一种"
    );
}
