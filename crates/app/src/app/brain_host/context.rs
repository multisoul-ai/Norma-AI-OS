use super::*;
use agent::context::ContextRef;
use norma::protocol::CardCommandError;

#[cfg(test)]
#[path = "../../../tests/unit/brain_context_guard_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_report_context_tests.rs"]
mod report_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_confirmation_context_tests.rs"]
mod confirmation_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_proposal_context_tests.rs"]
mod proposal_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_action_context_tests.rs"]
mod action_tests;

impl MultisoulApp {
    pub(crate) fn prepare_brain_context(
        &mut self,
        token: &str,
    ) -> Result<crate::app::shared_context::SharedContext, CardCommandError> {
        if !self.brain.token_matches(token) {
            return Err(CardCommandError::Unsupported {
                message: "大脑令牌无效".into(),
            });
        }
        self.shared_context_service()
            .map_err(|error| ops::reject(&error.to_string()))
    }

    pub(crate) fn handoff_with_context(
        &mut self,
        request: String,
        reference: ContextRef,
    ) -> Result<agent::AdapterResult, agent::AdapterError> {
        if self.brain.host.is_none() {
            return Err(agent::AdapterError::Rejected(
                "幕僚没有运行，无法交接".into(),
            ));
        }
        let service = self.shared_context_service()?;
        if service.scope() != &reference.scope {
            return Err(agent::AdapterError::Rejected(
                "context_scope_mismatch".into(),
            ));
        }
        self.push_brain(BrainEvent::ContextRequest {
            text: request,
            reference,
        });
        Ok(agent::AdapterResult::accepted("已交给幕僚，结果会稍后播报"))
    }
}
