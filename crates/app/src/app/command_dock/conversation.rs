use crate::app::{agent_runtime::artifacts_from_agent_events, MultisoulApp};
use gpui::{Context, Window};

impl MultisoulApp {
    pub(super) fn submit_dock_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self
            .command_dock
            .as_ref()
            .filter(|state| !state.pending_reply)
        else {
            return;
        };
        let input = state.draft.trim().to_owned();
        if input.is_empty() {
            return;
        }
        let response = match self.submit_project_agent(
            agent::AgentCommand::UserTurn {
                input,
                input_source: agent::AgentInputSource::Other,
            },
            window,
            cx,
        ) {
            Ok(response) => response,
            Err(error) => {
                if let Some(state) = &mut self.command_dock {
                    state.error = Some(format!(
                        "请在 Norma → 语音 → AI Connection 中完成模型配置：{error}"
                    ));
                }
                cx.notify();
                return;
            }
        };
        let state = self
            .command_dock
            .as_mut()
            .expect("conversation starts from the Dock");
        state.reply_generation += 1;
        let generation = state.reply_generation;
        state.pending_reply = true;
        state.reply = Some("Norma 正在思考…".into());
        state.error = None;
        let root = self.workspace_state.current_root.clone();
        cx.spawn(async move |this, cx| {
            let result = response
                .recv()
                .await
                .map_err(|_| "Norma 回复已中断，请重试".to_owned())
                .and_then(|result| result)
                .map(|events| artifacts_from_agent_events(events).reply_text);
            let _ = this.update(cx, |app, cx| {
                if app.workspace_state.current_root != root {
                    return;
                }
                let Some(state) = app
                    .command_dock
                    .as_mut()
                    .filter(|state| state.reply_generation == generation)
                else {
                    return;
                };
                state.pending_reply = false;
                match result {
                    Ok(reply) => state.reply = Some(reply),
                    Err(error) => {
                        state.reply = None;
                        state.error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
