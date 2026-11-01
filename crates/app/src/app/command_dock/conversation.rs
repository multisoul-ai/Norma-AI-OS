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
        if let Some(ask) = super::ask::parse(&input) {
            self.submit_dock_ask(ask, window, cx);
            return;
        }
        // 普通对话直接交给幕僚；幕僚没有运行时退回 Norma 前台对话。
        if self.handoff_to_brain(input.clone()).is_ok() {
            self.finish_brain_handoff(window, cx);
            return;
        }
        let response = match self.submit_project_agent(
            agent::AgentCommand::UserTurn {
                input: input.clone(),
                input_source: agent::AgentInputSource::Other,
            },
            window,
            cx,
        ) {
            Ok(response) => response,
            Err(error) => {
                if let Some(state) = &mut self.command_dock {
                    state.error = Some(ui::i18n::message(
                        "chrome.configure_the_model_in_norma_voice_ai_connection_error",
                        &[("error", (error).to_string())],
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
        state.reply = Some(ui::i18n::LocalizedText::new("chrome.norma_is_thinking"));
        state.error = None;
        state.draft.clear();
        state.set_visible(false);
        let input_state = state.input.clone();
        input_state.update(cx, |input, cx| input.set_value(String::new(), window, cx));
        self.record_assistant_user_message(&input, cx);
        let root = self.workspace_state.current_root.clone();
        cx.spawn(async move |this, cx| {
            let result = response
                .recv()
                .await
                .map_err(|_| {
                    ui::i18n::LocalizedText::new("chrome.norma_s_reply_was_interrupted_try_again")
                })
                .and_then(|result| result.map_err(Into::into))
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
                let history_text = match result {
                    Ok(reply) => {
                        state.reply = Some(reply.clone().into());
                        reply
                    }
                    Err(error) => {
                        let text = error.to_string();
                        state.reply = None;
                        state.error = Some(error);
                        text
                    }
                };
                app.record_assistant_norma_message(&history_text, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_brain_handoff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.command_dock.as_mut() else {
            return;
        };
        state.reply = Some(ui::i18n::LocalizedText::new("chrome.brain_handed_off"));
        state.awaiting_brain_reply = true;
        state.error = None;
        state.draft.clear();
        state.set_visible(false);
        let input = state.input.clone();
        input.update(cx, |input, cx| input.set_value(String::new(), window, cx));
        cx.notify();
    }

    /// 幕僚的 report 到达：若是 Dock 交接后的第一条，显示为 Dock 回复（不依赖语音播报）。
    pub(crate) fn deliver_dock_brain_reply(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(state) = self
            .command_dock
            .as_mut()
            .filter(|state| state.awaiting_brain_reply)
        else {
            return false;
        };
        state.awaiting_brain_reply = false;
        state.reply = Some(text.to_owned().into());
        // 回复区只在 Dock 展开时绘制；交接时已收起且草稿为空，set_visible 会拒绝，故直接展开。
        state.visible = true;
        cx.notify();
        true
    }
}
