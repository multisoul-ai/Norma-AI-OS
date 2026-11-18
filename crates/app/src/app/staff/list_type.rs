//! 幕僚任务列表和归档条的字级。数值只来自 docs/design.md §4.2 和 §5，不在这里另造字号。
use gpui::{div, prelude::*, px, relative, Div, FontWeight};

#[derive(Clone, Copy)]
pub(super) struct Role {
    pub size: f32,
    pub line: f32,
    pub weight: FontWeight,
}

pub(super) const BODY: Role = Role {
    size: 14.,
    line: 1.48,
    weight: FontWeight::NORMAL,
};
pub(super) const CALLOUT: Role = Role {
    size: 13.,
    line: 1.42,
    weight: FontWeight::MEDIUM,
};
pub(super) const CAPTION: Role = Role {
    size: 11.,
    line: 1.36,
    weight: FontWeight::MEDIUM,
};

/// §5.3 命中区域下限。头像同为 36，是因为它本来就得达到这个下限。
pub(super) const HIT: f32 = 36.;
pub(super) const AVATAR: f32 = 36.;
pub(super) const SPACE_1: f32 = 4.;
pub(super) const SPACE_2: f32 = 8.;
pub(super) const SPACE_3: f32 = 12.;

pub(super) fn text(role: Role) -> Div {
    div()
        .text_size(px(role.size))
        .line_height(relative(role.line))
        .font_weight(role.weight)
}
