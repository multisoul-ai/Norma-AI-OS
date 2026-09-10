use norma::agent_hook::AgentHookEvent;
use norma::mobile::TaskTiming;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub(super) struct TaskClock {
    turn: Option<String>,
    started: Option<Instant>,
    timing: Option<TaskTiming>,
}

impl TaskClock {
    pub(super) fn snapshot(&self) -> Option<TaskTiming> {
        self.timing
    }

    pub(super) fn start(&mut self, turn: Option<&str>) {
        let Some(turn) = turn.filter(|turn| !turn.is_empty()) else {
            *self = Self::default();
            return;
        };
        if self.turn.as_deref() == Some(turn) {
            return;
        }
        let Ok(unix) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            *self = Self::default();
            return;
        };
        self.turn = Some(turn.into());
        self.started = Some(Instant::now());
        self.timing = Some(TaskTiming {
            started_at_unix_ms: unix.as_millis().try_into().unwrap_or(u64::MAX),
            completed_elapsed_ms: None,
        });
    }

    pub(super) fn finish(&mut self) {
        if let (Some(started), Some(timing)) = (self.started, &mut self.timing) {
            timing.completed_elapsed_ms.get_or_insert_with(|| {
                started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
            });
        }
    }

    // Called only after the existing run/session/turn gate accepts the Hook.
    pub(super) fn observe_hook(&mut self, event: &AgentHookEvent) {
        match event.provider_event.as_str() {
            "SessionStart" => *self = Self::default(),
            "UserPromptSubmit" => self.start(event.turn_id.as_deref()),
            "Stop" | "StopFailure" | "SessionEnd" => self.finish(),
            _ => {}
        }
    }
}
