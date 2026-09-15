use super::*;

impl Runtime {
    /// Pure check; the App applies its existing close/layout path immediately without yielding.
    pub fn validate_mobile_card_deletion(
        &self,
        expected: &norma::mobile::CardSnapshot,
        confirm_busy: bool,
    ) -> Result<(), &'static str> {
        let current = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == expected.node_id)
            .ok_or("not_found")?;
        if !matches!(current.profile.as_str(), "Codex" | "Claude" | "Cursor") {
            return Err("unsupported_profile");
        }
        if &current != expected {
            return Err("card_changed");
        }
        if matches!(current.state.as_str(), "preparing" | "unknown") {
            return Err("state_not_ready");
        }
        if matches!(
            current.state.as_str(),
            "thinking" | "running_tool" | "waiting_approval" | "interrupting"
        ) {
            if current.run_id.is_none() || current.session_id.is_none() || current.turn_id.is_none()
            {
                return Err("state_not_ready");
            }
            if !confirm_busy {
                return Err("confirmation_required");
            }
        }
        Ok(())
    }
}
