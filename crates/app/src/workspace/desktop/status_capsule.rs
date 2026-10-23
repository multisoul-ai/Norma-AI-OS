//! Canvas status capsule.
//!
//! 参考 apple-design：浮动控件留在现有顶栏右簇，和抽屉重开按钮同一玻璃材质、同一 32px 行高。
//! 悬停与按下立即换色，不加弹簧或装饰动效。内容是跨项目汇总（运行中 / 待开始 / 待处理），
//! 与抽屉统计同一份投影；左右抽屉把画布挤窄时只留环、数字和角标，状态词进入悬停提示和读屏文字。拒绝另做一条画布顶栏。悬浮图钉属于后续工单，这里不画。

use super::{glass_button_variant, GLASS_TEXT};
use crate::app::MultisoulApp;
use gpui::{
    canvas, div, point, prelude::*, px, rgba, svg, AnyElement, ClickEvent, Context, PathBuilder,
    Window,
};
use runtime::desktop_drawers::{
    route_status_capsule, DesktopDrawerState, StatusCapsuleClick, StatusCapsuleEffect,
};
use ui::component::button::{Button, ButtonVariants};
use ui::icons::with_alpha;

const AMBER: u32 = 0xf2c773ff;
const COMPACT_CANVAS_WIDTH: f32 = 680.0;
const STATUS_WORD: u32 = 0xa9bfd1ff;
const RING_TRACK: u32 = 0xcde8ff2e;

#[derive(Clone, Copy)]
pub(super) struct CapsuleChrome {
    pub rest: u32,
    pub hover: u32,
    pub active: u32,
    pub focus: u32,
    pub ring: u32,
    pub amber: u32,
}

pub(super) fn capsule_chrome(theme: ui::Theme) -> CapsuleChrome {
    CapsuleChrome {
        rest: with_alpha(theme.dock, 0xa0),
        hover: with_alpha(theme.accent_blue, 0x78),
        active: with_alpha(theme.accent_blue, 0x9c),
        focus: theme.focused_stroke,
        ring: theme.accent_cyan,
        amber: AMBER,
    }
}

impl MultisoulApp {
    pub(crate) fn render_status_capsule(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let capsule = self.desktop_workspace.activity_capsule.clone()?;
        let summary = capsule.summary;
        if summary.attention + summary.running + summary.idle == 0 {
            return None;
        }
        let layout = self.desktop_assistant_layout(window);
        let drawers = self.desktop_workspace.drawers;
        let compact =
            drawers.left_open && drawers.right_open && layout.canvas_width < COMPACT_CANVAS_WIDTH;
        let theme = self.desktop_theme();
        let chrome = capsule_chrome(theme);
        let spoken = capsule.accessibility_label.clone();
        let status_focus = self.run_status_focus(cx);
        let pending = summary.attention;
        Some(
            div()
                .id("desktop-status-capsule-slot")
                .track_focus(&status_focus)
                .child(
                    Button::new("desktop-status-capsule")
                        .debug_selector(|| "desktop-status-capsule".to_string())
                        .h(px(32.0))
                        .pl(px(11.0))
                        .pr(px(10.0))
                        .rounded(px(9.0))
                        .custom(glass_button_variant(theme, cx))
                        .tooltip(spoken.clone())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .absolute()
                                        .size(px(0.0))
                                        .overflow_hidden()
                                        .text_size(px(1.0))
                                        .child(spoken),
                                )
                                .child(count_chip(
                                    summary.running,
                                    "运行中",
                                    Some(1.0),
                                    (chrome.ring, with_alpha(chrome.focus, 0x88)),
                                    compact,
                                ))
                                .child(count_chip(
                                    summary.idle,
                                    "待开始",
                                    None,
                                    (chrome.ring, with_alpha(chrome.focus, 0x88)),
                                    compact,
                                ))
                                .when(pending > 0, |row| {
                                    row.child(div().w(px(1.0)).h(px(14.0)).bg(rgba(0xcde8ff33)))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(6.0))
                                                .child(
                                                    div()
                                                        .min_w(px(18.0))
                                                        .h(px(18.0))
                                                        .px(px(5.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded(px(9.0))
                                                        .bg(rgba(chrome.amber))
                                                        .text_size(px(11.0))
                                                        .font_weight(gpui::FontWeight::BOLD)
                                                        .text_color(rgba(0x162638ff))
                                                        .child(pending.to_string()),
                                                )
                                                .when(!compact, |badge| {
                                                    badge.child(
                                                        div()
                                                            .text_size(px(12.0))
                                                            .text_color(rgba(0xf6dfaaff))
                                                            .child("待处理"),
                                                    )
                                                }),
                                        )
                                }),
                        )
                        .when(drawers.overview_floating, |button| {
                            button.child(
                                svg()
                                    .path("icons/pin.svg")
                                    .w(px(12.0))
                                    .h(px(12.0))
                                    .text_color(rgba(GLASS_TEXT)),
                            )
                        })
                        .on_click(cx.listener(|app, event: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            let from_keyboard = matches!(event, ClickEvent::Keyboard(_));
                            app.activate_status_capsule(from_keyboard, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn status_capsule_visible(&self) -> bool {
        self.desktop_workspace
            .activity_capsule
            .as_ref()
            .is_some_and(|capsule| {
                capsule.summary.attention + capsule.summary.running + capsule.summary.idle > 0
            })
    }

    fn activate_status_capsule(
        &mut self,
        from_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let floating = self.desktop_workspace.drawers.overview_floating;
        let (next, effect) = route_status_capsule(
            self.desktop_workspace.drawers,
            StatusCapsuleClick {
                floating,
                from_keyboard,
            },
        );
        self.desktop_workspace.focus_status_overview_row = matches!(
            effect,
            StatusCapsuleEffect::ShowOverview {
                focus_first_row: true
            }
        );
        if matches!(effect, StatusCapsuleEffect::FocusFloating) {
            let focus = self
                .desktop_workspace
                .overview_float_focus
                .get_or_insert_with(|| cx.focus_handle())
                .clone();
            window.focus(&focus);
            return;
        }
        self.apply_desktop_drawers(next, cx);
    }

    pub(crate) fn apply_desktop_drawers(
        &mut self,
        next: DesktopDrawerState,
        cx: &mut Context<Self>,
    ) {
        let before = self.desktop_workspace.drawers;
        self.desktop_workspace.drawers = next;
        if before.left_view != next.left_view {
            self.desktop_workspace.left_view_slide.retarget(
                next.left_view.rank() > before.left_view.rank(),
                self.motion.reduce_motion,
            );
        }
        if before.overview_floating != next.overview_floating {
            if next.overview_floating {
                self.desktop_workspace.overview_float_fade.retarget(true);
            } else {
                self.desktop_workspace.overview_float_fade =
                    super::drawers::float::OverviewFloatFade::hidden();
            }
        }
        let layout_changed = before.left_view != next.left_view
            || before.overview_floating != next.overview_floating
            || before.overview_offset != next.overview_offset;
        if layout_changed {
            if let Some(id) = self.desktop_workspace.active_id.clone() {
                self.persist_left_drawer_view(&id, cx);
            }
        }
        self.desktop_workspace
            .drawer_motion
            .retarget(next.left_open, next.right_open);
        if !before.right_open && next.right_open {
            self.load_selected_desktop_content(cx);
        }
        self.wake_animation_frame_loop();
        cx.notify();
    }

    pub(crate) fn focus_status_overview_first_row(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.desktop_workspace.focus_status_overview_row {
            return;
        }
        self.desktop_workspace.focus_status_overview_row = false;
        let focus = self.left_overview_focus(cx);
        let has_row = self
            .desktop_workspace
            .areas
            .iter()
            .any(|area| !self.overview_rows_for(&area.id).is_empty());
        window.on_next_frame(move |window, _cx| {
            window.focus(&focus);
            if has_row {
                window.focus_next();
            }
        });
    }
}

/// 汇总条的一段：环 + 数量 + 状态词。运行中是满环，待开始是空环；窄画布只留环和数量。
fn count_chip(
    count: usize,
    word: &'static str,
    fraction: Option<f32>,
    (ring, track): (u32, u32),
    compact: bool,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(progress_ring(fraction, false, ring, track))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgba(0xe4f1fdff))
                .child(count.to_string()),
        )
        .when(!compact, |chip| {
            chip.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(STATUS_WORD))
                    .child(word),
            )
        })
}

/// 16px 的环：底环与进度弧在同一个画布里绘制，线宽 3px，和原型的圆环蒙版一致。
fn progress_ring(fraction: Option<f32>, waiting: bool, color: u32, _track: u32) -> gpui::Div {
    let fraction = fraction.unwrap_or(0.0).clamp(0.0, 1.0);
    div()
        .relative()
        .w(px(16.0))
        .h(px(16.0))
        .flex_none()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let center = bounds.center();
                    let paint_arc = |share: f32, tint: u32, window: &mut Window| {
                        let mut builder = PathBuilder::stroke(px(3.0));
                        let steps = ((48.0 * share).ceil() as i32).max(1);
                        let start = -std::f32::consts::FRAC_PI_2;
                        let sweep = std::f32::consts::TAU * share;
                        for index in 0..=steps {
                            let angle = start + sweep * index as f32 / steps as f32;
                            let sample = point(
                                center.x + px(angle.cos() * 6.5),
                                center.y + px(angle.sin() * 6.5),
                            );
                            if index == 0 {
                                builder.move_to(sample);
                            } else {
                                builder.line_to(sample);
                            }
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, rgba(tint));
                        }
                    };
                    paint_arc(1.0, RING_TRACK, window);
                    if fraction > 0.0 {
                        paint_arc(fraction, color, window);
                    }
                },
            )
            .size_full(),
        )
        .when(waiting, |ring| {
            ring.child(
                div()
                    .absolute()
                    .top(px(-2.0))
                    .left(px(11.0))
                    .w(px(7.0))
                    .h(px(7.0))
                    .rounded_full()
                    .bg(rgba(AMBER)),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::capsule_chrome;

    /// 场景：状态胶囊的静止、悬停、按下和焦点在浅色与深色主题下都要能分开。
    ///
    /// 数据构造：paper_blue 与 sapphire_night 两套主题色。
    ///   静止取 dock 的 0xa0，悬停取 accent_blue 的 0x78，按下取同一色的 0x9c。
    /// 执行过程：分别计算两套胶囊色，并比较同一主题内的状态色。
    /// 预期结果：同一主题里静止、悬停、按下、焦点互不相同；两套主题的静止色和焦点色也不同。
    #[test]
    fn capsule_chrome_states_differ_in_light_and_dark() {
        let light = capsule_chrome(ui::Theme::paper_blue());
        let dark = capsule_chrome(ui::Theme::sapphire_night());
        for chrome in [light, dark] {
            assert_ne!(chrome.rest, chrome.hover, "悬停必须和静止底色不同");
            assert_ne!(chrome.hover, chrome.active, "按下必须比悬停更深");
            assert_ne!(chrome.focus, chrome.rest, "焦点环不能和静止底色相同");
            assert_ne!(chrome.amber, chrome.ring, "琥珀标记不能和进度环同色");
            assert_ne!(chrome.focus, 0, "焦点色必须可见");
        }
        assert_ne!(light.rest, dark.rest, "浅色与深色的静止玻璃不能是同一色");
        assert_ne!(light.hover, dark.hover, "浅色与深色的悬停色必须分开");
        assert_ne!(light.focus, dark.focus, "浅色与深色的焦点色必须分开");
        assert_ne!(light.active, light.rest, "按下不能退回静止色");
    }
}
