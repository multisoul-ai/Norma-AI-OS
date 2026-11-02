use crate::{
    app::MultisoulApp,
    settings::voice::a1_install::{self, A1InstallFailure, A1InstallProgress},
};
use gpui::{Context, WeakEntity};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use ui::Theme;
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
    fn install_label_key(&self) -> &'static str {
        match self {
            Self::Installed => "voice.a1_installed",
            Self::Saved => "voice.a1_reinstall",
            Self::Failed(_) => "voice.a1_install_retry",
            _ => "voice.a1_install_action",
        }
    }

    fn can_install(&self) -> bool {
        matches!(self, Self::Idle | Self::Saved | Self::Failed(_))
    }

    fn files_verified(&mut self) -> bool {
        if matches!(self, Self::Saved) {
            *self = Self::Installed;
            true
        } else {
            false
        }
    }

    fn progress(&mut self, event: A1InstallProgress) {
        if matches!(self, Self::Running(_)) {
            *self = Self::Running(event);
        }
    }
}

pub(in crate::settings::voice::view) struct A1Panel {
    pub(super) theme: Theme,
    root: PathBuf,
    host: WeakEntity<MultisoulApp>,
    state: State,
    busy: bool,
    cancelling: bool,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

impl Drop for A1Panel {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl A1Panel {
    pub(super) fn files_verified(&mut self, cx: &mut Context<Self>) {
        if self.state.files_verified() {
            cx.notify();
        }
    }

    pub(super) fn new(
        root: PathBuf,
        configured: bool,
        host: WeakEntity<MultisoulApp>,
        theme: Theme,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
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
        }
    }

    fn install(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let bundle = a1_install::engine_bundle();
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
            a1_install::install_a1(&root, &bundle, cancelled.as_ref(), |event| {
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
