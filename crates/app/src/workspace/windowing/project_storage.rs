use crate::app::MultisoulApp;
use gpui::{Context, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(in crate::workspace) fn open_workspace_in_project_window_with_state(
        &mut self,
        path: PathBuf,
        close_current_launchpad: bool,
        cloned: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = self.database_paths.clone();
        let root = path.clone();
        let storage = cx.background_executor().spawn(async move {
            storage::ProjectStore::initialize_for_database_paths(&paths, &root)
        });
        cx.spawn_in(window, async move |this, cx| {
            let prepared = storage.await;
            let _ = this.update_in(cx, |app, window, cx| match prepared {
                Ok(_) => app.open_prepared_project_window(
                    path,
                    close_current_launchpad,
                    cloned,
                    window,
                    cx,
                ),
                Err(error) => {
                    tracing::warn!(%error, "failed to prepare project storage");
                    app.workspace_state.error = Some(ui::i18n::message(
                        "workspace.could_not_open_project_window_error",
                        &[("error", error.to_string())],
                    ));
                    cx.notify();
                }
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}
