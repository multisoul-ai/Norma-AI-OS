use super::{open_multisoul_window, InitialWindowState};
use crate::app::MultisoulApp;
use gpui::{App, Entity};
use std::path::PathBuf;

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn reopen_desktop_window_for_e2e(
        app: &Entity<Self>,
        cx: &mut App,
    ) -> anyhow::Result<(Entity<MultisoulApp>, gpui::AnyWindowHandle)> {
        let (paths, guard) = app.read_with(cx, |app, _| {
            (app.database_paths.clone(), app.test_database_guard.clone())
        });
        let opened = open_multisoul_window(InitialWindowState::Launchpad, paths, guard, cx)?;
        Ok((opened.app, opened.root_handle.into()))
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn reopen_project_window_for_e2e(
        app: &Entity<Self>,
        path: PathBuf,
        cx: &mut App,
    ) -> anyhow::Result<(Entity<MultisoulApp>, gpui::AnyWindowHandle)> {
        let (paths, guard) = app.read_with(cx, |app, _| {
            (app.database_paths.clone(), app.test_database_guard.clone())
        });
        let opened = open_multisoul_window(InitialWindowState::Workspace(path), paths, guard, cx)?;
        Ok((opened.app, opened.root_handle.into()))
    }
}
