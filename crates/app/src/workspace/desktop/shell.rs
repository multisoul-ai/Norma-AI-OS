use super::scene::DesktopNavigation;
use crate::app::MultisoulApp;
use crate::shortcuts::ShortcutCommandId;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, ScrollHandle, Window};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Sizable, StyledExt,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};

impl MultisoulApp {
    pub(crate) fn render_desktop_dock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let switcher = self.render_desktop_project_switcher(window, cx);
        let project = self
            .desktop_workspace
            .active_id
            .as_ref()
            .zip(self.desktop_workspace.expanded_area_id.as_ref())
            .and_then(|(workspace_id, area_id)| {
                self.desktop_workspace
                    .sessions
                    .get(&(workspace_id.clone(), area_id.clone()))
                    .cloned()
            });
        if let Some(project) = project {
            project.update(cx, |project, cx| {
                project.sync_command_drawer(window);
                project.start_animation_frame_driver_if_needed(window, cx);
                div()
                    .absolute()
                    .inset_0()
                    .capture_key_down(cx.listener(Self::capture_command_dock_key))
                    .child(project.render_command_dock(window, cx, switcher))
                    .when_some(
                        project.render_command_suggestions(window, cx),
                        |dock, menu| dock.child(menu),
                    )
            })
        } else {
            self.render_command_dock(window, cx, switcher)
        }
    }

    fn render_desktop_project_switcher(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let workspace_id = self.desktop_workspace.active_id.as_ref()?;
        let areas = &self.desktop_workspace.areas;
        if areas.is_empty() {
            return None;
        }
        let selected = match &self.desktop_workspace.pending_navigation {
            Some(DesktopNavigation::Project(id)) => Some(id.as_str()),
            _ => self.desktop_workspace.expanded_area_id.as_deref(),
        };
        let scroll = window.use_keyed_state(
            gpui::ElementId::Name(format!("desktop-project-strip-{workspace_id}").into()),
            cx,
            |_, _| (ScrollHandle::new(), None::<String>),
        );
        let scroll = scroll.update(cx, |(handle, previous), _| {
            if previous.as_deref() != selected {
                if let Some(index) = areas
                    .iter()
                    .position(|area| Some(area.id.as_str()) == selected)
                {
                    handle.scroll_to_item(index);
                }
                *previous = selected.map(str::to_owned);
            }
            handle.clone()
        });
        let theme = self.desktop_theme();
        let disabled = self.desktop_workspace.busy;
        // Four 32px buttons + three 4px gaps; more projects stay reachable by horizontal scroll.
        let numbers_width = 4.0 * 32.0 + 3.0 * 4.0;
        Some(
            div()
                .id("desktop-project-switcher")
                .debug_selector(|| "desktop-project-switcher".to_string())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .px(px(6.0))
                .child(div().w(px(1.0)).h(px(20.0)).bg(rgba(theme.dock_line)))
                .child(
                    div()
                        .id("desktop-project-numbers")
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .max_w(px(numbers_width))
                        .overflow_x_scroll()
                        .track_scroll(&scroll)
                        .children(areas.iter().enumerate().map(|(index, area)| {
                            let number = format!("{:02}", index + 1);
                            let id = format!("toolbar-project-{number}");
                            let area_id = area.id.clone();
                            let is_selected = selected == Some(area.id.as_str());
                            project_switch_button(&id, is_selected, theme, cx)
                                .debug_selector(move || id.clone())
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_semibold()
                                        .text_color(rgba(if is_selected {
                                            theme.accent_cyan
                                        } else {
                                            super::GLASS_TEXT_SECONDARY
                                        }))
                                        .child(number.clone()),
                                )
                                .tooltip(format!("{number} · {}", area.display_name))
                                .disabled(
                                    disabled
                                        || self
                                            .desktop_workspace
                                            .area_errors
                                            .contains_key(&area.id),
                                )
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    if !is_selected {
                                        app.expand_desktop_project_area(
                                            area_id.clone(),
                                            window,
                                            cx,
                                        );
                                    }
                                }))
                        })),
                )
                .child(
                    project_switch_button(
                        "toolbar-project-overview",
                        selected.is_none(),
                        theme,
                        cx,
                    )
                    .debug_selector(|| "toolbar-project-overview".to_string())
                    .child(toolbar_icon(
                        ToolbarIcon::Workspaces,
                        theme.accent_cyan,
                        14.0,
                    ))
                    .tooltip("All projects")
                    .disabled(disabled)
                    .on_click(cx.listener(|app, _, window, cx| {
                        cx.stop_propagation();
                        app.navigate_desktop_shortcut(
                            ShortcutCommandId::ShowProjectOverview,
                            window,
                            cx,
                        );
                    })),
                )
                .into_any_element(),
        )
    }
}

fn project_switch_button(id: &str, selected: bool, theme: ui::Theme, cx: &gpui::App) -> Button {
    Button::new(gpui::ElementId::Name(id.to_owned().into()))
        .compact()
        .with_size(px(32.0))
        .w(px(32.0))
        .h(px(32.0))
        .p(px(0.0))
        .flex_none()
        .rounded(px(9.0))
        .custom(
            ButtonCustomVariant::new(cx)
                .color(
                    rgba(if selected {
                        with_alpha(theme.accent_cyan, 0x24)
                    } else {
                        0x00000000
                    })
                    .into(),
                )
                .border(
                    rgba(if selected {
                        with_alpha(theme.accent_cyan, 0x90)
                    } else {
                        0x00000000
                    })
                    .into(),
                )
                .foreground(rgba(super::GLASS_TEXT).into())
                .hover(rgba(with_alpha(theme.accent_cyan, 0x3c)).into())
                .active(rgba(with_alpha(theme.accent_cyan, 0x62)).into()),
        )
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_command_dock_tests.rs"]
mod tests;
