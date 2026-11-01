use crate::{
    app::MultisoulApp,
    settings::voice::a1_install::{self, A1InstallFailure, A1InstallProgress},
};
use gpui::{AppContext, Context, Entity, Subscription, WeakEntity, Window};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use ui::{
    component::input::{InputEvent, InputState},
    Theme,
};
pub(super) mod messages;
mod view;

enum State {
    Idle,
    Saved,
    Running(A1InstallProgress),
    Installed,
    Failed(A1InstallFailure),
}
impl State {
    fn progress(&mut self, event: A1InstallProgress) {
        if matches!(self, Self::Running(_)) {
            *self = Self::Running(event);
        }
    }
}

pub(in crate::settings::voice::view) struct A1Panel {
    pub(super) input: Entity<InputState>,
    pub(super) theme: Theme,
    root: PathBuf,
    host: WeakEntity<MultisoulApp>,
    state: State,
    busy: bool,
    cancelling: bool,
    generation: u64,
    cancelled: Arc<AtomicBool>,
    _subscription: Subscription,
}

impl Drop for A1Panel {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl A1Panel {
    pub(super) fn new(
        root: PathBuf,
        python: PathBuf,
        configured: bool,
        host: WeakEntity<MultisoulApp>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(python.to_string_lossy().to_string())
                .placeholder(ui::i18n::input_placeholder(
                    "voice.a1_python_placeholder",
                    window,
                    cx,
                ))
        });
        let subscription = cx.subscribe(&input, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && !view.busy {
                view.state = State::Idle;
                cx.notify();
            }
        });
        Self {
            input,
            theme,
            root,
            host,
            state: if configured {
                State::Saved
            } else {
                State::Idle
            },
            busy: false,
            cancelling: false,
            generation: 0,
            cancelled: Arc::new(AtomicBool::new(false)),
            _subscription: subscription,
        }
    }

    fn choose_python(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(ui::i18n::text("voice.a1_choose_python").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await {
                if let Some(path) = paths.first() {
                    let _ = this.update_in(cx, |view, window, cx| {
                        view.input.update(cx, |input, cx| {
                            input.set_value(path.to_string_lossy().to_string(), window, cx)
                        });
                    });
                }
            }
        })
        .detach();
    }

    fn install(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let python = PathBuf::from(self.input.read(cx).value().trim());
        self.cancelled = Arc::new(AtomicBool::new(false));
        self.busy = true;
        self.cancelling = false;
        self.generation = self.generation.wrapping_add(1);
        let job_generation = self.generation;
        self.state = State::Running(A1InstallProgress::Runtime);
        let generation = self
            .host
            .update(cx, |app, cx| {
                cx.notify();
                app.voice_settings.begin_a1_validation()
            })
            .ok();
        let root = self.root.clone();
        let cancelled = self.cancelled.clone();
        let (sender, receiver) = async_channel::unbounded();
        let work = cx.background_executor().spawn(async move {
            a1_install::install_a1(&root, &python, &cancelled, |event| {
                let _ = sender.try_send(event);
            })
        });
        cx.spawn(async move |this, cx| {
            while let Ok(event) = receiver.recv().await {
                let _ = this.update(cx, |view, cx| {
                    if view.generation == job_generation {
                        view.state.progress(event);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.spawn(async move|this,cx|{
            let result=work.await;
            let _=this.update(cx,|view,cx|{
                if view.generation!=job_generation{return;}
                view.busy=false;view.cancelling=false;
                let success=result.is_ok();
                view.state=match result {
                    Ok(_)=>{tracing::info!(event="a1_install_verified","A1 installation verified");State::Installed},
                    Err(error)=>{tracing::warn!(event="a1_install_failed",failure=?error,"A1 installation failed");State::Failed(error)},
                };
                if let Some(generation)=generation {
                    let _=view.host.update(cx,|app,cx|{
                        app.voice_settings.finish_a1_validation(generation,
                            if success{Ok(())}else{Err("A1 installation failed".into())});
                        cx.notify();
                    });
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/voice_a1_progress_tests.rs"]
mod tests;
