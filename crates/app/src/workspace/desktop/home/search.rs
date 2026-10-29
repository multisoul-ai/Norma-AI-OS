use super::*;
use gpui::{Focusable, HighlightStyle, StyledText};
use ui::component::{input::Input, Disableable, Icon, IconName};
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn desktop_home_search_layer(
        &mut self,
        root: Stateful<Div>,
        input: Entity<InputState>,
        size: canvas::Vec2,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = home_theme(self.theme_id);
        let panel_focus = window
            .use_keyed_state("home-search-panel-focus", cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        self.desktop_workspace.home.panel_focus = Some(panel_focus.clone());
        let scroll =
            window.use_keyed_state("home-search-scroll", cx, |_, _| gpui::ScrollHandle::new());
        self.desktop_workspace.home.search_scroll = Some(scroll.clone());
        let scroll = scroll.read(cx).clone();
        let open = self.desktop_workspace.home.search_open;
        let progress = material::progress(
            "home-search-motion",
            open,
            260,
            self.motion.reduce_motion || !open,
            window,
            cx,
        );
        let backdrop = material::progress(
            "home-search-scrim-motion",
            open,
            200,
            self.motion.reduce_motion,
            window,
            cx,
        );
        let top = super::super::layout::home_brand_top(size.y) + 108.0;
        let width = 640.0_f32.min((size.x - 40.0).max(1.0));
        let focused = input.focus_handle(cx).is_focused(window);
        let placeholder = ui::i18n::format(
            "workspace.home_search",
            &[("count", self.desktop_workspace.workspaces.len().to_string())],
        );
        input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
        let mut root = root;
        if open {
            root = root.child(
                div()
                    .id("desktop-home-search-scrim")
                    .debug_selector(|| "desktop-home-search-scrim".into())
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x02060c59))
                    .opacity(backdrop)
                    .occlude()
                    .on_click(cx.listener({
                        let input = input.clone();
                        move |app, _, window, cx| app.close_home_search(&input, window, cx)
                    })),
            );
        }
        let mut panel = div()
            .id("desktop-home-search-panel")
            .occlude()
            .track_focus(&panel_focus)
            .tab_group()
            .capture_action(cx.listener(Self::cancel_home_ime))
            .debug_selector(|| "desktop-workspace-search".into())
            .absolute()
            .left(px((size.x - width) * 0.5))
            .top(px(top))
            .w(px(width))
            .rounded(px(28.0))
            .border_1()
            .border_color(rgba(if focused {
                theme.focused_stroke
            } else {
                0xffffff24
            }))
            .bg(rgba(if open { 0x1e232bfa } else { 0x161c249e }))
            .shadow_md()
            .when(focused, |panel| {
                panel.shadow(material::ring(theme.focused_stroke))
            })
            .when(!open, |panel| panel.hover(|s| s.bg(rgba(0x242a33d1))))
            .child(
                div()
                    .id("desktop-workspace-search")
                    .debug_selector(|| "desktop-home-search-input".into())
                    .w(px(width - 2.0))
                    .h(px(54.0))
                    .pl(px(18.0))
                    .pr(px(if open { 54.0 } else { 18.0 }))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .on_click(cx.listener({
                        let input = input.clone();
                        move |app, _, window, cx| app.open_home_search(&input, window, cx)
                    }))
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(20.0))
                            .text_color(rgba(theme.text_secondary)),
                    )
                    .child(
                        div().flex_1().min_w(px(0.0)).child(
                            Input::new(&input)
                                .tab_index(0)
                                .appearance(false)
                                .text_size(px(16.0))
                                .text_color(rgba(theme.text_primary)),
                        ),
                    ),
            )
            .when(open, |panel| {
                panel.child(
                    div().absolute().top(px(7.0)).right(px(8.0)).child(
                        material::button("desktop-home-search-close", "×", theme, cx)
                            .tab_index(1)
                            .size(px(40.0))
                            .tooltip(ui::i18n::text("workspace.home_search_close"))
                            .on_click(cx.listener({
                                let input = input.clone();
                                move |app, _, window, cx| {
                                    // The close button overlaps the search entry's hit region.
                                    cx.stop_propagation();
                                    app.close_home_search(&input, window, cx);
                                }
                            })),
                    ),
                )
            });
        // Escape/cancel is immediate: clearing the query must not grow an exiting result list.
        if open {
            let groups = self.home_search_groups(cx);
            let total: usize = groups.iter().map(|group| group.entries.len()).sum();
            let height = if total == 0 {
                124.0
            } else {
                total as f32 * 52.0 + groups.len() as f32 * 36.0 + 12.0
            };
            let height = height.min((size.y - top - 116.0).clamp(96.0, 432.0));
            self.desktop_workspace.home.cursor = self
                .desktop_workspace
                .home
                .cursor
                .min(total.saturating_sub(1));
            let mut body = div()
                .id("desktop-home-search-results")
                .debug_selector(|| "desktop-home-search-results".into())
                .w_full()
                .max_h(px((size.y - top - 116.0).clamp(96.0, 432.0)))
                .py(px(6.0))
                .overflow_y_scroll()
                .track_scroll(&scroll);
            let mut index = 0;
            for group in groups {
                let name = match group.kind {
                    catalog::GroupKind::Running => "workspace.running",
                    catalog::GroupKind::Recent => "workspace.home_week",
                    catalog::GroupKind::Older => "workspace.home_older",
                    catalog::GroupKind::Matches => "workspace.search_results",
                };
                body = body.child(
                    div()
                        .h(px(36.0))
                        .px(px(20.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(format!("{} · {}", ui::i18n::text(name), group.total))
                        .when(
                            group.kind == catalog::GroupKind::Older && group.total > 3,
                            |heading| {
                                heading.child(
                                    material::button(
                                        "desktop-home-older",
                                        if self.desktop_workspace.home.older_expanded {
                                            ui::i18n::text("workspace.home_collapse").to_string()
                                        } else {
                                            ui::i18n::format(
                                                "workspace.home_show_all",
                                                &[("count", group.total.to_string())],
                                            )
                                            .to_string()
                                        },
                                        theme,
                                        cx,
                                    )
                                    .tab_index(2)
                                    .debug_selector(|| "desktop-home-older".into())
                                    .h(px(28.0))
                                    .on_click(cx.listener(
                                        |app, _, _, cx| {
                                            app.desktop_workspace.home.older_expanded =
                                                !app.desktop_workspace.home.older_expanded;
                                            app.desktop_workspace.home.cursor = 0;
                                            cx.notify();
                                        },
                                    )),
                                )
                            },
                        ),
                );
                for entry in group.entries {
                    body = body.child(self.desktop_home_search_row(index, entry, &input, cx));
                    index += 1;
                }
            }
            if total == 0 {
                let query = self.desktop_workspace.search_query.trim().to_string();
                body = body.child(
                    div()
                        .debug_selector(|| "desktop-home-search-empty".into())
                        .py(px(24.0))
                        .px(px(20.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(12.0))
                        .text_size(px(14.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(ui::i18n::format(
                            "workspace.home_no_match",
                            &[("name", query.clone())],
                        ))
                        .child(
                            material::button(
                                "desktop-home-create-query",
                                ui::i18n::format("workspace.home_create_query", &[("name", query)]),
                                theme,
                                cx,
                            )
                            .tab_index(2)
                            .debug_selector(|| "desktop-home-create-query".into())
                            .h(px(36.0))
                            .bg(rgba(0xa8c7fa29))
                            .disabled(self.desktop_workspace.busy)
                            .opacity(if self.desktop_workspace.busy {
                                0.45
                            } else {
                                1.0
                            })
                            .on_click(cx.listener(
                                |app, _, window, cx| app.create_home_search_query(window, cx),
                            )),
                        ),
                );
            }
            panel = panel
                .child(div().mx(px(16.0)).h(px(1.0)).bg(rgba(0xffffff14)))
                .child(
                    div()
                        .h(px((height + 37.0) * progress))
                        .overflow_hidden()
                        .child(body)
                        .child(
                            div()
                                .h(px(36.0))
                                .px(px(20.0))
                                .border_t_1()
                                .border_color(rgba(0xffffff10))
                                .flex()
                                .items_center()
                                .text_size(px(11.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child(ui::i18n::text("workspace.home_keys")),
                        ),
                );
        }
        root.child(panel)
    }

    fn desktop_home_search_row(
        &self,
        index: usize,
        entry: catalog::HomeEntry,
        input: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = home_theme(self.theme_id);
        let selected = index == self.desktop_workspace.home.cursor;
        let input = input.clone();
        let highlights =
            catalog::highlight_ranges(&entry.name, &self.desktop_workspace.search_query)
                .into_iter()
                .map(|range| {
                    (
                        range,
                        HighlightStyle {
                            color: Some(rgba(theme.accent).into()),
                            font_weight: Some(gpui::FontWeight::SEMIBOLD),
                            ..Default::default()
                        },
                    )
                });
        let label = StyledText::new(entry.name.clone()).with_highlights(highlights);
        let busy = self.desktop_workspace.busy;
        div()
            .id(("desktop-home-result", index))
            .debug_selector(move || format!("desktop-home-result-{index}"))
            .h(px(52.0))
            .mx(px(8.0))
            .px(px(12.0))
            .rounded(px(14.0))
            .flex()
            .items_center()
            .gap(px(14.0))
            .text_color(rgba(theme.text_primary))
            .cursor_pointer()
            .opacity(if busy { 0.45 } else { 1.0 })
            .bg(rgba(if selected { 0xa8c7fa24 } else { 0x00000000 }))
            .when(!busy, |row| {
                row.hover(|s| s.bg(rgba(0xf1f3f814)))
                    .active(|s| s.bg(rgba(0xf1f3f81f)))
            })
            .child(
                div()
                    .size(px(32.0))
                    .flex_none()
                    .rounded(px(10.0))
                    .bg(rgba(0xe3e3e814))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .child(entry.name.chars().next().unwrap_or(' ').to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(15.0))
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_tertiary))
                    .when(entry.running, |meta| {
                        meta.child(div().size(px(6.0)).rounded_full().bg(rgba(theme.success)))
                    })
                    .child(home_metadata(entry.project_count, entry.opened_ms)),
            )
            .child(
                div()
                    .w(px(20.0))
                    .text_color(rgba(with_alpha(
                        theme.accent,
                        if selected { 255 } else { 0 },
                    )))
                    .child("↩"),
            )
            .on_click(cx.listener(move |app, _, window, cx| {
                if app.desktop_workspace.busy {
                    return;
                }
                app.close_home_search(&input, window, cx);
                app.open_desktop_workspace(entry.id.clone(), window, cx);
            }))
    }
}

pub(crate) fn home_metadata(count: usize, opened: i64) -> String {
    let age = search_input::now_ms().saturating_sub(opened).max(0);
    let time = if age < 60_000 {
        ui::i18n::text("workspace.home_now").to_string()
    } else if age < 86_400_000 {
        ui::i18n::text("workspace.home_today").to_string()
    } else {
        ui::i18n::format(
            "workspace.home_days",
            &[("count", (age / 86_400_000).to_string())],
        )
        .to_string()
    };
    format!(
        "{} · {}",
        ui::i18n::format("workspace.project_count", &[("count", count.to_string())]),
        time
    )
}
