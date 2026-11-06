use super::{actions::IslandTarget, controller::IslandController};
use crate::app::AppScreen;
use gpui::Context;
use runtime::island::IslandEvent;
use std::time::Duration;

enum Destination {
    Agent(u64),
    Artifact(norma::mobile::RunOverviewAttention),
}

impl IslandController {
    pub fn open_staff_settings(&mut self, cx: &mut Context<Self>) {
        let Some(host) = crate::workspace::desktop::sessions::primary_host(cx) else {
            return;
        };
        let Ok(handle) = crate::workspace::windowing::reopen_desktop_host(&host, cx) else {
            return;
        };
        cx.activate(true);
        let _ = handle.update(cx, |_, window, cx| {
            host.update(cx, |host, cx| {
                if !host.settings_panel.open {
                    host.toggle_settings_panel(cx);
                }
                host.select_settings_section(
                    crate::settings::panel::SettingsSection::NormaBrain,
                    window,
                    cx,
                );
                host.focus_handle.focus(window);
            })
        });
        self.event(
            self.displays.active_display().unwrap_or_default(),
            IslandEvent::Located,
            cx,
        );
    }

    pub fn locate(&mut self, target: IslandTarget, node: u64, cx: &mut Context<Self>) {
        self.navigate(target, Destination::Agent(node), cx);
    }

    pub fn open_artifact(
        &mut self,
        target: IslandTarget,
        row: norma::mobile::RunOverviewAttention,
        cx: &mut Context<Self>,
    ) {
        self.navigate(target, Destination::Artifact(row), cx);
    }

    fn navigate(&mut self, target: IslandTarget, destination: Destination, cx: &mut Context<Self>) {
        let Some(host) = target.host(cx) else {
            self.location_failed(cx);
            return;
        };
        if let Destination::Agent(node) = &destination {
            if target
                .session(cx)
                .is_some_and(|session| session.read(cx).runtime.document.node(*node).is_none())
            {
                self.location_failed(cx);
                return;
            }
        }
        let Ok(window) = crate::workspace::windowing::reopen_desktop_host(&host, cx) else {
            self.location_failed(cx);
            return;
        };
        cx.activate(true);
        self.navigation = self.navigation.wrapping_add(1);
        let navigation = self.navigation;
        let _ = window.update(cx, |_, window, cx| {
            host.update(cx, |host, cx| {
                if host.desktop_workspace.active_id.as_deref() != Some(&target.workspace)
                    || host.screen != AppScreen::DesktopWorkspace
                {
                    host.open_desktop_workspace(target.workspace.clone(), window, cx);
                }
            })
        });
        cx.spawn(async move |this, cx| {
            // Waiting is only for the existing background workspace load/camera transition.
            // A newer locate cancels this navigation before it can select another card.
            for _ in 0..400 {
                if !this
                    .update(cx, |this, _| this.navigation == navigation)
                    .unwrap_or(false)
                {
                    return;
                }
                let outcome = window.update(cx, |_, window, cx| {
                    host.update(cx, |host, cx| {
                        if !host.island_navigation_ready(&target.workspace, &target.area) {
                            return None;
                        }
                        Some(match &destination {
                            Destination::Agent(node) => host
                                .norma_locate_workspace_agent(
                                    crate::workspace::desktop::AgentLocation {
                                        workspace_id: &target.workspace,
                                        area_id: &target.area,
                                        node_id: *node,
                                        approval_run_id: None,
                                        approval_turn_id: None,
                                    },
                                    window,
                                    cx,
                                )
                                .is_ok(),
                            Destination::Artifact(row) => {
                                if host.desktop_workspace.artifact_refreshing
                                    || host.desktop_workspace.artifact_revision == 0
                                {
                                    return None;
                                }
                                host.open_island_artifact(&target.workspace, row, window, cx)
                            }
                        })
                    })
                });
                match outcome {
                    Ok(None) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(25))
                            .await
                    }
                    result => {
                        let _ = this.update(cx, |this, cx| {
                            if matches!(result, Ok(Some(true))) {
                                this.message = None;
                                this.event(
                                    this.displays.active_display().unwrap_or_default(),
                                    IslandEvent::Located,
                                    cx,
                                );
                            } else {
                                this.location_failed(cx);
                            }
                        });
                        return;
                    }
                }
            }
            let _ = this.update(cx, |this, cx| this.location_failed(cx));
        })
        .detach();
    }

    fn location_failed(&mut self, cx: &mut Context<Self>) {
        self.message = Some(ui::i18n::LocalizedText::new("island.card_missing"));
        cx.notify();
    }
}
