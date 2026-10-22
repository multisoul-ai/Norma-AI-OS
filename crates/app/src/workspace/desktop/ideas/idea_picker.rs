//! `⌃⇧0` 新建智能体的类型选择器：列出全部类型（含没有数字键的 ZCode、InfCode），
//! ↑/↓ 移动、回车或点击确认、2…9 直达、Esc 或点空白收回。选定后新建卡片并派发光标所在想法。
//!
//! 设计注记（apple-design / material-menus）：选择器是短暂的模态浮层，出现与消失即时，无位移动画；
//! 已选高亮与悬停区分，所有行可键盘与指针操作。

use super::idea_assign::AssignRequest;
use crate::app::MultisoulApp;
use crate::shortcuts::idea_assign::{picker_index_for_digit, picker_profiles};
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful, Window};

const ROW_HEIGHT: f32 = 32.0;
const WIDTH: f32 = 220.0;

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn open_idea_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let default = self.desktop_workspace.ideas_aux.default_profile;
        let start = picker_profiles()
            .iter()
            .position(|profile| *profile == default)
            .unwrap_or(0);
        self.desktop_workspace.ideas_aux.picker = Some(start);
        self.desktop_workspace.ideas_menu = None;
        // 键盘交给想法栏：↑/↓/回车/Esc 由它的按键处理接管。
        self.reveal_ideas_group(cx);
        self.left_ideas_focus(cx).focus(window);
        cx.notify();
    }

    fn close_idea_picker(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.ideas_aux.picker = None;
        cx.notify();
    }

    /// 选择器打开时独占键盘；其他按键吞掉，避免误触想法栏里的删除等操作。
    pub(in crate::workspace::desktop) fn handle_idea_picker_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(cursor) = self.desktop_workspace.ideas_aux.picker else {
            return false;
        };
        let last = picker_profiles().len() - 1;
        match key {
            "escape" => self.close_idea_picker(cx),
            "down" => self.desktop_workspace.ideas_aux.picker = Some((cursor + 1).min(last)),
            "up" => self.desktop_workspace.ideas_aux.picker = Some(cursor.saturating_sub(1)),
            "enter" => self.choose_idea_profile(cursor, window, cx),
            other => {
                let digit = other.chars().next().filter(|_| other.chars().count() == 1);
                if let Some(index) = digit.and_then(picker_index_for_digit) {
                    self.choose_idea_profile(index, window, cx);
                }
            }
        }
        cx.notify();
        true
    }

    fn choose_idea_profile(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.desktop_workspace.ideas_aux.picker = None;
        if let Some(profile) = picker_profiles().get(index) {
            self.run_assign_request(AssignRequest::Profile(*profile), window, cx);
        }
    }

    pub(in crate::workspace::desktop) fn render_idea_picker(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let cursor = self.desktop_workspace.ideas_aux.picker?;
        let (targets, _) = self.idea_drag_targets(window, cx)?;
        let zone = targets.new_zone;
        let profiles = picker_profiles();
        let height = profiles.len() as f32 * ROW_HEIGHT + 40.0;
        let feedback = super::super::overview_rows::overview_feedback(self.desktop_theme());
        let rows = profiles.into_iter().enumerate().map(|(index, profile)| {
            let selected = index == cursor;
            let (hover, pressed, focus) = (feedback.hover, feedback.pressed, feedback.focus);
            let id = format!("desktop-idea-picker-{index}");
            div()
                .id(ElementId::Name(id.clone().into()))
                .debug_selector(move || id.clone())
                .h(px(ROW_HEIGHT))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(if selected { focus } else { 0x00000000 }))
                .cursor_pointer()
                .hover(move |style| style.bg(rgba(hover)))
                .active(move |style| style.bg(rgba(pressed)))
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.choose_idea_profile(index, window, cx);
                }))
                .text_size(px(13.0))
                .text_color(rgba(0xe8f3fdff))
                .child(div().flex_1().child(format!("{profile:?}")))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(0x9fb8cfff))
                        .child(
                            (index < 8)
                                .then(|| format!("⌃⇧{}", index + 2))
                                .unwrap_or_default(),
                        ),
                )
        });
        let panel = div()
            .id("desktop-idea-picker")
            .debug_selector(|| "desktop-idea-picker".to_string())
            .absolute()
            .left(px(zone.x + (zone.w - WIDTH) / 2.0))
            .top(px((zone.y + zone.h - height).max(8.0)))
            .w(px(WIDTH))
            .p(px(6.0))
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .border_1()
            .border_color(rgba(0x83c4ff66))
            .bg(rgba(0x0d1d2ff2))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px(px(10.0))
                    .pt(px(4.0))
                    .pb(px(6.0))
                    .text_size(px(11.0))
                    .text_color(rgba(0xa9c3daff))
                    .child("新建哪种智能体来完成"),
            )
            .children(rows);
        // 全窗口透明底：点空白处收回，不让点击落到下面的画布。
        Some(
            div()
                .id("desktop-idea-picker-backdrop")
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|app, _, _, cx| app.close_idea_picker(cx)),
                )
                .child(panel),
        )
    }
}
