//! Script-free host fixture; all subscriptions and authorization use the production owner.
use super::*;
use crate::app::{brain_host::BrainHostState, MultisoulApp};

impl BrainHostState {
    pub(in crate::app::brain_host) fn install_brain_host_for_test(&mut self, host: BrainHost) {
        let owner = self
            .client
            .as_ref()
            .map(|client| client.owner.clone())
            .unwrap_or_default();
        owner.configure(
            config::BrainSettings {
                enabled: true,
                ..Default::default()
            },
            false,
        );
        if self.client.is_none() {
            self.client = Some(owner.register(
                BrainScope {
                    workspace_id: "test".into(),
                    route_id: "route".into(),
                },
                "/fixture".into(),
            ));
        }
        owner.0.borrow_mut().invalidate();
        let ticket = owner
            .begin_start(Instant::now())
            .expect("fixture has no running process");
        owner.complete_start(ticket, Ok(host));
    }

    pub(in crate::app::brain_host) fn retire_brain_for_test(&mut self) {
        self.client
            .as_ref()
            .expect("fixture installed")
            .owner
            .0
            .borrow_mut()
            .invalidate();
        self.last_turn.clear();
    }

    pub(in crate::app::brain_host) fn retired_brain_empty_for_test(&self) -> bool {
        self.client
            .as_ref()
            .expect("fixture installed")
            .owner
            .0
            .borrow()
            .retired
            .is_empty()
    }
}

impl MultisoulApp {
    pub(in crate::app::brain_host) fn install_brain_host_for_test(&mut self, host: BrainHost) {
        if self.brain.client.is_none() {
            let owner = SharedBrain::default();
            let scope = self.brain_scope().unwrap_or(BrainScope {
                workspace_id: "test".into(),
                route_id: "route".into(),
            });
            self.brain.client = Some(
                owner.register(
                    scope,
                    self.runtime
                        .workspace_root()
                        .map(std::path::Path::to_path_buf)
                        .unwrap_or_else(|| "/fixture".into()),
                ),
            );
        }
        self.brain.install_brain_host_for_test(host);
    }
}
