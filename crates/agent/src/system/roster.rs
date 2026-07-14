use super::AgentSystem;
use crate::{
    AgentCoordinationError, AgentProvider, AgentRosterEntry, AgentTaskCoordinator, CardTaskJournal,
    CardToolHost,
};

impl<D, C, H, J> AgentSystem<D, C, H, J>
where
    D: AgentProvider,
    C: AgentProvider,
    H: CardToolHost,
    J: CardTaskJournal,
{
    pub fn set_agent_roster(
        &mut self,
        roster: Vec<AgentRosterEntry>,
    ) -> Result<(), AgentCoordinationError> {
        AgentTaskCoordinator::new(roster.clone())?;
        self.agent_roster_prompt = crate::render_agent_roster(&roster);
        self.agent_roster = roster;
        Ok(())
    }
}
