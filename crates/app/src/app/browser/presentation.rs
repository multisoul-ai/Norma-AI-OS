use super::{BrowserViewBounds, MultisoulApp};
use canvas::NodeId;
use gpui::{canvas, prelude::*, px, AnyElement, Context, SharedString};

impl MultisoulApp {
    /// Register during paint so retained canvases also update their native children.
    pub(crate) fn browser_scene_element(
        &self,
        node_id: NodeId,
        url: String,
        bounds: BrowserViewBounds,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        let id = SharedString::from(format!("native-browser-{}-{node_id}", cx.entity_id()));
        canvas(
            |_, _, _| (),
            move |local, (), window, _| {
                window.on_scene_presentation(id, local, move |placement, window, cx| {
                    let _ = owner.update(cx, |app, cx| {
                        let host = app.desktop_host.as_ref().and_then(|host| host.upgrade());
                        let host = host.as_ref().map(|host| host.read(cx));
                        let modal = app.settings_panel.open
                            || host.is_some_and(|host| {
                                host.settings_panel.open
                                    || host.desktop_workspace.create_input.is_some()
                                    || host.desktop_workspace.add_project_open
                                    || host.desktop_workspace.rename_target.is_some()
                                    || host.desktop_workspace.pending_delete.is_some()
                                    || host.desktop_workspace.pending_project_path.is_some()
                                    || host.desktop_workspace.switcher_open
                                    || host.clone_project.is_open()
                            });
                        let interactive = !modal
                            && host.is_none_or(|host| {
                                matches!(host.screen, crate::app::AppScreen::DesktopWorkspace)
                                    && host.expanded_desktop_project().is_some_and(|project| {
                                        project.entity_id() == cx.entity_id()
                                    })
                                    && !host.desktop_project_zoom_active()
                                    && !host.desktop_workspace_zoom_active()
                            });
                        if let Some(placement) = placement.filter(|_| !modal) {
                            // The native document always uses local layout size, never the camera frame.
                            let logical = BrowserViewBounds {
                                x: 0.0,
                                y: 0.0,
                                ..bounds
                            };
                            app.browsers.sync_view(node_id, &url, logical, window);
                            app.browsers
                                .present(node_id, placement, interactive, window);
                        } else {
                            app.browsers.hide(node_id);
                        }
                    });
                });
            },
        )
        .absolute()
        .left(px(bounds.x as f32))
        .top(px(bounds.y as f32))
        .w(px(bounds.width as f32))
        .h(px(bounds.height as f32))
        .into_any_element()
    }
}
