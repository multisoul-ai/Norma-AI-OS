use super::{AppScreen, MultisoulApp, NewTerminal};
use gpui::{Context, Window};
use runtime::{InputMode, NodeInput};
use std::path::PathBuf;

const SCENARIO_ENV: &str = "NORMA_CPU_BENCHMARK_SCENARIO";
const WORKSPACE_ENV: &str = "NORMA_CPU_BENCHMARK_WORKSPACE";
const ACTIVE_TERMINAL_COMMAND: &str = r#"i=0; while test $i -lt 4000; do printf "\033[2J\033[Hsynthetic frame %s\n" "$i"; i=$((i+1)); sleep 0.02; done"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CpuBenchmarkScenario {
    IdleSevenCards,
    BackgroundSevenCards,
    OneActiveTerminal,
    TwoActiveTerminals,
}

impl CpuBenchmarkScenario {
    pub(super) fn card_counts(self) -> (usize, usize) {
        match self {
            Self::IdleSevenCards | Self::BackgroundSevenCards => (7, 0),
            Self::OneActiveTerminal => (6, 1),
            Self::TwoActiveTerminals => (5, 2),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::IdleSevenCards => "idle-seven-cards",
            Self::BackgroundSevenCards => "background-seven-cards",
            Self::OneActiveTerminal => "one-active-terminal",
            Self::TwoActiveTerminals => "two-active-terminals",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CpuBenchmarkConfig {
    pub(super) scenario: CpuBenchmarkScenario,
    pub(super) workspace: PathBuf,
}

impl CpuBenchmarkConfig {
    pub(super) fn from_values(
        scenario: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let Some(scenario) = scenario.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let scenario = match scenario {
            "idle-seven-cards" => CpuBenchmarkScenario::IdleSevenCards,
            "background-seven-cards" => CpuBenchmarkScenario::BackgroundSevenCards,
            "one-active-terminal" => CpuBenchmarkScenario::OneActiveTerminal,
            "two-active-terminals" => CpuBenchmarkScenario::TwoActiveTerminals,
            _ => return Err(format!("unknown CPU benchmark scenario: {scenario}")),
        };
        let workspace = workspace
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "CPU benchmark workspace ({WORKSPACE_ENV}) is required when {SCENARIO_ENV} is set"
                )
            })?;
        Ok(Some(Self {
            scenario,
            workspace: PathBuf::from(workspace),
        }))
    }

    fn from_environment() -> Result<Option<Self>, String> {
        let scenario = std::env::var(SCENARIO_ENV).ok();
        let workspace = std::env::var(WORKSPACE_ENV).ok();
        Self::from_values(scenario.as_deref(), workspace.as_deref())
    }
}

impl MultisoulApp {
    pub(crate) fn configure_cpu_benchmark_from_environment(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = match CpuBenchmarkConfig::from_environment() {
            Ok(Some(config)) => config,
            Ok(None) => return,
            Err(error) => {
                tracing::error!(%error, "cpu_benchmark_configuration_failed");
                return;
            }
        };

        self.open_workspace_path(config.workspace.clone(), window, cx);
        if !matches!(self.screen, AppScreen::Canvas) {
            tracing::error!(
                scenario = config.scenario.name(),
                workspace = %config.workspace.display(),
                "cpu_benchmark_workspace_failed"
            );
            return;
        }

        let (idle_terminal_count, active_terminal_count) = config.scenario.card_counts();
        for _ in 0..idle_terminal_count {
            self.on_new_terminal(&NewTerminal, window, cx);
        }
        for _ in 0..active_terminal_count {
            self.on_new_terminal(&NewTerminal, window, cx);
            let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() else {
                tracing::error!("cpu_benchmark_active_terminal_focus_failed");
                continue;
            };
            let command = format!("{ACTIVE_TERMINAL_COMMAND}\r");
            if let Err(error) = self
                .runtime
                .send_node_input(node_id, NodeInput::Text(command))
            {
                tracing::error!(%error, node_id, "cpu_benchmark_active_terminal_start_failed");
            }
        }

        tracing::info!(
            target: "multisoul_frame",
            scenario = config.scenario.name(),
            workspace = %config.workspace.display(),
            node_count = self.runtime.document.nodes().len(),
            active_agent_count = 0,
            active_terminal_count,
            "cpu_benchmark_ready"
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cpu_benchmark_tests.rs"]
mod tests;
