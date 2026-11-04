//! 按住默认派发修饰键时浮出卡片序号（Windows Alt，macOS Ctrl）；Ctrl+Shift提示新建类型。
//! 提示层不接收指针事件，松开修饰键即消失；只读窗口修饰键，不碰存储。
//!
//! 设计注记（apple-design）：提示服务于“第几张卡片”的即时判断，随修饰键出现/消失，
//! 无位移与淡入动画，因此无需额外的减少动态效果分支。

use super::super::idea_drag::Rect;
use crate::app::MultisoulApp;
use crate::shortcuts::idea_assign::{badge_mode, new_agent_hint, BadgeMode};
use gpui::{div, prelude::*, px, rgba, Context, Div, Window};
use ui::component::StyledExt;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_idea_badges_tests.rs"]
mod tests;

/// 第 i 张卡片的序号；`⌃1…9` 之外的卡片没有序号。
pub(in crate::workspace::desktop) fn card_numbers(count: usize) -> Vec<Option<u8>> {
    (0..count)
        .map(|index| u8::try_from(index + 1).ok().filter(|n| *n <= 9))
        .collect()
}

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn render_idea_badges(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<Div> {
        let modifiers = window.modifiers();
        if modifiers.function {
            return None;
        }
        let todo = self
            .desktop_workspace
            .ideas
            .iter()
            .filter(|idea| idea.state == workspace::ideas::IdeaState::Todo);
        let mode = badge_mode(
            modifiers.control,
            modifiers.shift,
            modifiers.alt,
            modifiers.platform,
            todo.count(),
        );
        if mode == BadgeMode::Hidden || self.desktop_workspace.idea_drag.is_some() {
            return None;
        }
        let (targets, _) = self.idea_drag_targets(window, cx)?;
        let layer = div().absolute().inset_0();
        Some(match mode {
            BadgeMode::Cards => layer.children(
                targets
                    .cards
                    .iter()
                    .zip(card_numbers(targets.cards.len()))
                    .filter_map(|(card, number)| Some(badge(card.rect, number?))),
            ),
            _ => {
                let zone = targets.new_zone;
                layer.child(hint_pill(
                    zone,
                    new_agent_hint(self.desktop_workspace.ideas_aux.default_profile),
                ))
            }
        })
    }
}

fn badge(card: Rect, number: u8) -> gpui::Stateful<Div> {
    div()
        .id(("desktop-idea-badge", usize::from(number)))
        .absolute()
        .left(px(card.x - 8.0))
        .top(px(card.y - 13.0))
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(rgba(0xbfe6ffcc))
        .bg(rgba(0x123552f2))
        .text_size(px(14.0))
        .font_semibold()
        .text_color(rgba(0xffffffff))
        .child(number.to_string())
}

fn hint_pill(zone: Rect, text: String) -> Div {
    div()
        .absolute()
        .left(px(zone.x - 120.0))
        .top(px(zone.y))
        .w(px(zone.w + 240.0))
        .flex()
        .justify_center()
        .child(
            div()
                .px(px(14.0))
                .py(px(7.0))
                .rounded_full()
                .border_1()
                .border_color(rgba(0xbfe6ffcc))
                .bg(rgba(0x123552f2))
                .text_size(px(12.0))
                .font_semibold()
                .text_color(rgba(0xffffffff))
                .child(text),
        )
}
