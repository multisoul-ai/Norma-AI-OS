use super::state::{default_clone_parent, CloneProjectPhase};
use crate::app::{AppScreen, MultisoulApp};
use futures::channel::oneshot;
use gpui::{App, AppContext, Context, Focusable, Global, PathPromptOptions, Window};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use ui::component::input::{InputEvent, InputState};
use workspace::git_clone::{
    CloneCancellation, CloneEvent, CloneProgress, CloneStage, GitCloneRequest, GitHubRemote,
};

#[derive(Default)]
struct ActiveCloneTask {
    task_id: Option<u64>,
    cancellation: Option<CloneCancellation>,
    running: Option<Arc<AtomicBool>>,
}

impl Global for ActiveCloneTask {}

impl MultisoulApp {
    pub(crate) fn open_cloned_workspace_in_project_window(
        &mut self,
        path: PathBuf,
        close_current_launchpad: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.screen,
            AppScreen::Launchpad | AppScreen::DesktopWorkspace
        ) || self.desktop_host.is_some()
        {
            self.accept_project_for_desktop(path, window, cx);
            return;
        }
        self.open_workspace_in_project_window_with_state(
            path,
            close_current_launchpad,
            true,
            window,
            cx,
        );
    }

    pub(crate) fn release_clone_task_if_owned(&mut self, cx: &mut App) {
        let Some(task_id) = self.clone_project.dialog().map(|dialog| dialog.task_id) else {
            return;
        };
        let registry = cx.default_global::<ActiveCloneTask>();
        if registry.task_id == Some(task_id) {
            if let Some(cancellation) = registry.cancellation.as_ref() {
                cancellation.cancel();
            }
        }
    }

    pub(crate) fn open_clone_project_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |host, cx| host.open_clone_project_dialog(window, cx));
            return;
        }
        if self.clone_project.is_open() {
            return;
        }
        let remote_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://github.com/owner/repository.git")
                .multi_line(false)
        });
        let folder_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Use repository name")
                .multi_line(false)
        });
        let remote_subscription =
            cx.subscribe(&remote_input, |app, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(dialog) = app.clone_project.dialog_mut() {
                        dialog.remote_draft = input.read(cx).value().to_string();
                        if let Ok(remote) = GitHubRemote::parse(&dialog.remote_draft) {
                            dialog.transport = remote.transport();
                        }
                        if matches!(dialog.phase, CloneProjectPhase::Failed { .. }) {
                            dialog.phase = CloneProjectPhase::Editing;
                        }
                    }
                    cx.notify();
                }
            });
        let folder_subscription =
            cx.subscribe(&folder_input, |app, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(dialog) = app.clone_project.dialog_mut() {
                        dialog.folder_draft = input.read(cx).value().to_string();
                        if matches!(dialog.phase, CloneProjectPhase::Failed { .. }) {
                            dialog.phase = CloneProjectPhase::Editing;
                        }
                    }
                    cx.notify();
                }
            });
        let parent = default_clone_parent(self.workspace_state.current_root.as_deref());
        self.clone_project.open(
            remote_input.clone(),
            folder_input,
            parent,
            vec![remote_subscription, folder_subscription],
        );
        let focus = remote_input.focus_handle(cx);
        window.defer(cx, move |window, _cx| focus.focus(window));
        cx.notify();
    }

    pub(super) fn choose_clone_parent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clone_project.is_busy() {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose Clone Destination".into()),
        });
        self.handle_clone_parent_receiver(receiver, window, cx);
    }

    fn handle_clone_parent_receiver(
        &mut self,
        receiver: oneshot::Receiver<anyhow::Result<Option<Vec<PathBuf>>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = this.update(&mut *cx, |app, cx| {
                            if let Some(dialog) = app.clone_project.dialog_mut() {
                                dialog.parent = path;
                                dialog.phase = CloneProjectPhase::Editing;
                            }
                            cx.notify();
                        });
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    let _ = this.update(&mut *cx, |app, cx| {
                        app.set_clone_failure(format!(
                            "Could not choose a clone destination: {error}"
                        ));
                        cx.notify();
                    });
                }
                Err(error) => {
                    let _ = this.update(&mut *cx, |app, cx| {
                        app.set_clone_failure(format!(
                            "Clone destination picker was cancelled unexpectedly: {error}"
                        ));
                        cx.notify();
                    });
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(super) fn start_clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clone_project.is_busy() {
            return;
        }
        let prepared = match self.clone_project.prepare() {
            Ok(prepared) => prepared,
            Err(message) => {
                self.set_clone_failure(message);
                cx.notify();
                return;
            }
        };
        let Some(task_id) = self.clone_project.dialog().map(|dialog| dialog.task_id) else {
            return;
        };
        let cancellation = CloneCancellation::new();
        let registry = cx.default_global::<ActiveCloneTask>();
        let another_task_running = registry
            .running
            .as_ref()
            .is_some_and(|running| running.load(Ordering::Acquire));
        if another_task_running {
            self.set_clone_failure(
                "Another repository is already being cloned. Wait for it to finish or cancel it."
                    .to_string(),
            );
            cx.notify();
            return;
        }
        registry.task_id = Some(task_id);
        registry.cancellation = Some(cancellation.clone());
        let running = Arc::new(AtomicBool::new(true));
        registry.running = Some(running.clone());

        let initial_progress = CloneProgress {
            stage: CloneStage::Connecting,
            percent: None,
        };
        if let Some(dialog) = self.clone_project.dialog_mut() {
            dialog.phase = CloneProjectPhase::Running(initial_progress);
            dialog.cancellation = Some(cancellation.clone());
        }
        cx.notify();

        let request =
            GitCloneRequest::new(&prepared.remote, prepared.transport, prepared.destination);
        let backend = self.clone_project.backend();
        let (event_tx, event_rx) = async_channel::unbounded();
        let work_cancellation = cancellation.clone();
        let work = cx.background_executor().spawn(async move {
            let result = backend.run(request, &work_cancellation, |event| {
                let _ = event_tx.try_send(event);
            });
            running.store(false, Ordering::Release);
            result
        });

        cx.spawn(async move |this, cx| {
            while let Ok(event) = event_rx.recv().await {
                let _ = this.update(cx, |app, cx| {
                    app.apply_clone_event(task_id, event);
                    cx.notify();
                });
            }
        })
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.finish_clone_task(task_id, result, window, cx);
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(super) fn cancel_or_close_clone_project(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.clone_project.dialog_mut() else {
            return;
        };
        if let Some(cancellation) = dialog.cancellation.as_ref() {
            cancellation.cancel();
            dialog.phase = CloneProjectPhase::Cancelling;
        } else {
            self.clone_project.close();
        }
        cx.notify();
    }

    fn apply_clone_event(&mut self, task_id: u64, event: CloneEvent) {
        let Some(dialog) = self.clone_project.dialog_mut() else {
            return;
        };
        if dialog.task_id != task_id {
            return;
        }
        if let CloneEvent::Progress(progress) = event {
            dialog.phase = CloneProjectPhase::Running(progress);
        }
    }

    fn finish_clone_task(
        &mut self,
        task_id: u64,
        result: Result<PathBuf, workspace::git_clone::CloneFailure>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let registry = cx.default_global::<ActiveCloneTask>();
        if registry.task_id == Some(task_id) {
            registry.task_id = None;
            registry.cancellation = None;
            registry.running = None;
        }
        let current_task_matches = self
            .clone_project
            .dialog()
            .is_some_and(|dialog| dialog.task_id == task_id);
        if !current_task_matches {
            return;
        }

        match result {
            Ok(target) => {
                let close_launchpad = matches!(self.screen, AppScreen::Launchpad);
                self.clone_project.close();
                self.open_cloned_workspace_in_project_window(target, close_launchpad, window, cx);
            }
            Err(error) => {
                let target_preserved = error.target_preserved();
                let message = if target_preserved {
                    format!("{error} Files were kept at {}.", error.target().display())
                } else {
                    error.to_string()
                };
                if let Some(dialog) = self.clone_project.dialog_mut() {
                    dialog.phase = CloneProjectPhase::Failed {
                        message,
                        target_preserved,
                    };
                    dialog.cancellation = None;
                }
                cx.notify();
            }
        }
    }

    fn set_clone_failure(&mut self, message: String) {
        if let Some(dialog) = self.clone_project.dialog_mut() {
            dialog.phase = CloneProjectPhase::Failed {
                message,
                target_preserved: false,
            };
            dialog.cancellation = None;
        }
    }
}
