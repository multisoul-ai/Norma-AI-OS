//! Workspace opening flow for the app shell.

use crate::app::{AppScreen, MultisoulApp};
use crate::motion::MotionStore;
use crate::workspace_state::WorkspaceState;
use canvas::NodeId;
use gpui::{Context, PathPromptOptions};
use runtime::Runtime;
use std::path::{Path, PathBuf};
use workspace::WorkspaceSource;

impl MultisoulApp {
    pub(crate) fn prompt_open_project(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.update(&mut *cx, |app, cx| {
                            app.open_workspace_path(path, cx);
                        })?;
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error =
                            Some(format!("Could not open project picker: {error}"));
                        cx.notify();
                    })?;
                }
                Err(error) => {
                    this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error = Some(format!(
                            "Project picker was cancelled unexpectedly: {error}"
                        ));
                        cx.notify();
                    })?;
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(crate) fn open_workspace_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Err(error) = enter_workspace_for_path(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            &mut self.screen,
            &mut self.workspace_state,
            &path,
        ) {
            tracing::warn!(%error, path = %path.display(), "failed to open workspace");
        }
        self.last_reflow_window_size = None;
        cx.notify();
    }
}

pub(crate) fn enter_workspace_for_path(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::master_layout::MasterLayoutState,
    screen: &mut AppScreen,
    workspace_state: &mut WorkspaceState,
    path: &Path,
) -> Result<(), String> {
    let root = workspace_state.open_workspace(path, WorkspaceSource::Open)?;
    runtime.kill_all();
    *runtime = Runtime::new_for_workspace(root);
    master_layout.clear();
    motion.clear_transient();
    motion.retain_nodes(std::iter::empty::<NodeId>());
    *screen = AppScreen::Canvas;
    Ok(())
}
