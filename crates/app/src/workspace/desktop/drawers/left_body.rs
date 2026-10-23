//! Chat and run-overview bodies share the drawer. Only the body slides.

use super::{GLASS_TEXT_MUTED, GLASS_TEXT_SECONDARY};
use crate::app::assistant_conversation::Speaker;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, KeyDownEvent, Stateful, Window};
use runtime::desktop_drawers::{DesktopDrawerEvent, DesktopLeftView};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    StyledExt,
};

impl MultisoulApp {
    pub(super) fn render_left_drawer_body(
        &mut self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let reduce = self.motion.reduce_motion;
        let slide = self.desktop_workspace.left_view_slide;
        let offset = slide.offset(reduce);
        let opacity = slide.opacity();
        // 对话入口暂时隐藏：左栏固定显示运行概览，持久化的 Chat 视图也不再渲染。
        let body = match self.desktop_workspace.drawers.left_view {
            DesktopLeftView::Chat if super::chat::LEFT_CHAT_ENTRY_VISIBLE => {
                self.render_left_chat_body(width, cx)
            }
            _ => self.render_left_overview_body(window, cx),
        };
        div()
            .id("desktop-left-view-body")
            .debug_selector(|| "desktop-left-view-body".to_string())
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .bottom(px(0.0))
                    .left(px(offset))
                    .right(px(-offset))
                    .flex()
                    .flex_col()
                    .opacity(opacity)
                    .child(body),
            )
    }

    fn render_left_chat_body(&mut self, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let (_, _, status) = self.desktop_voice_presentation(cx);
        let messages = self.assistant_conversation.messages().to_vec();
        let conversation_scroll = self.assistant_conversation.scroll.clone();
        div()
            .id("desktop-left-chat-body")
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("desktop-assistant-intro")
                    .px(px(20.0))
                    .pt(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.0))
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgba(GLASS_TEXT_MUTED))
                                            .child("当前对话"),
                                    )
                                    .child(
                                        div()
                                            .font_semibold()
                                            .text_size(px(20.0))
                                            .child("和 Norma 聊聊"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .mr(px(48.0))
                                    .text_size(px(9.0))
                                    .text_color(rgba(GLASS_TEXT_SECONDARY))
                                    .child(
                                        div()
                                            .w(px(4.0))
                                            .h(px(4.0))
                                            .rounded_full()
                                            .bg(rgba(theme.accent_cyan)),
                                    )
                                    .child("在线"),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(5.0))
                            .mb(px(5.0))
                            .ml(px(4.0))
                            .flex()
                            .items_center()
                            .gap(px(1.0))
                            .child(
                                Button::new("desktop-assistant-orb")
                                    .debug_selector(|| "desktop-assistant-orb".to_string())
                                    .w(px(144.0))
                                    .h(px(144.0))
                                    .flex_none()
                                    .ml(px(-8.0))
                                    .rounded_full()
                                    .p(px(0.0))
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(rgba(0x00000000).into())
                                            .foreground(rgba(super::GLASS_TEXT).into())
                                            .border(rgba(0x00000000).into())
                                            .hover(rgba(0x7bc7ff12).into())
                                            .active(rgba(0x7bc7ff25).into()),
                                    )
                                    .tooltip(format!(
                                        "语音对话 · {}",
                                        self.shortcuts.voice_interaction_label()
                                    ))
                                    .on_click(cx.listener(|app, _, window, cx| {
                                        cx.stop_propagation();
                                        app.activate_desktop_assistant_voice(window, cx);
                                    }))
                                    .child(super::orb::orb_art(
                                        self.desktop_workspace.assistant_orb_phase,
                                        self.desktop_workspace.assistant_orb_energy,
                                        self.motion.reduce_motion,
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.0))
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgba(theme.accent_cyan))
                                            .child(
                                                status.unwrap_or_else(|| "随时为你".to_owned()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .font_semibold()
                                            .text_size(px(17.0))
                                            .pr(px(16.0))
                                            .child("我在，随时开口。"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgba(GLASS_TEXT_SECONDARY))
                                            .child("有什么想一起做的？"),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .id("desktop-assistant-conversation")
                    .debug_selector(|| "desktop-assistant-conversation".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&conversation_scroll)
                    .px(px(22.0))
                    .pt(px(8.0))
                    .pb(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .child(super::chat_message::conversation_message(
                        0,
                        Speaker::Norma,
                        "你好，我是 Norma。左边和我聊天，中间看项目画布，右边可用标签浏览网页、文件、图片和视频。".to_owned(),
                        true,
                        width,
                    ))
                    .children(messages.into_iter().enumerate().map(|(index, message)| {
                        super::chat_message::conversation_message(
                            index + 1,
                            message.speaker,
                            message.text,
                            false,
                            width,
                        )
                    })),
            )
    }

    fn render_left_overview_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.focus_status_overview_first_row(window, cx);
        self.ensure_idea_input(window, cx);
        let focus = self.left_overview_focus(cx);
        div()
            .id("desktop-run-overview")
            .debug_selector(|| "desktop-run-overview".to_string())
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .tab_index(0)
            .track_focus(&focus)
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                    app.toggle_desktop_drawer(DesktopDrawerEvent::EscapeOverview, cx);
                    app.focus_left_drawer_view(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if app.handle_run_overview_key(
                    event.keystroke.key.as_str(),
                    event.keystroke.modifiers.modified(),
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                }
            }))
            .child(self.render_overview_drawer_header(cx))
            .child(self.render_run_overview(cx))
    }
}
