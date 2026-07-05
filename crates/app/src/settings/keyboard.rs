use crate::app::MultisoulApp;
use crate::shortcuts::{
    keystroke_label, read_only_behaviors, EffectiveShortcutEntry, ShortcutCommandId,
    ShortcutSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use ui::Theme;

use super::super::with_alpha;

const COMMAND_CATEGORIES: &[&str] = &["Workspace", "Cards", "Layout", "Settings"];

pub(super) fn keyboard_settings(
    theme: Theme,
    shortcuts: &ShortcutSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(18.0))
        .child(status_message(theme, shortcuts))
        .child(restore_all_row(theme, cx))
        .children(
            COMMAND_CATEGORIES
                .iter()
                .copied()
                .map(|category| command_group(theme, category, shortcuts, cx)),
        )
        .child(read_only_group(theme))
}

fn restore_all_row(theme: Theme, cx: &mut Context<MultisoulApp>) -> Div {
    div().w_full().flex().justify_end().child(
        div()
            .id("settings-shortcuts-restore-all")
            .px(px(14.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x32)))
            .bg(rgba(with_alpha(theme.panel_high, 0x7a)))
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba(theme.text_secondary))
            .cursor_pointer()
            .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xa8))))
            .on_click(cx.listener(|app, _, _window, cx| {
                app.shortcuts.restore_all_defaults();
                cx.notify();
            }))
            .child("Restore Defaults"),
    )
}

fn command_group(
    theme: Theme,
    category: &'static str,
    shortcuts: &ShortcutSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(group_title(theme, category))
        .children(
            shortcuts
                .effective
                .iter()
                .filter(move |entry| entry.category == category)
                .map(|entry| command_row(theme, entry, shortcuts.recording, cx)),
        )
}

fn read_only_group(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(group_title(theme, "Input Behavior"))
        .children(read_only_behaviors().iter().map(|behavior| {
            row_shell(theme)
                .child(label_block(theme, behavior.title, behavior.hint))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(10.0))
                        .child(scope_badge(theme, behavior.scope))
                        .child(keycap(theme, &keystroke_label(Some(behavior.keystroke)))),
                )
        }))
}

fn command_row(
    theme: Theme,
    entry: &EffectiveShortcutEntry,
    recording: Option<ShortcutCommandId>,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let command = entry.command;
    let label = if recording == Some(command) {
        "Type shortcut".to_string()
    } else {
        keystroke_label(entry.current_keystroke.as_deref())
    };
    let mut controls = div().flex().flex_row().items_center().gap(px(10.0)).child(
        keycap(theme, &label)
            .id(shortcut_row_id(command))
            .cursor_pointer()
            .hover(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x1e))))
            .on_click(cx.listener(move |app, _, _window, cx| {
                app.shortcuts.begin_recording(command);
                cx.notify();
            })),
    );

    if entry.modified {
        controls = controls
            .child(modified_badge(theme))
            .child(restore_button(theme, command, cx));
    }

    row_shell(theme)
        .child(label_block(theme, entry.title, entry.hint))
        .child(controls)
}

fn restore_button(
    theme: Theme,
    command: ShortcutCommandId,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    div()
        .id(shortcut_restore_id(command))
        .px(px(10.0))
        .py(px(7.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x2c)))
        .text_size(px(12.0))
        .text_color(rgba(theme.text_secondary))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0x94))))
        .on_click(cx.listener(move |app, _, _window, cx| {
            app.shortcuts.restore_default(command);
            cx.notify();
        }))
        .child("Restore")
}

fn status_message(theme: Theme, shortcuts: &ShortcutSettingsState) -> Div {
    let Some(message) = shortcuts.status.message.as_deref() else {
        return div();
    };
    div()
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(
            if shortcuts.status.is_error {
                theme.destructive
            } else {
                theme.accent_cyan
            },
            0x42,
        )))
        .bg(rgba(with_alpha(
            if shortcuts.status.is_error {
                theme.destructive
            } else {
                theme.accent_cyan
            },
            0x12,
        )))
        .text_size(px(13.0))
        .text_color(rgba(if shortcuts.status.is_error {
            theme.destructive
        } else {
            theme.accent_cyan
        }))
        .child(message.to_string())
}

fn group_title(theme: Theme, title: &'static str) -> Div {
    div()
        .pt(px(18.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .text_size(px(14.0))
        .font_weight(FontWeight::BOLD)
        .text_color(rgba(theme.text_tertiary))
        .child(title)
}

fn row_shell(theme: Theme) -> Div {
    div()
        .min_h(px(58.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(20.0))
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(8.0))
        .bg(rgba(with_alpha(theme.panel_high, 0x54)))
}

fn label_block(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_tertiary))
                .child(hint),
        )
}

fn keycap(theme: Theme, label: &str) -> Div {
    div()
        .min_w(px(88.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .px(px(12.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x40)))
        .bg(rgba(with_alpha(theme.node_fill, 0xc6)))
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(theme.text_primary))
        .child(label.to_string())
}

fn modified_badge(theme: Theme) -> Div {
    div()
        .px(px(8.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .bg(rgba(with_alpha(theme.accent_orange, 0x18)))
        .text_size(px(12.0))
        .text_color(rgba(theme.accent_orange))
        .child("Modified")
}

fn scope_badge(theme: Theme, label: &'static str) -> Div {
    div()
        .px(px(8.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .bg(rgba(with_alpha(theme.accent_cyan, 0x14)))
        .text_size(px(12.0))
        .text_color(rgba(theme.accent_cyan))
        .child(label)
}

fn shortcut_row_id(command: ShortcutCommandId) -> &'static str {
    match command {
        ShortcutCommandId::NewWindow => "settings-shortcut-new-window",
        ShortcutCommandId::NewTerminal => "settings-shortcut-new-terminal",
        ShortcutCommandId::NewBrowser => "settings-shortcut-new-browser",
        ShortcutCommandId::CopyCardContent => "settings-shortcut-copy-card-content",
        ShortcutCommandId::OpenProject => "settings-shortcut-open-project",
        ShortcutCommandId::OpenPreviewFile => "settings-shortcut-open-preview-file",
        ShortcutCommandId::NewClaudeAgent => "settings-shortcut-new-claude-agent",
        ShortcutCommandId::NewCodexAgent => "settings-shortcut-new-codex-agent",
        ShortcutCommandId::NewOpenCodeAgent => "settings-shortcut-new-opencode-agent",
        ShortcutCommandId::DeleteCard => "settings-shortcut-delete-card",
        ShortcutCommandId::ReflowLayout => "settings-shortcut-reflow-layout",
        ShortcutCommandId::ToggleMasterLayout => "settings-shortcut-toggle-master-layout",
        ShortcutCommandId::ExitFocus => "settings-shortcut-exit-focus",
        ShortcutCommandId::ToggleSettings => "settings-shortcut-toggle-settings",
    }
}

fn shortcut_restore_id(command: ShortcutCommandId) -> &'static str {
    match command {
        ShortcutCommandId::NewWindow => "settings-shortcut-restore-new-window",
        ShortcutCommandId::NewTerminal => "settings-shortcut-restore-new-terminal",
        ShortcutCommandId::NewBrowser => "settings-shortcut-restore-new-browser",
        ShortcutCommandId::CopyCardContent => "settings-shortcut-restore-copy-card-content",
        ShortcutCommandId::OpenProject => "settings-shortcut-restore-open-project",
        ShortcutCommandId::OpenPreviewFile => "settings-shortcut-restore-open-preview-file",
        ShortcutCommandId::NewClaudeAgent => "settings-shortcut-restore-new-claude-agent",
        ShortcutCommandId::NewCodexAgent => "settings-shortcut-restore-new-codex-agent",
        ShortcutCommandId::NewOpenCodeAgent => "settings-shortcut-restore-new-opencode-agent",
        ShortcutCommandId::DeleteCard => "settings-shortcut-restore-delete-card",
        ShortcutCommandId::ReflowLayout => "settings-shortcut-restore-reflow-layout",
        ShortcutCommandId::ToggleMasterLayout => "settings-shortcut-restore-toggle-master-layout",
        ShortcutCommandId::ExitFocus => "settings-shortcut-restore-exit-focus",
        ShortcutCommandId::ToggleSettings => "settings-shortcut-restore-toggle-settings",
    }
}
