use super::style::{guide_stroke, with_alpha, FeatureGuidePalette};
use gpui::{div, prelude::*, px, rgba, AnyElement, FontWeight};
use ui::Theme;

pub(super) struct CheatRow {
    pub(super) keys: &'static [&'static str],
    pub(super) label_key: &'static str,
}

pub(super) struct CheatGroup {
    pub(super) title_key: &'static str,
    pub(super) rows: &'static [CheatRow],
}

const fn row(keys: &'static [&'static str], label_key: &'static str) -> CheatRow {
    CheatRow { keys, label_key }
}

/// 键位按默认值展示，与 `docs/references/keyboard-shortcuts.md` 保持一致。
pub(super) const GROUPS: [CheatGroup; 4] = [
    CheatGroup {
        title_key: "guide.cheat_group_workspace",
        rows: &[
            row(&["⌘N"], "guide.cheat_new_workspace"),
            row(&["⇧⌘K"], "guide.cheat_workspace_switcher"),
            row(&["⇧⌘←", "⇧⌘→"], "guide.cheat_workspace_prev_next"),
            row(&["⇧⌘↑"], "guide.cheat_all_workspaces"),
        ],
    },
    CheatGroup {
        title_key: "guide.cheat_group_project",
        rows: &[
            row(&["⇧⌘A"], "guide.cheat_add_project"),
            row(&["⌘←", "⌘→"], "guide.cheat_project_prev_next"),
            row(&["⌘↓"], "guide.cheat_project_open"),
            row(&["⌘↑"], "guide.cheat_project_overview"),
        ],
    },
    CheatGroup {
        title_key: "guide.cheat_group_cards",
        rows: &[
            row(&["⌘1"], "guide.cheat_new_terminal"),
            row(&["⌘2", "…", "⌘9"], "guide.cheat_new_agents"),
        ],
    },
    CheatGroup {
        title_key: "guide.cheat_group_ideas",
        rows: &[
            row(&["⌘⇧I"], "guide.cheat_open_ideas"),
            row(&["⌃1", "…", "⌃9"], "guide.cheat_assign_card"),
            row(&["⌃0"], "guide.cheat_assign_default"),
            row(&["⌃⇧2", "…", "⌃⇧9"], "guide.cheat_assign_typed"),
            row(&["⌃⇧0"], "guide.cheat_assign_picker"),
            row(&["⌥↑", "⌥↓"], "guide.cheat_idea_reorder"),
        ],
    },
];

pub(super) fn render_cheat_sheet(
    compact: bool,
    theme: Theme,
    palette: FeatureGuidePalette,
) -> AnyElement {
    let column = |groups: &'static [CheatGroup]| {
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .children(
                groups
                    .iter()
                    .map(|group| render_group(group, theme, palette)),
            )
    };
    let body = if compact {
        column(&GROUPS)
    } else {
        div()
            .flex()
            .gap(px(24.0))
            .child(column(&GROUPS[..2]))
            .child(column(&GROUPS[2..]))
    };
    div()
        .id("feature-guide-cheat-sheet")
        .debug_selector(|| "feature-guide-cheat-sheet".to_string())
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(body)
        .into_any_element()
}

fn render_group(
    group: &'static CheatGroup,
    theme: Theme,
    palette: FeatureGuidePalette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(guide_stroke(theme, 0x68)))
        .bg(rgba(with_alpha(theme.graphite, 0x0a)))
        .child(
            div()
                .pb(px(4.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(palette.accent_text))
                .child(ui::i18n::text(group.title_key)),
        )
        .children(group.rows.iter().map(|row| render_row(row, theme, palette)))
        .into_any_element()
}

fn render_row(row: &'static CheatRow, theme: Theme, palette: FeatureGuidePalette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .min_h(px(26.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(12.0))
                .text_color(rgba(palette.primary_text))
                .child(ui::i18n::text(row.label_key)),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .gap(px(4.0))
                .children(row.keys.iter().map(|key| key_chip(key, theme, palette))),
        )
        .into_any_element()
}

fn key_chip(key: &'static str, theme: Theme, palette: FeatureGuidePalette) -> AnyElement {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(rgba(guide_stroke(theme, 0x80)))
        .bg(rgba(with_alpha(theme.graphite, 0x14)))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(palette.supporting_text))
        .child(key)
        .into_any_element()
}
