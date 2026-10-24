//! 只属于某个窗口的临时界面状态：悬停预览、打开的菜单、重播计时。

use super::state::Source;
use std::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct EffectsUiState {
    /// 正在悬停预览的网格序号。
    pub hover: Option<usize>,
    pub hover_since: Option<Instant>,
    /// 当前展开的来源菜单与键盘浏览位置（浏览不改变已保存的值）。
    pub menu: Option<Source>,
    pub menu_cursor: usize,
    /// 减少动态效果时，大预览只在点击“重播”后播放一次。
    pub replay_started: Option<Instant>,
}
