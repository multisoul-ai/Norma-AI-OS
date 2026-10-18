//! Visible turns from the current Norma session.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Speaker {
    User,
    Norma,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Message {
    pub(crate) speaker: Speaker,
    pub(crate) text: String,
}

pub(crate) struct AssistantConversation {
    messages: Vec<Message>,
    pub(crate) scroll: gpui::ScrollHandle,
}

impl Default for AssistantConversation {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            scroll: gpui::ScrollHandle::new(),
        }
    }
}

impl AssistantConversation {
    pub(crate) fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub(crate) fn user(&mut self, text: &str) {
        self.push(Speaker::User, text);
    }

    pub(crate) fn norma(&mut self, text: &str) {
        self.push(Speaker::Norma, text);
    }

    fn push(&mut self, speaker: Speaker, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let scroll = self.scroll.offset();
        let remaining = f32::from(self.scroll.max_offset().height) + f32::from(scroll.y);
        let follow = speaker == Speaker::User || remaining <= 48.0;
        self.messages.push(Message {
            speaker,
            text: text.to_owned(),
        });
        if follow {
            // The greeting is the first scroll item; new turns follow it.
            self.scroll.scroll_to_item(self.messages.len());
        }
    }
}

impl super::MultisoulApp {
    pub(crate) fn record_assistant_user_message(
        &mut self,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |app, cx| {
                app.assistant_conversation.user(text);
                cx.notify();
            });
        } else {
            self.assistant_conversation.user(text);
        }
    }

    pub(crate) fn record_assistant_norma_message(
        &mut self,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |app, cx| {
                app.assistant_conversation.norma(text);
                cx.notify();
            });
        } else {
            self.assistant_conversation.norma(text);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/assistant_conversation_tests.rs"]
mod tests;
