//! 运行概览的分栏约定：固定顺序、基准高度、折叠状态与空态文案。纯数据，不碰 GPUI。
//!
//! 每栏有一个基准高度；抽屉放不下时按比例收缩（栏内滚动），有多余高度时按权重分摊，
//! 折叠的栏只占一行标题，把高度让给其余栏。

#[cfg(test)]
#[path = "../../../tests/unit/desktop_overview_layout_tests.rs"]
mod tests;

/// 一栏至少容得下标题和一行内容。
pub(super) const MIN_GROUP_HEIGHT: f32 = 84.0;
/// 栏标题（30）加分隔线上方的间距（4）。
const GROUP_CHROME_HEIGHT: f32 = 34.0;
/// 行与行之间的间隙，与渲染一致。
const ROW_GAP: f32 = 2.0;
/// 没有行时的空态占高：图标、标题、提示三行加上下留白。
const EMPTY_GROUP_HEIGHT: f32 = 120.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverviewGroup {
    Attention,
    Running,
    Idle,
    Ideas,
}

impl OverviewGroup {
    /// 产品约定的固定顺序，不随内容变化：先看要处理的，再看谁在跑、谁待开始，最后是想法。
    pub(super) const ORDER: [Self; 4] = [Self::Attention, Self::Running, Self::Idle, Self::Ideas];

    /// 栏高上限（想法栏为固定基准高度）。状态栏实际起始高度见 `basis_height`，内容少时更矮。
    pub(super) fn base_height(self) -> f32 {
        match self {
            Self::Attention => 200.0,
            Self::Running => 260.0,
            Self::Idle => 200.0,
            Self::Ideas => 250.0,
        }
    }

    /// 有多余高度时的分摊权重：想法栏吃掉大部分剩余高度，状态栏只在想法栏折叠时才分到一点。
    pub(super) fn grow(self) -> f32 {
        match self {
            Self::Attention => 0.1,
            Self::Running => 0.2,
            Self::Idle => 0.1,
            Self::Ideas => 1.5,
        }
    }

    /// 状态栏的起始高度：按内容定，封顶到基准高度（超出就栏内滚动）。
    /// `row_heights` 是各行实际行高；没有行时按空态高度。想法栏不按内容定，始终用基准高度。
    pub(super) fn basis_height(self, row_heights: &[f32]) -> f32 {
        if self == Self::Ideas {
            return self.base_height();
        }
        let content = if row_heights.is_empty() {
            EMPTY_GROUP_HEIGHT
        } else {
            GROUP_CHROME_HEIGHT
                + row_heights
                    .iter()
                    .map(|height| height + ROW_GAP)
                    .sum::<f32>()
        };
        content.clamp(MIN_GROUP_HEIGHT, self.base_height())
    }

    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Attention => "attention",
            Self::Running => "running",
            Self::Idle => "idle",
            Self::Ideas => "ideas",
        }
    }

    /// 折叠位固定编号并写进持久化状态（运行时只保留低四位），不随枚举顺序变化。
    /// 位 1、2 沿用旧版“当前项目”“想法”的编号；位 3 旧版是“其他项目”，
    /// 它的行现在并入“待开始”，所以旧偏好折叠了“其他项目”的人会看到“待开始”仍是折叠的。
    fn bit(self) -> u8 {
        match self {
            Self::Attention => 1 << 0,
            Self::Running => 1 << 1,
            Self::Ideas => 1 << 2,
            Self::Idle => 1 << 3,
        }
    }

    /// 空态顶部的小图标字符。
    pub(super) fn empty_icon(self) -> &'static str {
        match self {
            Self::Attention => "✓",
            Self::Running => "◌",
            Self::Idle => "✓",
            Self::Ideas => "💡",
        }
    }

    /// 栏内没有内容时的（标题，提示）。
    pub(super) fn empty_copy(self) -> (&'static str, &'static str) {
        match self {
            Self::Attention => ("没有需要你处理的事", "审核、成果和待查看文件会出现在这里"),
            Self::Running => (
                "没有正在运行的智能体",
                "对 Norma 说“新建一个 Claude”，或把想法拖给智能体",
            ),
            Self::Idle => ("没有空闲的智能体", "所有智能体都在工作"),
            Self::Ideas => (
                "还没有想法",
                "对 Norma 说“记一下：…”，或在下方输入；记下后拖到智能体卡片即可开工",
            ),
        }
    }
}

/// 各栏的折叠状态（位集合）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OverviewCollapse(u8);

impl OverviewCollapse {
    /// 从抽屉状态里保存的折叠位还原。
    pub(super) fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub(super) fn bits(self) -> u8 {
        self.0
    }

    pub(super) fn is_collapsed(self, group: OverviewGroup) -> bool {
        self.0 & group.bit() != 0
    }

    pub(super) fn toggle(&mut self, group: OverviewGroup) {
        self.0 ^= group.bit();
    }

    /// 要求某栏可见：展开它，其他栏不动。
    pub(super) fn reveal(&mut self, group: OverviewGroup) {
        self.0 &= !group.bit();
    }
}
