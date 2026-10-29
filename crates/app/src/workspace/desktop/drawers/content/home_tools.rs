//! Tool tiles and recommendation slots of the right drawer's empty "new tab" page.
//!
//! 参考原型 `.nt-tool` / `.nt-rec`：40px 磁贴、左对齐图标 + 名称 + 快捷键，按下即时变色。
//! 尚未实现的能力保持同样外观，点击只给出就地提示，不打开标签也不改变布局。

use super::{GLASS_TEXT, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, ElementId, KeyDownEvent, Stateful};
use ui::icons::with_alpha;

pub(super) const UPCOMING_NOTICE_KEY: &str = "drawers.coming_soon";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HomeTool {
    Changes,
    Terminal,
    Files,
    Progress,
    Web,
    More,
}

impl HomeTool {
    /// Prototype order: two columns, row by row.
    pub(super) const ALL: [HomeTool; 6] = [
        HomeTool::Changes,
        HomeTool::Terminal,
        HomeTool::Files,
        HomeTool::Progress,
        HomeTool::Web,
        HomeTool::More,
    ];

    pub(crate) fn selector(self) -> &'static str {
        match self {
            Self::Changes => "desktop-home-tool-changes",
            Self::Terminal => "desktop-home-tool-terminal",
            Self::Files => "desktop-home-tool-files",
            Self::Progress => "desktop-home-tool-progress",
            Self::Web => "desktop-home-tool-web",
            Self::More => "desktop-home-tool-more",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Changes => ui::i18n::text("drawers.changes"),
            Self::Terminal => ui::i18n::text("drawers.terminal"),
            Self::Files => ui::i18n::text("drawers.files"),
            Self::Progress => ui::i18n::text("drawers.project_progress"),
            Self::Web => ui::i18n::text("drawers.web"),
            Self::More => ui::i18n::text("drawers.more_tools"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Changes => "icons/diff.svg",
            Self::Terminal => "icons/toolbar-terminal.svg",
            Self::Files => "icons/folder.svg",
            Self::Progress => "icons/activity.svg",
            Self::Web => "icons/globe.svg",
            Self::More => "icons/grid.svg",
        }
    }

    /// Shown as the tile's trailing kbd; only tools that own a registered shortcut.
    fn shortcut(self, app: &MultisoulApp) -> Option<String> {
        use crate::shortcuts::ShortcutCommandId as Id;
        let command = match self {
            Self::Changes => Id::ShowChanges,
            Self::Files => Id::OpenFileWorkbench,
            Self::Terminal => Id::NewTerminal,
            _ => return None,
        };
        app.shortcuts.current_keystroke(command).map(symbol_label)
    }
}

/// macOS-style label such as ⌃⇧G, matching the prototype's kbd text.
fn symbol_label(source: &str) -> String {
    let Ok(keystroke) = gpui::Keystroke::parse(source) else {
        return source.to_string();
    };
    let m = keystroke.modifiers;
    let mut label = String::new();
    for (on, symbol) in [
        (m.control, "⌃"),
        (m.alt, "⌥"),
        (m.shift, "⇧"),
        (m.platform, "⌘"),
    ] {
        if on {
            label.push_str(symbol);
        }
    }
    label.push_str(&keystroke.key.to_uppercase());
    label
}

/// Recommendation slot: real recent work when available, otherwise an upcoming placeholder.
pub(super) enum Recommendation {
    Artifact(usize, String),
    Upcoming(&'static str, &'static str),
}

pub(super) const PLACEHOLDER_RECOMMENDATIONS: [(&str, &str); 4] = [
    ("icons/globe.svg", "drawers.web"),
    ("icons/file-document.svg", "drawers.files"),
    ("icons/image.svg", "drawers.images"),
    ("icons/video.svg", "drawers.videos"),
];

impl MultisoulApp {
    pub(super) fn render_home_tool_grid(&self, columns: usize, cx: &mut Context<Self>) -> Div {
        let mut grid = div().flex().flex_col().gap(px(7.0));
        for row in HomeTool::ALL.chunks(columns) {
            grid = grid.child(
                div().flex().gap(px(7.0)).children(
                    row.iter()
                        .map(|tool| self.render_home_tool(*tool, cx).into_any_element()),
                ),
            );
        }
        grid
    }

    fn render_home_tool(&self, tool: HomeTool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let hover = with_alpha(theme.edge_highlight, 0x24);
        let pressed = with_alpha(theme.edge_highlight, 0x3a);
        let kbd = tool.shortcut(self);
        let mut tile = div()
            .id(ElementId::Name(tool.selector().into()))
            .debug_selector(|| tool.selector().to_string())
            .flex_1()
            .min_w(px(0.0))
            .h(px(40.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(9.0))
            .bg(rgba(with_alpha(theme.edge_highlight, 0x14)))
            .border_1()
            .border_color(rgba(0x00000000))
            .text_size(px(12.0))
            .text_color(rgba(GLASS_TEXT))
            .cursor_pointer()
            .tab_index(0)
            .hover(move |style| style.bg(rgba(hover)))
            .active(move |style| style.bg(rgba(pressed)))
            .focus(move |style| style.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, window, cx| {
                app.activate_content_home_tool(tool, window, cx)
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    app.activate_content_home_tool(tool, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                svg()
                    .path(tool.icon())
                    .w(px(15.0))
                    .h(px(15.0))
                    .flex_none()
                    .text_color(rgba(GLASS_TEXT_SECONDARY)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(tool.label()),
            );
        if let Some(label) = kbd {
            tile = tile.child(
                div()
                    .flex_none()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(6.0))
                    .bg(rgba(with_alpha(theme.edge_highlight, 0x18)))
                    .font_family("Menlo")
                    .text_size(px(10.0))
                    .text_color(rgba(GLASS_TEXT_SECONDARY))
                    .child(label),
            );
        }
        tile
    }

    pub(super) fn render_home_recommendations(
        &self,
        slots: Vec<Recommendation>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.desktop_theme();
        let hover = with_alpha(theme.edge_highlight, 0x24);
        let pressed = with_alpha(theme.edge_highlight, 0x3a);
        div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(slots.into_iter().enumerate().map(move |(index, slot)| {
                let (icon, label) = match &slot {
                    Recommendation::Artifact(_, name) => ("icons/file-document.svg", name.clone()),
                    Recommendation::Upcoming(icon, label) => {
                        (*icon, ui::i18n::text(label).to_string())
                    }
                };
                div()
                    .id(ElementId::Name(format!("desktop-home-rec-{index}").into()))
                    .debug_selector(move || format!("desktop-home-rec-{index}"))
                    .w(px(96.0))
                    .pt(px(14.0))
                    .pb(px(12.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgba(0x00000000))
                    .text_size(px(11.0))
                    .text_color(rgba(GLASS_TEXT_SECONDARY))
                    .cursor_pointer()
                    .tab_index(0)
                    .hover(move |style| style.bg(rgba(hover)))
                    .active(move |style| style.bg(rgba(pressed)))
                    .focus(move |style| style.border_color(rgba(theme.focused_stroke)))
                    .on_click(
                        cx.listener(move |app, _, _, cx| {
                            app.activate_home_recommendation(&slot, cx)
                        }),
                    )
                    .child(
                        div()
                            .size(px(38.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(11.0))
                            .bg(rgba(with_alpha(theme.edge_highlight, 0x1c)))
                            .child(
                                svg()
                                    .path(icon)
                                    .w(px(19.0))
                                    .h(px(19.0))
                                    .text_color(rgba(GLASS_TEXT)),
                            ),
                    )
                    .child(
                        div()
                            .max_w_full()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(label),
                    )
            }))
    }
}
