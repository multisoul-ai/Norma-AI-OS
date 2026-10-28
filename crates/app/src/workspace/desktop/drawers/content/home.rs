//! Empty right drawer: the prototype's "new tab" page (address pill, tools, recommendations).

use super::home_tools::{HomeTool, Recommendation, PLACEHOLDER_RECOMMENDATIONS, UPCOMING_NOTICE};

use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, Focusable, Stateful, Window};
use std::time::Duration;
use storage::DesktopContentKind;
use ui::component::{
    input::{Input, InputState},
    StyledExt,
};
use ui::icons::with_alpha;

/// Below this width a second tool column would squeeze labels and shortcut hints.
const TWO_COLUMN_MIN_WIDTH: f32 = 330.0;

impl MultisoulApp {
    pub(super) fn render_content_home(
        &mut self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self.desktop_workspace.content_browser.input.is_none()
            || self.desktop_workspace.content_browser.tab_id.is_some()
        {
            self.desktop_workspace.content_browser.input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("搜索或输入网址")));
            self.desktop_workspace.content_browser.tab_id = None;
        }
        let theme = self.desktop_theme();
        let input = self
            .desktop_workspace
            .content_browser
            .input
            .clone()
            .expect("home input");
        let columns = if width >= TWO_COLUMN_MIN_WIDTH { 2 } else { 1 };
        let notice = self.desktop_workspace.content_browser.home_notice.clone();
        let recommendations = self.home_recommendations();
        let heading = |text: &'static str| {
            div()
                .mt(px(6.0))
                .mb(px(10.0))
                .text_size(px(11.0))
                .font_semibold()
                .text_color(rgba(theme.text_secondary))
                .child(text)
        };
        div()
            .id("desktop-content-home")
            .debug_selector(|| "desktop-content-home".into())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .px(px(16.0))
            .pt(px(14.0))
            .pb(px(24.0))
            .child(
                div()
                    .id("desktop-home-address")
                    .debug_selector(|| "desktop-home-address".into())
                    .h(px(36.0))
                    .flex_none()
                    .mb(px(22.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .rounded(px(18.0))
                    .track_focus(&input.read(cx).focus_handle(cx))
                    .border_1()
                    .border_color(rgba(theme.node_stroke))
                    .bg(rgba(with_alpha(theme.edge_highlight, 0x14)))
                    .hover(|style| style.bg(rgba(theme.panel_high)))
                    .focus(|style| style.border_color(rgba(theme.focused_stroke)))
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_primary))
                    .child(div().flex_1().child(Input::new(&input).appearance(false)))
                    .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" {
                            app.open_desktop_url_from_input(cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(heading("工具"))
            .child(self.render_home_tool_grid(columns, cx).mb(px(26.0)))
            .child(heading("推荐"))
            .child(self.render_home_recommendations(recommendations, cx))
            .when_some(notice, |page, text| {
                page.child(
                    div()
                        .id("desktop-home-notice")
                        .debug_selector(|| "desktop-home-notice".into())
                        .absolute()
                        .left(px(16.0))
                        .bottom(px(14.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x40)))
                        .bg(rgba(ui::icons::with_alpha(
                            self.desktop_theme().panel,
                            0xfa,
                        )))
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(text),
                )
            })
    }

    /// Real task results first; remaining slots keep the prototype's upcoming categories.
    fn home_recommendations(&self) -> Vec<Recommendation> {
        let mut slots: Vec<Recommendation> = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .enumerate()
            .take(PLACEHOLDER_RECOMMENDATIONS.len())
            .map(|(index, artifact)| {
                let name = artifact
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "文件".to_owned());
                Recommendation::Artifact(index, name)
            })
            .collect();
        for (icon, label) in PLACEHOLDER_RECOMMENDATIONS.iter().skip(slots.len()) {
            slots.push(Recommendation::Upcoming(icon, label));
        }
        slots
    }

    pub(super) fn activate_home_recommendation(
        &mut self,
        slot: &Recommendation,
        cx: &mut Context<Self>,
    ) {
        match slot {
            Recommendation::Artifact(index, _) => {
                if let Some(artifact) = self.desktop_workspace.content_artifacts.get(*index) {
                    let artifact = artifact.clone();
                    self.open_desktop_artifact(artifact, cx);
                }
            }
            Recommendation::Upcoming(..) => self.show_home_notice(cx),
        }
    }

    pub(crate) fn activate_content_home_tool(
        &mut self,
        tool: HomeTool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match tool {
            HomeTool::Files => self.open_file_workbench(cx),
            HomeTool::Changes => self.open_changes_view(cx),
            HomeTool::Terminal => {
                self.on_new_terminal(&crate::app::NewTerminal, window, cx);
            }
            HomeTool::Web => self.open_desktop_content_tab(
                DesktopContentKind::Browser,
                "about:blank".into(),
                "浏览器".into(),
                None,
                cx,
            ),
            HomeTool::Progress | HomeTool::More => self.show_home_notice(cx),
        }
    }

    /// Inline, non-blocking feedback for entries that are designed but not built yet.
    fn show_home_notice(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.desktop_workspace.content_browser;
        state.home_notice = Some(UPCOMING_NOTICE.to_string());
        state.home_notice_generation = state.home_notice_generation.wrapping_add(1);
        let generation = state.home_notice_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |app, cx| {
                let state = &mut app.desktop_workspace.content_browser;
                if state.home_notice_generation == generation {
                    state.home_notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}
