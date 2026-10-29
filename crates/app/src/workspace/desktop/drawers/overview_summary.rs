//! 运行概览顶部的三张统计卡：待你处理 / 运行中 / 待开始。
//!
//! 设计注记（Material 3 tonal card）：数字是第一层级，标签只写状态名（与原型一致）；点击展开并跳到对应分组，
//! 悬停、按下各加一档状态层。数字与分组标题同源（`OverviewScreen::summary`），不在 render 里重算。

use super::super::overview_layout::OverviewGroup;
use super::super::overview_rows::overview_feedback;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, KeyDownEvent};
use ui::component::StyledExt;
use ui::icons::with_alpha;

#[cfg(test)]
#[path = "../../../../tests/unit/overview_summary_tests.rs"]
mod tests;

// 卡片内容：上下内边距 10 + 数字行 28 + 间隙 2 + 标签行 16 = 66。行高显式给定，不依赖默认行高，
// 否则 22px 的数字与 12px 的标签会超出卡片高度而贴边。
const NUMBER_LINE: f32 = 28.0;
const LABEL_LINE: f32 = 16.0;
const LABEL: u32 = 0xa9c3daff;

impl MultisoulApp {
    pub(super) fn render_overview_summary(
        &self,
        counts: [usize; 3],
        cx: &mut Context<Self>,
    ) -> Div {
        let feedback = overview_feedback(self.desktop_theme());
        let focus_ring = feedback.focus;
        // （分组，标签，底色，数字色）：底色按类型固定，数量为 0 时只把数字调淡，卡片不变灰。
        let cards = [
            (
                OverviewGroup::Attention,
                ui::i18n::text("drawers.your_attention"),
                0xf2c7731c_u32,
                0xf6dfaaff_u32,
            ),
            (
                OverviewGroup::Running,
                ui::i18n::text("drawers.running"),
                0x36dce81a,
                0x7ee9eeff,
            ),
            (
                OverviewGroup::Idle,
                ui::i18n::text("drawers.not_started"),
                0x9fcfff14,
                0xb3c6d8ff,
            ),
        ];
        div()
            .flex_none()
            .px(px(4.0))
            .pt(px(6.0))
            .pb(px(10.0))
            .flex()
            .gap(px(8.0))
            .children(
                cards
                    .into_iter()
                    .zip(counts)
                    .map(|((group, label, tint, color), count)| {
                        let hover = feedback.hover;
                        let pressed = feedback.pressed;
                        let id = format!("desktop-run-summary-{}", group.key());
                        div()
                            .id(ElementId::Name(id.clone().into()))
                            .debug_selector(move || id.clone())
                            .flex_1()
                            .min_w(px(0.0))
                            .px(px(14.0))
                            .py(px(10.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .rounded(px(12.0))
                            .border_1()
                            .border_color(rgba(0x0000_0000))
                            .bg(rgba(tint))
                            .cursor_pointer()
                            // 只给 id 与 tab_index：GPUI 会在元素状态里自动创建带 tab stop 的焦点句柄。
                            .tab_index(0)
                            .hover(move |style| style.bg(rgba(hover)))
                            .active(move |style| style.bg(rgba(pressed)))
                            .focus(move |style| style.border_color(rgba(focus_ring)))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.jump_to_overview_group(group, window, cx);
                            }))
                            .on_key_down(cx.listener(
                                move |app, event: &KeyDownEvent, window, cx| {
                                    if summary_key_activates(
                                        event.keystroke.key.as_str(),
                                        event.keystroke.modifiers.modified(),
                                    ) {
                                        cx.stop_propagation();
                                        app.jump_to_overview_group(group, window, cx);
                                    }
                                },
                            ))
                            .child(
                                div()
                                    .h(px(NUMBER_LINE))
                                    .line_height(px(NUMBER_LINE))
                                    .text_size(px(22.0))
                                    .font_semibold()
                                    .text_color(rgba(if count == 0 {
                                        with_alpha(color, 0x80)
                                    } else {
                                        color
                                    }))
                                    .child(count.to_string()),
                            )
                            .child(
                                div()
                                    .h(px(LABEL_LINE))
                                    .line_height(px(LABEL_LINE))
                                    .text_size(px(12.0))
                                    .text_color(rgba(LABEL))
                                    .overflow_hidden()
                                    .child(label),
                            )
                    }),
            )
    }
}

/// 统计卡的键盘激活：回车和空格与点击等价；带修饰键的留给快捷键，其余按键交给焦点移动与概览导航。
pub(super) fn summary_key_activates(key: &str, modified: bool) -> bool {
    !modified && matches!(key, "enter" | "space")
}
