//! Agent card creation actions for toolbar, menu, and shortcuts.

use super::{
    AppScreen, MultisoulApp, NewClaudeAgent, NewCodexAgent, NewCursorAgent, NewGrokAgent,
    NewOpenCodeAgent,
};
use crate::canvas::master_layout::term_grid_size_for_card;
use crate::canvas::spawn_focus;
use canvas::{AgentProfile, AgentSpec, NodeType};
use gpui::{Context, Window};
use std::path::Path;
use storage::AgentDefinition;

pub(crate) fn agent_spec_from_definition(definition: &AgentDefinition) -> AgentSpec {
    AgentSpec {
        profile: definition.runtime_profile,
        alias: Some(definition.name.clone()),
        definition_id: Some(definition.id),
        mobile_control: false,
    }
}

impl MultisoulApp {
    pub(crate) fn on_new_claude_agent(
        &mut self,
        _: &NewClaudeAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("agent.open_project_first"));
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Claude, window, cx);
    }

    pub(crate) fn on_new_codex_agent(
        &mut self,
        _: &NewCodexAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("agent.open_project_first"));
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Codex, window, cx);
    }

    pub(crate) fn on_new_opencode_agent(
        &mut self,
        _: &NewOpenCodeAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("agent.open_project_first"));
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::OpenCode, window, cx);
    }

    pub(crate) fn on_new_grok_agent(
        &mut self,
        _: &NewGrokAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("agent.open_project_first"));
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Grok, window, cx);
    }

    pub(crate) fn on_new_cursor_agent(
        &mut self,
        _: &NewCursorAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("agent.open_project_first"));
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Cursor, window, cx);
    }

    pub(crate) fn spawn_agent_card(
        &mut self,
        profile: AgentProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            let source = cx.entity().entity_id();
            host.update(cx, |host, cx| {
                host.begin_desktop_agent_creation(source, profile, None, window, cx)
            });
            return;
        }
        match self.agent_definitions.create_quick_definition(profile) {
            Ok(definition) => {
                self.spawn_agent_card_from_spec(agent_spec_from_definition(&definition), window, cx)
            }
            Err(message) => {
                self.workspace_state.error = Some(
                    ui::i18n::LocalizedText::new("common.operation_failed")
                        .with_arg("reason", message),
                );
                cx.notify();
            }
        }
    }

    pub(crate) fn spawn_agent_card_from_definition(
        &mut self,
        definition_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            let Some(definition) = self
                .agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == definition_id)
            else {
                self.workspace_state.error =
                    Some(ui::i18n::LocalizedText::new("agent.definition_missing"));
                cx.notify();
                return;
            };
            let profile = definition.runtime_profile;
            let source = cx.entity().entity_id();
            host.update(cx, |host, cx| {
                host.begin_desktop_agent_creation(source, profile, Some(definition_id), window, cx)
            });
            return;
        }
        let Some(definition) = self
            .agent_definitions
            .definitions()
            .iter()
            .find(|definition| definition.id == definition_id)
            .cloned()
        else {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("agent.definition_missing"));
            cx.notify();
            return;
        };
        let spec = agent_spec_from_definition(&definition);
        self.spawn_agent_card_from_spec(spec, window, cx);
    }

    fn spawn_agent_card_from_spec(
        &mut self,
        spec: AgentSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_agent_card_from_spec_at(spec, None, window, cx);
    }

    pub(crate) fn spawn_desktop_agent_at(
        &mut self,
        profile: AgentProfile,
        definition_id: Option<i64>,
        worktree: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Desktop areas share the catalog, but keep independent loaded instances.
        self.agent_definitions.reload();
        let definition = match definition_id {
            Some(id) => self
                .agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == id)
                .cloned()
                .ok_or_else(|| ui::i18n::text("agent.definition_missing").to_string()),
            None => self.agent_definitions.create_quick_definition(profile),
        };
        match definition {
            Ok(definition) => self.spawn_agent_card_from_spec_at(
                agent_spec_from_definition(&definition),
                Some(worktree),
                window,
                cx,
            ),
            Err(error) => {
                self.workspace_state.error = Some(
                    ui::i18n::LocalizedText::new("common.operation_failed")
                        .with_arg("reason", error),
                );
                cx.notify();
            }
        }
    }

    fn spawn_agent_card_from_spec_at(
        &mut self,
        spec: AgentSpec,
        worktree: Option<&Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.next_tiled_node_bounds(window);
        let (cols, rows) =
            term_grid_size_for_card(NodeType::Agent, bounds, self.runtime.document.viewport.zoom);
        let profile = spec.profile;
        let spawn = match (spec.definition_id, spec.alias) {
            (Some(definition_id), Some(alias)) => {
                self.runtime.spawn_agent_for_definition_in_worktree(
                    profile,
                    definition_id,
                    alias,
                    bounds,
                    (cols, rows),
                    worktree,
                )
            }
            (_, alias) => match worktree {
                Some(worktree) => self
                    .runtime
                    .spawn_agent_with_worktree(profile, alias, bounds, cols, rows, worktree),
                None => self
                    .runtime
                    .spawn_agent_with_alias(profile, alias, bounds, cols, rows),
            },
        };
        match spawn {
            Ok(node_id) => {
                tracing::info!(?node_id, ?profile, "spawned agent node");
                spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
                self.motion.spawn_node(node_id, bounds);
                self.motion.start_initialization(
                    node_id,
                    crate::motion::InitializationAnimationKind::for_agent_profile(profile),
                );
                self.apply_layout_for_new_node(node_id, window);
                self.mark_canvas_dirty();
                self.mark_session_dirty();
            }
            Err(e) => {
                tracing::error!(error = %e, ?profile, "failed to spawn agent");
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agent definition 映射：定义名称应复制到 AgentSpec.alias，runtime profile 应原样保留。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   definition.id = 7，仅用于模拟已持久化 project agent
    ///   definition.name = "Code Reviewer"，应成为 AgentSpec.alias
    ///   definition.runtime_profile = Codex，应成为 AgentSpec.profile
    ///   workflow_description = "Review code."，Task 4 不应注入 AgentSpec
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造 storage::AgentDefinition
    ///   2. agent_spec_from_definition(&definition)
    ///   3. 读取 spec.profile 与 spec.alias
    ///
    /// 预期结果：
    ///   - 正断言：profile == Codex
    ///   - 正断言：alias == Some("Code Reviewer")
    ///   - 负断言：alias 不应退回默认 runtime 名称 "Codex"
    #[test]
    fn test_agent_spec_from_definition_maps_name_to_alias_and_profile() {
        let definition = AgentDefinition {
            id: 7,
            name: "Code Reviewer".to_string(),
            speech_alias: None,
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Review code.".to_string(),
            enabled: true,
            sort_order: 0,
            origin: storage::AgentDefinitionOrigin::Explicit,
        };

        let spec = agent_spec_from_definition(&definition);

        assert_eq!(
            spec.profile,
            AgentProfile::Codex,
            "AgentSpec.profile 应保留定义中的 runtime profile"
        );
        assert_eq!(
            spec.alias.as_deref(),
            Some("Code Reviewer"),
            "AgentSpec.alias 应复制定义名称，供卡片标题显示"
        );
        assert_ne!(
            spec.alias.as_deref(),
            Some("Codex"),
            "AgentSpec.alias 不应退回 runtime 默认名称，否则无法体现 project agent 配置"
        );
    }
}
