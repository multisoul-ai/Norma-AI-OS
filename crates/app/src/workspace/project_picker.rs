//! Native project picker coordination for component-root GPUI windows.

use crate::app::MultisoulApp;
use futures::channel::oneshot;
use gpui::{Context, PathPromptOptions, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(crate) fn prompt_open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });
        self.handle_project_picker_receiver(receiver, window, cx);
    }

    fn handle_project_picker_receiver(
        &mut self,
        receiver: oneshot::Receiver<anyhow::Result<Option<Vec<PathBuf>>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        if let Err(error) = this.update_in(cx, |app, window, cx| {
                            app.open_workspace_in_project_window(path, true, window, cx);
                        }) {
                            tracing::warn!(
                                %error,
                                "failed to apply selected project path to its source window"
                            );
                        }
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    let _ = this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error =
                            Some(format!("Could not open project picker: {error}"));
                        cx.notify();
                    });
                }
                Err(error) => {
                    let _ = this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error = Some(format!(
                            "Project picker was cancelled unexpectedly: {error}"
                        ));
                        cx.notify();
                    });
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn complete_project_picker_for_e2e(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (sender, receiver) = oneshot::channel();
        self.handle_project_picker_receiver(receiver, window, cx);
        if sender.send(Ok(Some(vec![path]))).is_err() {
            tracing::error!("failed to inject the E2E project picker selection");
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/project_picker_tests.rs"]
mod tests;
