//! 运行概览的分栏约定：固定顺序、基准高度、折叠状态与空态文案。纯数据，不碰 GPUI。
//!
//! 每栏有一个基准高度；抽屉放不下时按比例收缩（栏内滚动），有多余高度时按权重分摊，
//! 折叠的栏只占一行标题，把高度让给其余栏。

#[cfg(test)]
#[path = "../../../tests/unit/desktop_overview_layout_tests.rs"]
mod tests;

/// 一栏至少容得下标题和一行内容。
pub(super) const MIN_GROUP_HEIGHT: f32 = 84.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverviewGroup {
    Attention,
    Current,
    Ideas,
    Other,
}

impl OverviewGroup {
    /// 产品约定的固定顺序，不随内容变化。
    pub(super) const ORDER: [Self; 4] = [Self::Attention, Self::Current, Self::Ideas, Self::Other];

    pub(super) fn base_height(self) -> f32 {
        match self {
            Self::Attention => 190.0,
            Self::Current => 210.0,
            Self::Ideas => 236.0,
            Self::Other => 88.0,
        }
    }

    /// 有多余高度时的分摊权重。
    pub(super) fn grow(self) -> f32 {
        match self {
            Self::Attention => 1.0,
            Self::Current => 1.2,
            Self::Ideas => 1.4,
            Self::Other => 0.6,
        }
    }

    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Attention => "attention",
            Self::Current => "current",
            Self::Ideas => "ideas",
            Self::Other => "other",
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }

    /// 栏内没有内容时的（标题，提示）。
    pub(super) fn empty_copy(self) -> (&'static str, &'static str) {
        match self {
            Self::Attention => ("没有需要你处理的事", "审核、成果和待查看文件会出现在这里"),
            Self::Current => (
                "当前项目还没有智能体",
                "对 Norma 说“新建一个 Claude”，或在画布上添加",
            ),
            Self::Ideas => (
                "还没有想法",
                "对 Norma 说“记一下：…”，或在下方输入；记下后拖到智能体卡片即可开工",
            ),
            Self::Other => ("没有其他项目", "新建或打开项目后显示在这里"),
        }
    }
}

/// 各栏的折叠状态（位集合）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OverviewCollapse(u8);

impl OverviewCollapse {
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
