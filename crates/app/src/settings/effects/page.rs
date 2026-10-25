//! 设置页一次渲染所需的数据快照：在 `MultisoulApp` 里组装，视图只读它。

use super::state::{EffectsSettingsState, EffectsTarget, Source};
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub(crate) struct EffectsPageData {
    pub state: EffectsSettingsState,
    pub target: Option<EffectsTarget>,
    pub workspace_name: Option<String>,
    pub project_name: Option<String>,
    pub can_edit_project: bool,
    /// 系统或环境要求减少动态效果时只暂停自动预览，不改变实际派发款式。
    pub reduce_motion: bool,
    pub hover: Option<usize>,
    pub hover_since: Option<Instant>,
    pub menu: Option<Source>,
    pub menu_cursor: usize,
    pub replay_started: Option<Instant>,
    pub run: super::ui_state::RunPreviewUi,
}

impl EffectsPageData {
    pub(crate) fn scope_label(&self) -> String {
        match (
            &self.workspace_name,
            &self.project_name,
            self.state.edit_project,
        ) {
            (Some(workspace), Some(project), true) => format!("{workspace} / {project}"),
            (Some(workspace), _, _) => workspace.clone(),
            _ => String::new(),
        }
    }
}
