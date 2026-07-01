//! Apply compiled voice commands to the live runtime.

use crate::app::MultisoulApp;
use crate::norma::server::{execute_card_command_for_area, NormaCommandEffects, NormaExecution};
use gpui::Window;
use norma::protocol::{CardCommand, CardCommandResult};
use std::collections::HashMap;
use voice::ExecutableCommand;

impl MultisoulApp {
    pub(crate) fn execute_compiled_voice_plan(
        &mut self,
        commands: Vec<ExecutableCommand>,
        window: &mut Window,
    ) -> Result<String, String> {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let mut created_by_step = HashMap::new();
        let mut last_message = "Voice command executed".to_string();

        for (step, command) in commands.into_iter().enumerate() {
            let card_command = match command {
                ExecutableCommand::Card(command) => command,
                ExecutableCommand::SendTextToPrevious {
                    previous_step,
                    text,
                    submit,
                } => {
                    let node_id = *created_by_step
                        .get(&previous_step)
                        .ok_or_else(|| format!("missing created node for step {previous_step}"))?;
                    CardCommand::SendText {
                        node_id,
                        text,
                        submit,
                    }
                }
                ExecutableCommand::FocusPrevious { previous_step } => {
                    let node_id = *created_by_step
                        .get(&previous_step)
                        .ok_or_else(|| format!("missing created node for step {previous_step}"))?;
                    CardCommand::Focus { node_id }
                }
            };
            let execution = execute_card_command_for_area(
                &mut self.runtime,
                &mut self.motion,
                &mut self.master_layout,
                card_command,
                area,
                zoom,
            )
            .map_err(|error| error.to_string())?;
            if let CardCommandResult::Created { node_id } = &execution.result {
                created_by_step.insert(step, *node_id);
                last_message = format!("Created card {node_id}");
            }
            self.apply_voice_execution_effects(execution);
        }

        Ok(last_message)
    }

    fn apply_voice_execution_effects(&mut self, execution: NormaExecution) {
        let NormaCommandEffects {
            canvas_dirty,
            session_dirty,
            notify: _,
        } = execution.effects;
        if canvas_dirty {
            self.mark_canvas_dirty();
        }
        if session_dirty {
            self.mark_session_dirty();
        }
    }
}
