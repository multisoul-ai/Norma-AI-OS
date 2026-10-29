//! The left drawer: chat and run overview share one composer.

use super::{glass_button_variant, BOTTOM, TOP};
use crate::app::MultisoulApp;
use gpui::{
    div, point, prelude::*, px, rgba, svg, BoxShadow, Context, Div, MouseButton, Stateful, Window,
};
use runtime::desktop_drawers::DesktopDrawerEvent;
use ui::component::{
    button::{Button, ButtonVariants},
    input::Input,
    StyledExt,
};
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn render_assistant_left(
        &mut self,
        width: f32,
        edge_inset: f32,
        opacity: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let frost = self.render_drawer_frost(true, width, edge_inset, window);
        let chat_visible = self.desktop_workspace.left_chat_visible;
        let chat_input = if let Some(project) = self.desktop_drawer_chat_project() {
            project.update(cx, |project, cx| {
                project.ensure_command_input(window, cx);
                project.command_dock.as_ref().map(|dock| dock.input.clone())
            })
        } else {
            self.ensure_command_input(window, cx);
            self.command_dock.as_ref().map(|dock| dock.input.clone())
        };
        let has_draft = self.desktop_assistant_has_draft(cx);
        let send_button = div()
            .id("desktop-assistant-send")
            .tab_index(0)
            .focus(|style| style.border_color(rgba(ui::icons::with_alpha(theme.accent, 0xcc))))
            .debug_selector(|| "desktop-assistant-send".to_string())
            .w(px(32.0))
            .h(px(32.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(ui::icons::with_alpha(theme.accent, 0x16)))
            .bg(rgba(ui::icons::with_alpha(theme.accent, 0x1a)))
            .text_color(rgba(theme.text_secondary))
            .when(has_draft, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(ui::icons::with_alpha(theme.accent, 0x33))))
                    .active(|style| style.bg(rgba(ui::icons::with_alpha(theme.accent, 0x4d))))
                    .on_click(cx.listener(|app, _, window, cx| {
                        cx.stop_propagation();
                        app.submit_desktop_assistant_chat(window, cx);
                    }))
            })
            .when(!has_draft, |button| button.opacity(0.4))
            .child(
                svg()
                    .path("icons/arrow-up.svg")
                    .w(px(15.0))
                    .h(px(15.0))
                    .flex_none()
                    .text_color(rgba(theme.text_secondary)),
            );
        div()
            .id("desktop-assistant-left")
            .debug_selector(|| "desktop-assistant-left".to_string())
            .absolute()
            .left(px(edge_inset))
            .top(px(TOP))
            .bottom(px(BOTTOM))
            .w(px(width))
            .opacity(opacity)
            .flex()
            .flex_col()
            .rounded(px(0.0))
            .overflow_hidden()
            .border_r_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x55)))
            .shadow(vec![BoxShadow {
                color: rgba(crate::workspace::desktop::surface_theme::shadow_color(
                    theme, 0x70,
                ))
                .into(),
                offset: point(px(8.0), px(0.0)),
                blur_radius: px(40.0),
                spread_radius: px(0.0),
            }])
            .text_color(rgba(theme.text_primary))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(frost)
            .child(
                div()
                    .id("desktop-assistant-left-header")
                    .debug_selector(|| "desktop-assistant-left-header".to_string())
                    .h(px(crate::app::TOP_BAR_HEIGHT))
                    .flex()
                    .items_center()
                    .pl(px(crate::app::BRAND_TOGGLE_LEFT))
                    .pr(px(12.0))
                    .gap(px(10.0))
                    .border_b_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x36)))
                    .bg(rgba(with_alpha(theme.canvas_from, 0x50)))
                    .child(
                        Button::new("desktop-close-left-drawer")
                            .debug_selector(|| "desktop-close-left-drawer".to_string())
                            .w(px(32.0))
                            .h(px(32.0))
                            .p(px(0.0))
                            .child(
                                svg()
                                    .path("icons/panel-left-close.svg")
                                    .text_color(rgba(theme.text_primary))
                                    .w(px(17.0))
                                    .h(px(17.0)),
                            )
                            .custom(glass_button_variant(theme, cx))
                            .tooltip(ui::i18n::text("drawers.collapse_chat"))
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("desktop-assistant-brand")
                            .debug_selector(|| "desktop-assistant-brand".to_string())
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(crate::app::brand_logo())
                            .child(
                                div()
                                    .debug_selector(|| "desktop-brand-wordmark".to_string())
                                    .font_semibold()
                                    .text_size(px(15.0))
                                    .child("Norma"),
                            ),
                    ),
            )
            .when(chat_visible, |this| {
                this.child(self.render_left_drawer_tabs(cx))
            })
            .child(self.render_left_drawer_body(width, window, cx))
            .when(chat_visible, |this| {
                this.child(
                    div()
                        .id("desktop-assistant-chat-input")
                        .debug_selector(|| "desktop-assistant-chat-input".to_string())
                        .h(px(52.0))
                        .mx(px(18.0))
                        .mb(px(12.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(26.0))
                        .border_1()
                        .border_color(rgba(ui::icons::with_alpha(theme.text_primary, 0x1f)))
                        .bg(rgba(theme.panel_high))
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|app, _, _, cx| {
                                app.mark_desktop_assistant_chat_input(cx);
                                cx.stop_propagation();
                            }),
                        )
                        .when_some(chat_input, |composer, input| {
                            composer.child(
                                div().flex_1().min_w(px(0.0)).child(
                                    Input::new(&input).appearance(false).text_size(px(12.0)),
                                ),
                            )
                        })
                        .child(
                            div()
                                .id("desktop-assistant-voice")
                                .tab_index(0)
                                .border_1()
                                .border_color(rgba(0x00000000))
                                .focus(|style| {
                                    style.border_color(rgba(ui::icons::with_alpha(
                                        theme.accent,
                                        0xcc,
                                    )))
                                })
                                .debug_selector(|| "desktop-assistant-voice".to_string())
                                .w(px(34.0))
                                .h(px(34.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(9.0))
                                .cursor_pointer()
                                .hover(|style| {
                                    style.bg(rgba(ui::icons::with_alpha(theme.accent, 0x13)))
                                })
                                .active(|style| {
                                    style.bg(rgba(ui::icons::with_alpha(theme.accent, 0x26)))
                                })
                                .tooltip({
                                    let label = ui::i18n::format(
                                        "drawers.voice_chat",
                                        &[(
                                            "arg0",
                                            (self.shortcuts.voice_interaction_label()).to_string(),
                                        )],
                                    );
                                    move |window, cx| {
                                        ui::component::tooltip::Tooltip::new(label.clone())
                                            .build(window, cx)
                                    }
                                })
                                .on_click(cx.listener(|app, _, window, cx| {
                                    app.activate_desktop_assistant_voice(window, cx);
                                }))
                                .child(
                                    svg()
                                        .path("icons/microphone.svg")
                                        .w(px(17.0))
                                        .h(px(17.0))
                                        .text_color(rgba(theme.text_secondary)),
                                ),
                        )
                        .child(send_button),
                )
            })
    }

    /// 输入框里有没有待发送的文字（决定发送按钮是否可用）。
    fn desktop_assistant_has_draft(&self, cx: &gpui::App) -> bool {
        let has = |app: &MultisoulApp| {
            app.command_dock
                .as_ref()
                .is_some_and(|dock| !dock.input.read(cx).value().trim().is_empty())
        };
        match self.expanded_desktop_project() {
            Some(project) => has(project.read(cx)),
            None => has(self),
        }
    }

    /// 发送按钮与回车等价：交给命令栏的同一条执行路径。
    fn submit_desktop_assistant_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(project) = self.expanded_desktop_project() {
            project.update(cx, |project, cx| {
                project.execute_dock_suggestion(window, cx)
            });
        } else {
            self.execute_dock_suggestion(window, cx);
        }
    }

    fn mark_desktop_assistant_chat_input(&mut self, cx: &mut Context<Self>) {
        if let Some(project) = self.expanded_desktop_project() {
            project.update(cx, |project, cx| {
                if let Some(dock) = &mut project.command_dock {
                    dock.sidebar_origin = true;
                }
                cx.notify();
            });
        } else if let Some(dock) = &mut self.command_dock {
            dock.sidebar_origin = true;
        }
        cx.notify();
    }

    pub(super) fn activate_desktop_assistant_voice(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.expanded_desktop_project() {
            project.update(cx, |project, cx| {
                project.activate_desktop_assistant_voice_for_session(window, cx);
            });
        } else {
            self.activate_desktop_assistant_voice_for_session(window, cx);
        }
    }

    fn activate_desktop_assistant_voice_for_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.voice_settings.readiness(),
            crate::settings::voice::VoiceEntryState::Ready
        ) {
            self.activate_ready_voice(true, window, cx);
        } else {
            self.open_voice_settings(window, cx);
        }
    }

    pub(super) fn desktop_voice_presentation(
        &self,
        cx: &gpui::App,
    ) -> (Option<String>, Option<String>, Option<String>) {
        let snapshot = |source: &MultisoulApp| {
            if let Some(dock) = source.command_dock.as_ref().filter(|dock| {
                dock.sidebar_origin && (dock.reply.is_some() || dock.error.is_some())
            }) {
                return (
                    Some(dock.draft.clone()),
                    dock.reply
                        .as_ref()
                        .or(dock.error.as_ref())
                        .map(ToString::to_string),
                    Some(ui::i18n::text("drawers.text_chat").to_owned()),
                );
            }
            if source.voice_settings.values().desktop_mode()
                == crate::settings::voice::DesktopVoiceMode::Doubao
            {
                return (
                    source.doubao.last_transcript.clone(),
                    source.doubao.last_reply.clone(),
                    Some(source.doubao.label().to_owned()),
                );
            }
            (
                source
                    .voice_caption_text()
                    .or_else(|| source.dialogue.pending_transcript().map(ToOwned::to_owned)),
                source.dialogue.last_reply_text().map(ToOwned::to_owned),
                source.dialogue.dialogue_voice_label(),
            )
        };
        match self.desktop_drawer_chat_project() {
            Some(project) => snapshot(&project.read(cx)),
            None => snapshot(self),
        }
    }

    pub(crate) fn desktop_assistant_orb_mode(&self, cx: &gpui::App) -> super::orb::OrbMode {
        use super::orb::OrbMode;
        let source = self.desktop_drawer_chat_project();
        let source = source.as_ref().map(|project| project.read(cx));
        let source = source.as_deref().unwrap_or(self);
        match source.voice_waveform_state() {
            None => OrbMode::Idle,
            Some(crate::app::VoiceWaveformState::Listening) => OrbMode::Listening,
            Some(crate::app::VoiceWaveformState::Thinking) => OrbMode::Thinking,
            Some(crate::app::VoiceWaveformState::Replying) => OrbMode::Speaking,
        }
    }
}
