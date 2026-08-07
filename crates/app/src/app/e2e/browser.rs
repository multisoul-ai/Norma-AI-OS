use crate::app::browser::BrowserAuthUiState;
use crate::app::MultisoulApp;
use canvas::NodeId;

impl MultisoulApp {
    pub fn set_browser_auth_fake_success_for_e2e(&mut self, imported: usize, skipped: usize) {
        self.browsers
            .set_chrome_auth_fake_result(Some(Ok((imported, skipped))));
    }

    pub fn set_browser_auth_fake_failure_for_e2e(&mut self, message: &str) {
        self.browsers
            .set_chrome_auth_fake_result(Some(Err(message.to_string())));
    }

    pub fn browser_auth_state_for_e2e(&self, node_id: NodeId) -> &'static str {
        match self.browsers.chrome_auth_state(node_id) {
            BrowserAuthUiState::Idle => "idle",
            BrowserAuthUiState::Waiting => "waiting",
            BrowserAuthUiState::Ready => "ready",
            BrowserAuthUiState::Failed => "failed",
        }
    }
}
