use crate::app::{AppScreen, MultisoulApp};
use crate::settings::wallpaper::scoped::{WallpaperContext, WallpaperTarget};
use crate::settings::wallpaper::state::WallpaperSelection;
use gpui::Context;
use storage::WallpaperReference;

impl MultisoulApp {
    pub(crate) fn bind_desktop_wallpaper(&mut self, cx: &mut Context<Self>) {
        let context = matches!(self.screen, AppScreen::DesktopWorkspace)
            .then(|| self.desktop_workspace.active().cloned())
            .flatten()
            .map(|workspace| WallpaperContext {
                area: self
                    .desktop_workspace
                    .areas
                    .iter()
                    .find(|area| Some(&area.id) == self.desktop_workspace.expanded_area_id.as_ref())
                    .cloned(),
                workspace,
            });
        self.wallpaper_settings.scoped.bind(context);
        let selection = if matches!(self.screen, AppScreen::Launchpad)
            && self.desktop_workspace.home_directory
        {
            WallpaperSelection::Builtin {
                rel: crate::assets::HOME_BACKGROUND_ASSET,
            }
        } else {
            self.wallpaper_settings.scoped.effective_selection()
        };
        self.wallpaper_settings
            .set_effective_selection(selection, self.motion.reduce_motion);
        self.refresh_optional_media(cx);
        self.sync_all_dispatch_effects(cx);
    }

    pub(super) fn publish_wallpaper(
        &mut self,
        target: &WallpaperTarget,
        reference: Option<WallpaperReference>,
        status: Option<String>,
        cx: &mut Context<Self>,
    ) {
        match target {
            WallpaperTarget::Workspace(id) => {
                if let Some(workspace) = self
                    .desktop_workspace
                    .workspaces
                    .iter_mut()
                    .find(|workspace| &workspace.id == id)
                {
                    workspace.wallpaper = reference.clone();
                }
            }
            WallpaperTarget::Project {
                workspace_id,
                area_id,
            } => {
                if let Some(area) =
                    self.desktop_workspace.areas.iter_mut().find(|area| {
                        &area.id == area_id && &area.desktop_workspace_id == workspace_id
                    })
                {
                    area.wallpaper = reference.clone();
                }
            }
            WallpaperTarget::Global => {}
        }
        self.wallpaper_settings
            .scoped
            .apply(target, reference.clone());
        for session in self
            .desktop_workspace
            .sessions
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            session.update(cx, |session, _| {
                session
                    .wallpaper_settings
                    .scoped
                    .apply(target, reference.clone())
            });
        }
        self.bind_desktop_wallpaper(cx);
        self.broadcast_wallpaper_state(status.map(|status| (target, status)), cx);
    }

    pub(crate) fn broadcast_wallpaper_state(
        &mut self,
        status: Option<(&WallpaperTarget, String)>,
        cx: &mut Context<Self>,
    ) {
        let global = self.wallpaper_settings.scoped.global.clone();
        let customs = self.wallpaper_settings.scoped.customs.clone();
        let busy = self.wallpaper_settings.scoped.busy;
        let prompt = self.wallpaper_settings.scoped.delete_prompt.clone();
        if let Some((target, status)) = &status {
            if &self.wallpaper_settings.scoped.target() == *target {
                self.wallpaper_settings.scoped.status = Some(status.clone());
            }
        }
        let workspaces = self.desktop_workspace.workspaces.clone();
        for session in self
            .desktop_workspace
            .sessions
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            session.update(cx, |session, cx| {
                let scoped = &mut session.wallpaper_settings.scoped;
                scoped.global = global.clone();
                scoped.customs = customs.clone();
                scoped.busy = busy;
                scoped.delete_prompt = prompt.clone();
                if let Some(context) = &mut scoped.context {
                    if let Some(workspace) = workspaces
                        .iter()
                        .find(|workspace| workspace.id == context.workspace.id)
                    {
                        context.workspace.wallpaper = workspace.wallpaper.clone();
                    }
                }
                if let Some((target, status)) = &status {
                    if &scoped.target() == *target {
                        scoped.status = Some(status.clone());
                    }
                }
                let selection = scoped.effective_selection();
                session
                    .wallpaper_settings
                    .set_effective_selection(selection, session.motion.reduce_motion);
                session.refresh_optional_media(cx);
                cx.notify();
            });
        }
        cx.notify();
    }
}

impl MultisoulApp {
    /// A catalog read may have started before an optimistic write. Keep the newer
    /// in-memory selections while still accepting fresh names and project metadata.
    pub(crate) fn preserve_wallpaper_snapshot(
        &self,
        workspaces: &mut [storage::DesktopWorkspace],
        areas: &mut [storage::DesktopProjectArea],
        revision: u64,
        cx: &gpui::App,
    ) {
        if revision == self.wallpaper_settings.scoped.revision {
            return;
        }
        for workspace in workspaces {
            if let Some(current) = self
                .desktop_workspace
                .workspaces
                .iter()
                .find(|current| current.id == workspace.id)
            {
                workspace.wallpaper = current.wallpaper.clone();
            }
        }
        for area in areas {
            if let Some(current) = self.desktop_workspace.areas.iter().find(|current| {
                current.id == area.id && current.desktop_workspace_id == area.desktop_workspace_id
            }) {
                area.wallpaper = current.wallpaper.clone();
            } else if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(area.desktop_workspace_id.clone(), area.id.clone()))
            {
                if let Some(current) = session
                    .read(cx)
                    .wallpaper_settings
                    .scoped
                    .context
                    .as_ref()
                    .and_then(|context| context.area.as_ref())
                {
                    area.wallpaper = current.wallpaper.clone();
                }
            }
        }
    }
}
