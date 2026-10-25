//! 只属于某个窗口的临时界面状态：悬停预览、打开的菜单、重播计时。

use super::state::Source;
use crate::motion::run_style::{RunState, RunStyle};
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
    pub run: RunPreviewUi,
}

/// 运行特效分区的预览：大预览自动轮播或固定某个状态；网格悬停的格子循环播放。
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RunPreviewUi {
    /// `None` = 自动轮播。
    pub pinned: Option<RunState>,
    /// 当前轮播或固定状态从何时开始计时。
    pub since: Option<Instant>,
    /// 点击大预览卡片 = 标记完成提示已查看。
    pub viewed_at: Option<Instant>,
    pub hover: Option<(RunStyle, Instant)>,
}
