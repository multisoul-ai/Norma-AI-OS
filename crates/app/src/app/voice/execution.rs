//! Apply compiled voice commands to the live runtime.

use crate::app::MultisoulApp;
use crate::norma::server::{NormaCommandEffects, NormaExecution};

impl MultisoulApp {
    pub(crate) fn apply_voice_execution_effects(&mut self, execution: NormaExecution) {
        let NormaExecution {
            result: _,
            effects:
                NormaCommandEffects {
                    canvas_dirty,
                    session_dirty,
                    notify: _,
                },
            removed_agent_definition_id,
        } = execution;
        if canvas_dirty {
            self.mark_canvas_dirty();
        }
        if session_dirty {
            self.mark_session_dirty();
        }
        if let Some(definition_id) = removed_agent_definition_id {
            if let Err(message) = self
                .agent_definitions
                .cleanup_quick_definition(definition_id)
            {
                self.workspace_state.error = Some(
                    ui::i18n::LocalizedText::new("common.operation_failed")
                        .with_arg("reason", message),
                );
            }
        }
    }
}
