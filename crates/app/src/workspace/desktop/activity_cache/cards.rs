use norma::mobile::{CardSnapshot, DesktopChatAgent};

pub(super) fn chat_agent(name: String, card: CardSnapshot) -> DesktopChatAgent {
    let profile = card.profile.clone();
    DesktopChatAgent {
        definition_id: 0,
        name,
        profile,
        card,
    }
}

pub(super) fn card_from_status(
    agent: &storage::DesktopAgentStatus,
    snapshot: Option<&CardSnapshot>,
) -> CardSnapshot {
    let mut card = snapshot.cloned().unwrap_or(CardSnapshot {
        node_id: agent.node_id,
        profile: "Agent".into(),
        title: agent.name.clone(),
        mobile_control: false,
        state: agent.state.clone(),
        run_id: agent.run_id.clone(),
        session_id: None,
        turn_id: agent.turn_id.clone(),
        task_title: agent.task.clone(),
        latest_prompt: None,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        last_tool: None,
    });
    card.state = agent.state.clone();
    card.task_title = agent.task.clone();
    card.run_id = agent.run_id.clone();
    card.turn_id = agent.turn_id.clone();
    card
}
