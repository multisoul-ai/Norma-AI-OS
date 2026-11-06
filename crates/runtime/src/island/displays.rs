//! One expanded surface across all displays; activity data and preferences are shared.
use super::{IslandEvent, IslandMode, IslandState};

#[derive(Default)]
pub struct IslandDisplays {
    pub state: IslandState,
    active: Option<u32>,
}

impl IslandDisplays {
    pub fn active_display(&self) -> Option<u32> {
        self.active
    }

    pub fn mode_on(&self, display: u32) -> IslandMode {
        match self.state.mode() {
            IslandMode::Expanded | IslandMode::Alert if self.active != Some(display) => {
                IslandMode::Compact
            }
            mode => mode,
        }
    }

    pub fn apply(&mut self, display: u32, event: IslandEvent, now_ms: u64) -> bool {
        let before = self.active;
        if matches!(
            event,
            IslandEvent::PointerLeft | IslandEvent::InputFocusChanged(_)
        ) && self.active != Some(display)
        {
            return false;
        }
        if matches!(
            event,
            IslandEvent::PointerEntered | IslandEvent::Expand | IslandEvent::ToggleExpanded
        ) {
            self.active = Some(display);
        }
        if matches!(
            self.state.mode(),
            IslandMode::Compact | IslandMode::Activity
        ) && matches!(event, IslandEvent::OverviewUpdated(_))
        {
            self.active = Some(display);
        }
        if before != self.active {
            self.state
                .apply(IslandEvent::InputFocusChanged(false), now_ms);
        }
        let event = if before != self.active
            && self.state.mode() == IslandMode::Expanded
            && matches!(event, IslandEvent::ToggleExpanded)
        {
            IslandEvent::Expand
        } else {
            event
        };
        let changed = self.state.apply(event, now_ms);
        changed || before != self.active
    }

    pub fn retain_displays(&mut self, displays: &[u32], now_ms: u64) {
        if self.active.is_none_or(|active| !displays.contains(&active)) {
            self.state.apply(IslandEvent::PointerLeft, now_ms);
            self.state
                .apply(IslandEvent::InputFocusChanged(false), now_ms);
            self.active = displays.first().copied();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_displays.rs"]
mod tests;
