fn start_event_pump(
    events: async_channel::Receiver<AgentEvent>,
    runtime_id: u64,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn(async move |this, cx| {
        while let Ok(event) = events.recv().await {
            diagnostics::event_receive(runtime_id, &event);
            let _ = this.update(cx, |app, cx| {
                if !is_current_runtime(app, runtime_id) {
                    return;
                }
                match event {
                    AgentEvent::DialogueTextDelta { text } => {
                        app.dialogue.on_agent_text_delta(&text, cx);
                    }
                    AgentEvent::DialogueStreamFinished => {
                        app.dialogue.finish_agent_text_stream(cx);
                    }
                    _ => {
                        if let Some(report) = terminal_task_report(&event) {
                            app.dialogue
                                .queue_agent_report_for_current_voice_source(report);
                        }
                    }
                }
                cx.notify();
            });
        }
    })
    .detach();
}

fn terminal_task_report(event: &AgentEvent) -> Option<String> {
    match event {
        AgentEvent::TaskSucceeded { summary, .. } => Some(summary.clone()),
        AgentEvent::TaskPartiallyDispatched { summary, .. } => Some(summary.clone()),
        AgentEvent::TaskFailed { reason, .. } => Some(format!("任务失败：{reason}")),
        AgentEvent::TaskCancelled {
            partial_effects: true,
            ..
        } => Some("任务已取消；已发生的操作不会回滚".to_string()),
        AgentEvent::TaskCancelled { .. } => Some("任务已取消".to_string()),
        AgentEvent::TaskInterrupted { .. } => Some("任务已中断".to_string()),
        _ => None,
    }
}

fn is_current_runtime(app: &MultisoulApp, runtime_id: u64) -> bool {
    app.agent_runtime
        .as_ref()
        .is_some_and(|runtime| runtime.id() == runtime_id)
}
