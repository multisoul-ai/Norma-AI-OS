//! Project subscriptions to the single App-owned service.
use super::{owner, BrainHostState, MultisoulApp};
use gpui::Context;

impl BrainHostState {
    pub(super) fn is_running(&self) -> bool {
        self.client
            .as_ref()
            .is_some_and(owner::BrainClient::is_running)
    }

    pub(super) fn token(&self) -> Option<String> {
        self.client.as_ref().and_then(owner::BrainClient::token)
    }
}

impl MultisoulApp {
    pub(crate) fn ensure_brain_client(&mut self, cx: &mut Context<Self>) {
        let Some((scope, root)) = self.brain_scope().zip(self.runtime.workspace_root()) else {
            self.release_brain_client();
            return;
        };
        if self
            .brain
            .client
            .as_ref()
            .is_some_and(|client| client.matches_scope(&scope))
        {
            return;
        }
        let root = root.to_path_buf();
        self.release_brain_client();
        self.brain.client = Some(owner::shared(cx).register(scope, root));
    }

    pub(crate) fn release_brain_client(&mut self) {
        for activity in self.staff.activity.values_mut() {
            activity.interrupt();
        }
        self.brain.client = None;
        self.brain.active_generation = None;
        self.brain.tracked.clear();
        self.brain.last_turn.clear();
    }

    pub(super) fn sync_brain_client(&mut self, cx: &mut Context<Self>) {
        self.ensure_brain_client(cx);
        let owner = owner::shared(cx);
        if let Some(settings) = owner.settings() {
            self.brain.settings = Some(settings);
        }
        let generation = self
            .brain
            .client
            .as_ref()
            .and_then(owner::BrainClient::generation);
        if self.brain.active_generation != generation {
            if self.brain.active_generation.is_some() {
                for activity in self.staff.activity.values_mut() {
                    activity.interrupt();
                }
            }
            self.brain.active_generation = generation;
            self.brain.tracked.clear();
            self.brain.last_turn.clear();
            if generation.is_some() {
                if ["island.staff_loading", "island.staff_disabled"]
                    .iter()
                    .any(|key| {
                        self.staff.error.as_ref() == Some(&ui::i18n::LocalizedText::new(key))
                    })
                {
                    self.staff.error = None;
                }
                self.load_staff_tasks(cx);
            }
            self.notify_staff(cx);
        }
        if generation.is_some() {
            let pending: Vec<_> = self.brain.pending_events.borrow_mut().drain(..).collect();
            for event in pending {
                self.push_scoped_brain(event);
            }
        }
    }
}
