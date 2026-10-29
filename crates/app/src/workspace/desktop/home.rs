use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{div, prelude::*, px, rgba, Context, Div, Entity, Stateful, Window};
use ui::component::input::InputState;

mod catalog;
mod chrome;
pub(super) mod material;
mod menu;
mod motion;
mod navigation;
mod preview;
#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_preview_tests.rs"]
mod preview_tests;
pub(super) use search::home_metadata;
mod search;
mod search_input;
mod welcome;
pub(crate) use material::home_theme;
pub(crate) use search_input::HomeState;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_background_tests.rs"]
mod background_tests;
#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_focus_tests.rs"]
mod focus_tests;
#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_input_tests.rs"]
mod input_tests;
#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_material_tests.rs"]
mod material_tests;

impl MultisoulApp {
    fn home_entries(&self, cx: &gpui::App) -> Vec<catalog::HomeEntry> {
        self.desktop_workspace
            .workspaces
            .iter()
            .map(|workspace| catalog::HomeEntry {
                id: workspace.id.clone(),
                name: workspace.name.clone(),
                project_count: workspace.project_count,
                opened_ms: workspace.last_opened_at_unix_ms,
                running: self
                    .desktop_workspace
                    .sessions
                    .iter()
                    .any(|((id, _), session)| {
                        id == &workspace.id && session.read(cx).runtime.running_terminal_count() > 0
                    }),
            })
            .collect()
    }

    pub(super) fn render_desktop_home(
        &mut self,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = home_theme(self.theme_id);
        let backdrop = material::progress(
            "home-backdrop",
            self.desktop_workspace.home.search_open,
            200,
            self.motion.reduce_motion,
            window,
            cx,
        );
        self.desktop_workspace.home.search_input = Some(input.clone());
        let viewport = window.viewport_size();
        let full = CanvasRect::new(0.0, 0.0, viewport.width.into(), viewport.height.into());
        let frame = self.desktop_scene_frame(full, true);
        let scale = frame.size.x / full.size.x;
        let moving = self.desktop_directory_zoom_active();
        let first = self.desktop_workspace.workspaces.is_empty();
        let top = super::layout::home_brand_top(full.size.y);
        let count = self.desktop_workspace.workspaces.len();
        let mut entries = catalog::recent(&self.home_entries(cx));
        if moving {
            entries = self.home_entries(cx);
            entries.retain(|entry| {
                self.desktop_workspace
                    .workspace_motion_rects
                    .contains_key(&entry.id)
            });
            entries.sort_by(|a, b| {
                let a = self.desktop_workspace.workspace_motion_rects[&a.id];
                let b = self.desktop_workspace.workspace_motion_rects[&b.id];
                a.origin
                    .y
                    .total_cmp(&b.origin.y)
                    .then_with(|| a.origin.x.total_cmp(&b.origin.x))
            });
        }
        let slots = if moving {
            entries
                .iter()
                .map(|entry| {
                    let preview = self.desktop_workspace.workspace_motion_rects[&entry.id];
                    CanvasRect::new(
                        preview.origin.x - 7.0,
                        preview.origin.y - 7.0,
                        preview.size.x + 14.0,
                        204.0,
                    )
                })
                .collect()
        } else {
            super::layout::home_slots(full.size.x, full.size.y, count)
        };
        let scroll = self.desktop_directory_scroll.offset();
        let mut cards = div().relative().w(px(full.size.x * scale)).h(px(slots
            .last()
            .map_or(full.size.y, |r| r.max().y + 40.0)
            .max(full.size.y)
            * scale));
        if !moving {
            self.desktop_workspace
                .workspace_rects
                .lock()
                .expect("workspace bounds mutex poisoned")
                .clear();
        }
        for (index, (entry, rect)) in entries.iter().zip(&slots).enumerate() {
            let Some(workspace) = self
                .desktop_workspace
                .workspaces
                .iter()
                .find(|w| w.id == entry.id)
                .cloned()
            else {
                continue;
            };
            if !moving {
                let mut anchor = super::layout::home_preview_panel_rect(*rect, false);
                anchor.origin.x += 1.0;
                anchor.origin.y += 1.0 + f32::from(scroll.y);
                self.desktop_workspace
                    .workspace_rects
                    .lock()
                    .expect("workspace bounds mutex poisoned")
                    .insert(entry.id.clone(), anchor);
            }
            cards = cards.child(
                div()
                    .absolute()
                    .left(px(rect.origin.x * scale))
                    .top(px(rect.origin.y * scale))
                    .w(px(rect.size.x * scale))
                    .h(px(rect.size.y * scale))
                    .child(self.desktop_home_card(
                        index, workspace, rect.size, full.size, scale, window, cx,
                    )),
            );
        }
        if !moving && (1..=2).contains(&count) {
            if let Some(rect) = slots.last() {
                cards = cards.child(self.desktop_home_create_card(*rect, cx));
            }
        }
        let left = slots.first().map_or(24.0, |rect| rect.origin.x);
        let section_width = (full.size.x - 2.0 * left).max(1.0);
        cards = cards.child(
            div()
                .absolute()
                .left(px(left))
                .top(px(top + 228.0))
                .w(px(section_width))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_secondary))
                .child(ui::i18n::text("workspace.home_recent"))
                .when(count > 2, |row| {
                    row.child(
                        material::button(
                            "desktop-home-all",
                            ui::i18n::format("workspace.home_all", &[("count", count.to_string())]),
                            theme,
                            cx,
                        )
                        .debug_selector(|| "desktop-home-all".into())
                        .tab_stop(!self.desktop_workspace.home.search_open)
                        .h(px(28.0))
                        .on_click(cx.listener({
                            let input = input.clone();
                            move |app, _, window, cx| app.open_home_search(&input, window, cx)
                        })),
                    )
                }),
        );
        let chrome_opacity = self
            .desktop_directory_workspace_id()
            .and_then(|id| self.desktop_directory_chrome_opacity(id, viewport))
            .unwrap_or(1.0);
        let mut page = div()
            .id("desktop-home-world")
            .absolute()
            .left(px(frame.origin.x))
            .top(px(frame.origin.y))
            .w(px(frame.size.x))
            .h(px(frame.size.y))
            .text_color(rgba(theme.text_primary));
        if first {
            page = page.child(self.desktop_home_welcome(full.size, cx));
        } else {
            page = page
                .child(
                    div()
                        .id("desktop-workspace-scroll")
                        .debug_selector(|| "desktop-workspace-scroll".into())
                        .absolute()
                        .inset_0()
                        .when(!moving, |layer| {
                            layer
                                .top(px(top + 218.0))
                                .overflow_y_scroll()
                                .track_scroll(&self.desktop_directory_scroll)
                        })
                        .opacity(1.0 - 0.8 * backdrop)
                        .child(cards.mt(px(if moving { 0.0 } else { -(top + 218.0) }))),
                )
                .child(self.desktop_home_brand(full.size.x, top, 1.0 - 0.55 * backdrop, cx));
        }
        if !moving {
            page = page.child(self.desktop_home_toolbar(full.size.x, cx));
        }
        let root = div()
            .id("desktop-workspace-launchpad")
            .debug_selector(|| "desktop-workspace-launchpad".into())
            .absolute()
            .inset_0()
            .when(!moving, |root| {
                root.child(
                    div()
                        .absolute()
                        .inset_0()
                        .debug_selector(|| "desktop-home-wallpaper".into())
                        .child(self.render_wallpaper_layer())
                        .child(material::scrim(true, 160.0))
                        .child(material::scrim(false, 240.0)),
                )
            })
            .child(page.opacity(chrome_opacity));
        let root = if !first && !moving {
            self.desktop_home_search_layer(root, input, full.size, window, cx)
        } else {
            root
        };
        root.when_some(self.desktop_workspace.error.clone(), |root, error| {
            root.child(
                div()
                    .absolute()
                    .bottom(px(12.0))
                    .left(px(24.0))
                    .right(px(24.0))
                    .p(px(12.0))
                    .rounded(px(12.0))
                    .bg(rgba(0x1e232bfa))
                    .text_color(rgba(theme.destructive))
                    .text_size(px(13.0))
                    .child(error),
            )
        })
    }
}
