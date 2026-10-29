//! Left document column of the file workbench: path bar, then line-numbered content.
//!
//! Structure follows the prototype's `.ex-main` / `.ex-doc` / `.ex-path` and `.df-*`:
//! the dark panel is its own surface so the outer shell can keep following the app theme.

use super::text_index::TextIndex;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, uniform_list, AnyElement, ClipboardItem, Context, Div, KeyDownEvent,
    ListHorizontalSizingBehavior, Stateful, Window,
};
use std::rc::Rc;
use storage::DesktopContentTab;
use ui::component::input::Input;

pub(super) const LINE_HEIGHT: f32 = 22.8;
const PATH_BAR_HEIGHT: f32 = 36.0;

type Action = Rc<dyn Fn(&mut MultisoulApp, &mut Window, &mut Context<MultisoulApp>)>;

/// Hover tooltip for plain `div` controls (Button has its own `.tooltip`).
pub(super) fn tooltip_view(
    text: impl Into<gpui::SharedString>,
) -> impl Fn(&mut Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text: gpui::SharedString = text.into();
    move |window, cx| ui::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
}

fn activates(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space")
}

/// Display form of one source line: tabs become spaces, empty lines keep their height.
pub(super) fn display_line(line: &str) -> String {
    if line.is_empty() {
        " ".into()
    } else {
        line.replace('\t', "    ")
    }
}

impl MultisoulApp {
    pub(super) fn render_workbench_document(
        &self,
        tab: &DesktopContentTab,
        width: f32,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.desktop_theme();
        let s = &self.desktop_workspace.workbench;
        let body = if s.diff.is_some() {
            self.render_workbench_diff(cx)
        } else {
            self.render_workbench_file_body(tab, cx)
        };
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .min_w(px(0.0))
            .bg(rgba(theme.canvas_from))
            .text_color(rgba(theme.text_secondary))
            .child(self.render_workbench_path_bar(compact, cx))
            .child(body)
    }

    fn render_workbench_path_bar(&self, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let s = &self.desktop_workspace.workbench;
        let key = s.draft_key();
        let draft = key.as_ref().and_then(|key| s.drafts.get(key));
        let dirty = draft.is_some_and(|d| d.dirty);
        let selected = s.saved.selected.clone();
        let mut bar = div()
            .id("files-path-bar")
            .debug_selector(|| "files-path-bar".to_string())
            .h(px(PATH_BAR_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.0))
            .px(px(10.0))
            .bg(rgba(theme.panel_high))
            .border_b_1()
            .border_color(rgba(theme.node_stroke));
        if compact {
            bar = bar.child(self.tool_button(
                "files-toggle-tree",
                "☰",
                ui::i18n::text("drawers.show_directory"),
                false,
                Rc::new(|app, _, cx| {
                    app.desktop_workspace.workbench.sidebar_open = true;
                    app.hide_desktop_content_browsers();
                    cx.notify();
                }),
                cx,
            ));
        }
        bar = bar.child(
            div()
                .id("files-path-text")
                .flex_1()
                .min_w(px(0.0))
                .px(px(4.0))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(12.0))
                .child(format!(
                    "{}{}",
                    selected
                        .as_deref()
                        .unwrap_or(ui::i18n::text("drawers.open_file")),
                    if dirty { " ●" } else { "" }
                )),
        );
        if s.file.is_some() && draft.is_none() && s.diff.is_none() {
            bar = bar.child(self.tool_button(
                "files-edit",
                "✎",
                ui::i18n::text("drawers.edit"),
                false,
                Rc::new(|app, w, cx| app.edit_workbench_file(w, cx)),
                cx,
            ));
        }
        if draft.is_some() {
            bar = bar.child(self.tool_button(
                "files-save",
                "✓",
                ui::i18n::text("drawers.save"),
                draft.is_none_or(|d| !d.dirty || d.saving),
                Rc::new(|app, _, cx| app.save_workbench_file(cx)),
                cx,
            ));
        }
        if s.preview.is_some() && s.file.is_some() {
            bar = bar.child(self.tool_button(
                "files-preview-mode",
                if s.show_source { "◐" } else { "‹›" },
                if s.show_source {
                    ui::i18n::text("drawers.preview")
                } else {
                    ui::i18n::text("drawers.source")
                },
                false,
                Rc::new(|app, _, cx| {
                    app.desktop_workspace.workbench.show_source ^= true;
                    app.hide_desktop_content_browsers();
                    cx.notify();
                }),
                cx,
            ));
        }
        if s.diff.is_some() {
            bar = bar.child(self.tool_button(
                "files-current-content",
                "⇤",
                ui::i18n::text("drawers.view_current_file"),
                false,
                Rc::new(|app, _, cx| app.load_workbench_file(false, false, cx)),
                cx,
            ));
        }
        if s.file.is_some() {
            bar = bar.child(self.tool_button(
                "files-copy",
                "⧉",
                ui::i18n::text("drawers.copy_all"),
                false,
                Rc::new(|app, _, cx| {
                    if let Some(file) = &app.desktop_workspace.workbench.file {
                        cx.write_to_clipboard(ClipboardItem::new_string(file.text.clone()));
                    }
                }),
                cx,
            ));
        }
        if selected.is_some() {
            bar = bar.child(self.tool_button(
                "files-open-system",
                "↗",
                ui::i18n::text("drawers.open_system"),
                false,
                Rc::new(|app, _, cx| {
                    let s = &app.desktop_workspace.workbench;
                    if let (Some(project), Some(path)) = (&s.project, &s.saved.selected) {
                        if let Ok(url) = url::Url::from_file_path(project.root().join(path)) {
                            cx.open_url(url.as_str());
                        }
                    }
                }),
                cx,
            ));
        }
        bar
    }

    /// Compact 24px glyph control with hover, pressed, keyboard-focus and disabled states.
    fn tool_button(
        &self,
        id: &'static str,
        glyph: &'static str,
        tip: &'static str,
        disabled: bool,
        action: Action,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let key_action = action.clone();
        let base = div()
            .id(id)
            .debug_selector(move || id.to_string())
            .w(px(24.0))
            .h(px(24.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgba(0x00000000))
            .text_size(px(13.0))
            .tooltip(tooltip_view(tip));
        if disabled {
            return base.text_color(rgba(theme.text_tertiary)).child(glyph);
        }
        base.text_color(rgba(theme.text_secondary))
            .cursor_pointer()
            .tab_index(0)
            .hover(|s| {
                s.bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x14)))
                    .text_color(rgba(theme.text_primary))
            })
            .active(|s| s.bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x22))))
            .focus(|s| s.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, w, cx| action(app, w, cx)))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, w, cx| {
                if activates(event) {
                    cx.stop_propagation();
                    key_action(app, w, cx);
                }
            }))
            .child(glyph)
    }

    fn render_workbench_file_body(
        &self,
        tab: &DesktopContentTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.desktop_theme();
        let s = &self.desktop_workspace.workbench;
        let key = s.draft_key();
        let draft = key.as_ref().and_then(|key| s.drafts.get(key));
        if let Some(draft) = draft.filter(|_| s.show_source || s.preview.is_none()) {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .child(Input::new(&draft.input).appearance(false).h_full().w_full())
                .into_any_element();
        }
        if let Some(preview) = &s.preview {
            return div()
                .relative()
                .flex_1()
                .min_h(px(0.0))
                .child(self.browser_scene_element_filling(
                    super::super::super::content_node_id(&tab.id),
                    preview.url.clone(),
                    cx,
                ))
                .into_any_element();
        }
        if s.file.is_some() {
            return self.render_text_lines(cx);
        }
        let message = if let Some(error) = &s.error {
            error.to_string()
        } else if s.loading {
            ui::i18n::text("drawers.opening_file").to_string()
        } else {
            ui::i18n::text("drawers.choose_file").to_string()
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(rgba(if s.error.is_some() {
                theme.destructive
            } else {
                theme.text_tertiary
            }))
            .child(message)
            .into_any_element()
    }

    fn render_text_lines(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.desktop_theme();
        let s = &self.desktop_workspace.workbench;
        if let Some(file) = &s.file {
            let mut cache = s.text_index.borrow_mut();
            if !cache
                .as_ref()
                .is_some_and(|c| c.matches(&file.version, &file.text))
            {
                *cache = Some(TextIndex::new(&file.version, &file.text));
            }
        }
        let (count, longest) = s
            .text_index
            .borrow()
            .as_ref()
            .map_or((1, None), |c| (c.count(), c.longest));
        uniform_list(
            "files-text-lines",
            count,
            cx.processor(move |app, range: std::ops::Range<usize>, _, _| {
                let s = &app.desktop_workspace.workbench;
                let text = s.file.as_ref().map(|f| f.text.as_str()).unwrap_or_default();
                let cache = s.text_index.borrow();
                range
                    .map(|index| {
                        let line = cache.as_ref().map_or("", |c| c.line(text, index));
                        let bg = (s.jump_line == Some(index))
                            .then_some(ui::icons::with_alpha(theme.accent, 0x2e));
                        numbered_line(
                            index + 1,
                            theme.text_tertiary,
                            display_line(line),
                            theme.text_primary,
                            bg,
                        )
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.workbench_text_scroll.clone())
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(longest)
        .font_family("Menlo")
        .text_size(px(12.0))
        .flex_1()
        .min_h(px(0.0))
        .into_any_element()
    }
}

pub(super) fn numbered_line(
    number: usize,
    number_color: u32,
    text: String,
    color: u32,
    bg: Option<u32>,
) -> Div {
    let mut row = div()
        .h(px(LINE_HEIGHT))
        .flex()
        .items_center()
        .min_w_full()
        .child(
            div()
                .w(px(38.0))
                .flex_none()
                .pr(px(12.0))
                .flex()
                .justify_end()
                .text_color(rgba(number_color))
                .child(number.to_string()),
        )
        .child(
            div()
                .pr(px(16.0))
                .whitespace_nowrap()
                .text_color(rgba(color))
                .child(text),
        );
    if let Some(bg) = bg {
        row = row.bg(rgba(bg));
    }
    row
}
