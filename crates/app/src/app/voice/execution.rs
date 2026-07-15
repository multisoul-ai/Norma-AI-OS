//! Apply compiled voice commands to the live runtime.

use crate::app::MultisoulApp;
use crate::norma::server::{NormaCommandEffects, NormaExecution};

impl MultisoulApp {
    pub(crate) fn apply_voice_execution_effects(&mut self, execution: NormaExecution) {
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
