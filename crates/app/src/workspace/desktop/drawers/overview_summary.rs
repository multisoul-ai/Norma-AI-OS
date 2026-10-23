//! 运行概览顶部的三张统计卡：待你处理 / 运行中 / 待开始。
//!
//! 设计注记（Material 3 tonal card）：数字是第一层级，标签只写状态名（与原型一致）；点击展开并跳到对应分组，
//! 悬停、按下各加一档状态层。数字与分组标题同源（`OverviewScreen::summary`），不在 render 里重算。

use super::super::overview_layout::OverviewGroup;
use super::super::overview_rows::overview_feedback;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId};
use ui::component::StyledExt;
use ui::icons::with_alpha;

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
        // （分组，标签，底色，数字色）：底色按类型固定，数量为 0 时只把数字调淡，卡片不变灰。
        let cards = [
            (
                OverviewGroup::Attention,
                "待你处理",
                0xf2c7731c_u32,
                0xf6dfaaff_u32,
            ),
            (OverviewGroup::Running, "运行中", 0x36dce81a, 0x7ee9eeff),
            (OverviewGroup::Idle, "待开始", 0x9fcfff14, 0xb3c6d8ff),
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
                            .bg(rgba(tint))
                            .cursor_pointer()
                            .hover(move |style| style.bg(rgba(hover)))
                            .active(move |style| style.bg(rgba(pressed)))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.jump_to_overview_group(group, cx);
                            }))
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
