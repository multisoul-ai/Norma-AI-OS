//! 设置页一次渲染所需的数据快照：在 `MultisoulApp` 里组装，视图只读它。

use super::state::{EffectsSettingsState, Source};
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub(crate) struct EffectsPageData {
    pub state: EffectsSettingsState,
    /// 系统或环境要求减少动态效果时只暂停自动预览，不改变实际派发款式。
    pub reduce_motion: bool,
    pub hover: Option<usize>,
    pub hover_since: Option<Instant>,
    pub menu: Option<Source>,
    pub menu_cursor: usize,
    pub replay_started: Option<Instant>,
    pub run: super::ui_state::RunPreviewUi,
}
