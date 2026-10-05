use super::action::WallpaperAction;
use super::{scoped::ScopedWallpaperState, state::WallpaperSelection};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
use ui::component::{
    button::{Button, ButtonGroup},
    switch::Switch,
    Disableable, Selectable, StyledExt,
};
use ui::Theme;

pub(super) fn scope_controls(
    theme: Theme,
    state: &ScopedWallpaperState,
    compact: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mut content = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .when(compact, |row| row.flex_col().items_start());
    if let Some(context) = &state.context {
        let label = if state.edit_project {
            context
                .area
                .as_ref()
                .map(|area| format!("{} / {}", context.workspace.name, area.display_name))
                .unwrap_or_else(|| context.workspace.name.clone())
        } else {
            context.workspace.name.clone()
        };
        content = content
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(theme.text_secondary))
                            .child(ui::i18n::text("settings.personalize")),
                    )
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_medium()
                            .truncate()
                            .child(label),
                    ),
            )
            .child(
                ButtonGroup::new("wallpaper-scope").outline().children([
                    Button::new("wallpaper-scope-workspace")
                        .w(px(122.0))
                        .rounded(px(18.0))
                        .label(if !state.edit_project {
                            ui::i18n::text("settings.workspace_selected")
                        } else {
                            ui::i18n::text("common.workspace")
                        })
                        .selected(!state.edit_project)
                        .disabled(state.busy || state.delete_prompt.is_some())
                        .on_wallpaper_activate(
                            !state.busy && state.delete_prompt.is_none(),
                            cx.listener(|app, _, _, cx| {
                                app.wallpaper_settings.scoped.edit_project = false;
                                app.wallpaper_settings.scoped.status = None;
                                cx.notify();
                            }),
                        ),
                    Button::new("wallpaper-scope-project")
                        .w(px(108.0))
                        .rounded(px(18.0))
                        .label(if state.edit_project {
                            ui::i18n::text("settings.project_selected")
                        } else {
                            ui::i18n::text("settings.project")
                        })
                        .selected(state.edit_project)
                        .disabled(
                            state.busy || context.area.is_none() || state.delete_prompt.is_some(),
                        )
                        .tooltip(if context.area.is_none() {
                            ui::i18n::text("settings.open_project_wallpaper")
                        } else {
                            ui::i18n::text("settings.project_wallpaper_scope")
                        })
                        .on_wallpaper_activate(
                            !state.busy && context.area.is_some() && state.delete_prompt.is_none(),
                            cx.listener(|app, _, _, cx| {
                                app.wallpaper_settings.scoped.edit_project = true;
                                app.wallpaper_settings.scoped.status = None;
                                cx.notify();
                            }),
                        ),
                ]),
            );
    } else {
        content = content.child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_secondary))
                .child(ui::i18n::text("settings.directory_hint")),
        );
    }
    content
}

pub(super) fn follow_control(
    theme: Theme,
    state: &ScopedWallpaperState,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let follows = state.follows_workspace();
    let enabled = !state.busy && state.delete_prompt.is_none();
    div()
        .id("wallpaper-follow-workspace")
        .w_full()
        .h(px(36.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgba(0x00000000))
        .tab_index(if enabled { 0 } else { -1 })
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .opacity(if enabled { 1.0 } else { 0.6 })
        .when(enabled, |row| {
            row.cursor_pointer()
                .hover(move |style| style.bg(rgba(theme.panel)))
                .active(move |style| style.bg(rgba(theme.node_stroke)))
                .focus(move |style| style.border_color(rgba(theme.accent_blue)))
        })
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(ui::i18n::text("settings.follow_workspace")),
        )
        .child(
            Switch::new("wallpaper-follow-workspace-switch")
                .checked(follows)
                .disabled(!enabled),
        )
        .on_click(cx.listener(move |app, _, _, cx| {
            if enabled {
                app.set_wallpaper_follow_workspace(!follows, cx);
            }
        }))
        .on_key_down(cx.listener(move |app, event: &gpui::KeyDownEvent, _, cx| {
            if enabled && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.set_wallpaper_follow_workspace(!follows, cx);
                cx.stop_propagation();
            }
        }))
}

pub(super) fn notices(theme: Theme, state: &ScopedWallpaperState) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .when(state.has_missing_selection(), |content| {
            content.child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgba(theme.destructive))
                    .child(ui::i18n::text("settings.missing_selection")),
            )
        })
}

pub(super) fn selected_state(
    state: &super::WallpaperSettingsState,
) -> super::WallpaperSettingsState {
    let mut selected = state.clone();
    selected.selection = state.scoped.edit_selection();
    selected
}

pub(super) fn request_builtin(
    app: &mut MultisoulApp,
    rel: &'static str,
    cx: &mut Context<MultisoulApp>,
) {
    if !app.wallpaper_settings.scoped.busy && app.wallpaper_settings.scoped.delete_prompt.is_none()
    {
        app.request_wallpaper_selection(WallpaperSelection::Builtin { rel }, cx);
    }
}
