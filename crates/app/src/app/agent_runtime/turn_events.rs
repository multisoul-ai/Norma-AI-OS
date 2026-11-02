use super::*;

pub(super) struct RuntimeEvent {
    pub event: AgentEvent,
    turn: Option<(u64, Arc<AtomicU64>)>,
    voice_generation: Option<u64>,
}
impl From<AgentEvent> for RuntimeEvent {
    fn from(event: AgentEvent) -> Self {
        Self {
            event,
            turn: None,
            voice_generation: None,
        }
    }
}
impl RuntimeEvent {
    pub fn for_turn(
        event: AgentEvent,
        turn: u64,
        current: Arc<AtomicU64>,
        source: AgentInputSource,
    ) -> Self {
        let voice_generation = match source {
            AgentInputSource::SingleVoiceCommand { generation } => Some(generation),
            _ => None,
        };
        Self {
            event,
            turn: Some((turn, current)),
            voice_generation,
        }
    }
    pub fn is_current(&self, voice_generation: Option<u64>) -> bool {
        self.turn
            .as_ref()
            .is_none_or(|(turn, current)| current.load(Ordering::Acquire) == *turn)
            && self.voice_generation.is_none_or(|generation| {
                voice_generation.is_none_or(|current| current == generation)
            })
    }
}
