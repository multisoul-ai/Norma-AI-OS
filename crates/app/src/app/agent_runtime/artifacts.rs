use super::diagnostics;
use agent::AgentEvent;

pub(crate) fn artifacts_from_agent_events(
    events: Vec<AgentEvent>,
) -> crate::app::dialogue::VoiceTurnArtifacts {
    diagnostics::turn_events(&events);
    let mut reply = String::new();
    for event in events {
        match event {
            AgentEvent::DialogueTextDelta { text } => reply.push_str(&text),
            AgentEvent::DialogueStreamFinished => {}
            AgentEvent::TaskQueued { summary, .. } => reply = format!("任务处理中：{summary}"),
            AgentEvent::TaskSucceeded { summary, .. } => reply = summary,
            AgentEvent::TaskPartiallyDispatched { summary, .. } => reply = summary,
            AgentEvent::TaskFailed { reason, .. } => reply = format!("任务失败：{reason}"),
            AgentEvent::TaskCancelled { .. } => reply = "任务已取消".to_string(),
            AgentEvent::TaskInterrupted { .. } => reply = "任务已中断".to_string(),
            AgentEvent::TaskStatus {
                task_id,
                state,
                completed_tool_calls,
            } => {
                reply = format!(
                    "任务 #{task_id} 状态：{state}；已完成 {completed_tool_calls} 次工具调用"
                )
            }
            AgentEvent::ProviderError { message } => reply = format!("模型服务错误：{message}"),
            _ => {}
        }
    }
    let reply = strip_thinking_blocks(&reply);
    let reply = if reply.trim().is_empty() {
        diagnostics::empty_turn_fallback();
        "我没有收到有效回复，请再试一次。".to_string()
    } else {
        reply
    };
    crate::app::dialogue::VoiceTurnArtifacts {
        reply_text: reply,
        executed_commands: Vec::new(),
        template_reports: Vec::new(),
        queued_template_reports: Vec::new(),
    }
}

fn strip_thinking_blocks(text: &str) -> String {
    let mut visible = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(start) = remaining.find("<think>") {
        visible.push_str(&remaining[..start]);
        let after_start = &remaining[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            return visible.trim().to_string();
        };
        remaining = &after_start[end + "</think>".len()..];
    }
    visible.push_str(remaining);
    visible.trim().to_string()
}
