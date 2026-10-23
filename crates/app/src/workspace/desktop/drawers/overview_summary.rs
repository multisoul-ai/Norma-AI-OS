//! 运行概览顶部的三张统计卡：待你处理 / 运行中 / 待开始。
//!
//! 设计注记（Material 3 tonal card）：数字是第一层级，标签写清单位；点击展开并跳到对应分组，
//! 悬停、按下各加一档状态层。数字与分组标题同源（`OverviewScreen::summary`），不在 render 里重算。

use super::super::overview_layout::OverviewGroup;
use super::super::overview_rows::{overview_feedback, TONE_AMBER, TONE_RUNNING};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId};
use ui::component::StyledExt;
use ui::icons::with_alpha;

const CARD_HEIGHT: f32 = 52.0;
const NUMBER: u32 = 0xeef6ffff;
const LABEL: u32 = 0xa9c3daff;

impl MultisoulApp {
    pub(super) fn render_overview_summary(
        &self,
        counts: [usize; 3],
        cx: &mut Context<Self>,
    ) -> Div {
        let feedback = overview_feedback(self.desktop_theme());
        let cards = [
            (OverviewGroup::Attention, "待你处理", "项", TONE_AMBER),
            (OverviewGroup::Running, "运行中", "个", TONE_RUNNING),
            (OverviewGroup::Idle, "待开始", "个", 0xb3c6d8ff),
        ];
        div()
            .flex_none()
            .px(px(4.0))
            .pb(px(4.0))
            .flex()
            .gap(px(6.0))
            .children(
                cards
                    .into_iter()
                    .zip(counts)
                    .map(|((group, label, unit, color), count)| {
                        let hover = feedback.hover;
                        let pressed = feedback.pressed;
                        let tint = if count == 0 {
                            0x9fcfff0d
                        } else {
                            with_alpha(color, 0x1c)
                        };
                        let id = format!("desktop-run-summary-{}", group.key());
                        div()
                            .id(ElementId::Name(id.clone().into()))
                            .debug_selector(move || id.clone())
                            .flex_1()
                            .min_w(px(0.0))
                            .h(px(CARD_HEIGHT))
                            .px(px(12.0))
                            .flex()
                            .flex_col()
                            .justify_center()
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
                                    .text_size(px(22.0))
                                    .font_semibold()
                                    .text_color(rgba(if count == 0 {
                                        with_alpha(NUMBER, 0x80)
                                    } else {
                                        color
                                    }))
                                    .child(count.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgba(LABEL))
                                    .overflow_hidden()
                                    .child(format!("{label} · {unit}")),
                            )
                    }),
            )
    }
}
