use crate::app::MultisoulApp;
use gpui::{Context, Window};

pub(super) fn start_direct_agent_task(
    app: &mut MultisoulApp,
    raw_transcript: String,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    app.voice.set_planning(raw_transcript.clone());
    let response = match app.submit_project_agent(
        agent::AgentCommand::DirectCardTask(raw_transcript),
        window,
        cx,
    ) {
        Ok(response) => response,
        Err(error) => {
            app.voice
                .set_error(format!("Agent task setup failed: {error}"));
            cx.notify();
            return;
        }
    };
    let task = cx.spawn_in(window, async move |this, cx| {
        let result = response
            .recv()
            .await
            .map_err(|_| "agent response closed".to_string())
            .and_then(|result| result)
            .map(crate::app::agent_runtime::artifacts_from_agent_events);
        let _ = this.update_in(cx, |app, _window, cx| {
            app.voice.clear_speech_task();
            match result {
                Ok(artifacts) => app.voice.set_executed(artifacts.reply_text),
                Err(error) => app.voice.set_error(format!("Agent task failed: {error}")),
            }
            cx.notify();
        });
    });
    app.voice.set_speech_task(task);
    cx.notify();
}
