//! Exact Provider input retained for the current turn; never reconstruct a task from its title.
use super::*;

#[derive(Default)]
pub(super) struct RetryTask(Option<(String, String)>);
impl RetryTask {
    pub(super) fn record(&mut self, turn: Option<&str>, text: Option<&str>) {
        self.0 = turn
            .zip(text)
            .filter(|(_, text)| {
                !text.trim().is_empty()
                    && text.len() <= 32 * 1024
                    && !text
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
            })
            .map(|(turn, text)| (turn.into(), text.into()));
    }
    pub(super) fn observe_hook(&mut self, event: &norma::agent_hook::AgentHookEvent) {
        let text = ["prompt", "message", "user_message"]
            .iter()
            .find_map(|key| event.raw_json[*key].as_str());
        self.record(event.turn_id.as_deref(), text);
    }
}
impl Runtime {
    /// Read only. The existing send path still validates the card and confirms a new Provider turn.
    pub fn mobile_retry_text(
        &mut self,
        expected: &norma::mobile::CardSnapshot,
    ) -> Result<String, &'static str> {
        self.drain_mobile_codex();
        let actual = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == expected.node_id)
            .ok_or("target_missing")?;
        if actual.state != "failed"
            || actual.profile != expected.profile
            || actual.run_id != expected.run_id
            || actual.session_id != expected.session_id
            || actual.turn_id != expected.turn_id
            || actual.turn_id.is_none()
        {
            return Err("target_missing");
        }
        let run = self
            .mobile_runs
            .get(&expected.node_id)
            .ok_or("target_missing")?;
        let original = run
            .codex
            .as_ref()
            .map(|host| &host.retry_task)
            .unwrap_or(&run.retry_task);
        let (_, text) = original
            .0
            .as_ref()
            .filter(|(turn, _)| Some(turn) == expected.turn_id.as_ref())
            .ok_or("original_task_unavailable")?;
        Ok(text.clone())
    }
}
