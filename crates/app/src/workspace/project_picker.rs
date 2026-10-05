//! Native project picker coordination for component-root GPUI windows.

use crate::app::MultisoulApp;
use futures::channel::oneshot;
use gpui::{Context, PathPromptOptions, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(crate) fn prompt_open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |host, cx| host.prompt_open_project(window, cx));
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(ui::i18n::text("workspace.open_project").into()),
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
                            app.accept_project_for_desktop(path, window, cx);
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
                        app.workspace_state.error = Some(ui::i18n::message(
                            "workspace.could_not_open_project_picker_error",
                            &[("error", (error).to_string())],
                        ));
                        cx.notify();
                    });
                }
                Err(error) => {
                    let _ = this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error = Some(ui::i18n::message(
                            "workspace.project_picker_was_cancelled_unexpectedly_error",
                            &[("error", (error).to_string())],
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
