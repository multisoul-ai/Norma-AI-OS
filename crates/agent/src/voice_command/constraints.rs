pub(crate) fn execution_is_paused(input: &str) -> bool {
    let input = input.to_lowercase();
    [
        "先别执行",
        "不要执行",
        "别执行",
        "先不执行",
        "暂不执行",
        "先别发",
        "不要发送",
        "别发送",
        "先等一下",
        "先等等",
        "先暂停",
        "don't execute",
        "do not execute",
        "do not send",
        "don't send",
        "hold off",
        "not yet",
    ]
    .iter()
    .any(|marker| input.contains(marker))
}

/// Decide whether this user turn requests an effect. Model wording is never an execution receipt.
pub(crate) fn requires_action_receipt(input: &str, roster: &[crate::AgentRosterEntry]) -> bool {
    let lower = input.to_lowercase();
    let explicit_effect = [
        "新建", "创建", "派发", "交给", "发给", "发送", "取消", "撤回", "删除", "create ",
        "assign ", "send ", "cancel ", "delete ",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let explanation = [
        "什么意思",
        "是什么意思",
        "解释",
        "含义",
        "what does",
        "meaning of",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let directed_request = ["帮我", "请", "让", "ask ", "please "]
        .iter()
        .any(|marker| lower.contains(marker));
    let status_topic = ["状态", "进展", "进度", "完成", "做完", "status", "progress"]
        .iter()
        .any(|marker| lower.contains(marker));
    let status_question = status_topic
        && ["吗", "？", "?", "如何", "怎么样", "是否", "what ", "how "]
            .iter()
            .any(|marker| lower.contains(marker));
    // A task's name may itself contain an action verb. An explicit status question
    // is still a query; requesting someone to check it is a delegated action.
    if status_question && !directed_request {
        return false;
    }
    if explicit_effect {
        return true;
    }
    // Explanations in task content do not override an explicit effect request.
    if explanation {
        return false;
    }
    let target = super::resolve_voice_target(input, roster);
    if target == super::VoiceTargetResolution::Assistant {
        return false;
    }
    let mutation = [
        "修复",
        "实现",
        "重构",
        "继续",
        "执行",
        "补充",
        "fix ",
        "implement ",
        "continue ",
        "run ",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let review = ["检查", "测试", "整理", "回复", "check ", "review ", "test "]
        .iter()
        .any(|marker| lower.contains(marker));
    let named = matches!(
        target,
        super::VoiceTargetResolution::CanvasAgent { .. }
            | super::VoiceTargetResolution::OutsideCanvas { .. }
            | super::VoiceTargetResolution::Ambiguous
    );
    // Once an executor is named, require a receipt by default. This must not
    // depend on enumerating every possible task verb (optimize, summarize, ...).
    named || mutation || (review && directed_request)
}
