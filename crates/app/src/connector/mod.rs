//! App-global connector ownership and the GPUI boundary; no detached OS service.
mod execution;
pub(crate) mod live;
mod projection;
mod repository;
mod view;
mod worker;
mod writes;
use gpui::{App, AppContext, Context, Entity, Global, Task};
use norma::connector::CardSigner;
use storage::connector_store::{ConnectorAudit, ConnectorConfig};
use ui::component::input::InputState;
use worker::{Command, Update};

struct Host(Entity<Controller>);
impl Global for Host {}
pub(super) struct Controller {
    commands: async_channel::Sender<Command>,
    config: ConnectorConfig,
    listening: bool,
    busy: bool,
    error: Option<String>,
    expiries: Vec<Option<i64>>,
    locked_until: i64,
    audit: Vec<ConnectorAudit>,
    port: Option<Entity<InputState>>,
    tunnel: Option<Entity<InputState>>,
    theme: ui::Theme,
    updates: Option<Task<()>>,
    capture: Option<Task<()>>,
    signer: Option<CardSigner>,
}
pub(crate) fn install(cx: &mut App) {
    let load = cx.background_executor().spawn(async {
        crate::database::default_database_context().map(|(_, store)| store.path().to_owned())
    });
    cx.spawn(async move |cx| {
        let Ok(database) = load.await else {
            return;
        };
        let _ = cx.update(|cx| {
            let (commands, receive) = async_channel::unbounded();
            let (updates, events) = async_channel::bounded::<Update>(8);
            let (live, requests) = async_channel::bounded::<repository::LiveRequest>(8);
            let target = cx.new(|cx| {
                let mut controller = Controller {
                    commands,
                    config: Default::default(),
                    listening: false,
                    busy: true,
                    error: None,
                    expiries: Vec::new(),
                    locked_until: 0,
                    audit: Vec::new(),
                    port: None,
                    tunnel: None,
                    theme: ui::Theme::default(),
                    updates: None,
                    capture: None,
                    signer: None,
                };
                controller.updates = Some(cx.spawn(async move |this, cx| {
                    while let Ok(mut update) = events.recv().await {
                        let tokens = update.tokens.take();
                        let _ = this.update(cx, |state: &mut Controller, cx| {
                            state.config = update.config;
                            state.listening = update.listening;
                            state.busy = false;
                            state.error = update.error;
                            state.expiries = update.expiries;
                            state.locked_until = update.locked_until;
                            state.audit = update.audit;
                            if let Some(signer) = update.signer {
                                state.signer = Some(signer);
                            }
                            cx.notify();
                        });
                        if let Some(tokens) = tokens {
                            let _ = cx.update(|cx| view::show_tokens(tokens, cx));
                        }
                    }
                }));
                let capture_database = database.clone();
                controller.capture = Some(cx.spawn(async move |this, cx| {
                    while let Ok(request) = requests.recv().await {
                        let _ = this.update(cx, |state: &mut Controller, cx| {
                            if let Some(signer) = &state.signer {
                                live::capture(&capture_database, request, signer, cx);
                            }
                        });
                    }
                }));
                controller
            });
            cx.set_global(Host(target));
            worker::start(database, receive, updates, live);
        });
    })
    .detach();
}
pub(crate) fn shutdown(cx: &mut App) {
    if let Some(host) = cx.try_global::<Host>() {
        let _ = host.0.read(cx).commands.try_send(Command::Shutdown);
    }
}
pub(crate) fn settings(theme: ui::Theme, cx: &mut App) -> gpui::Div {
    use gpui::prelude::*;
    let Some(host) = cx.try_global::<Host>() else {
        return gpui::div().child("连接器正在初始化…");
    };
    let entity = host.0.clone();
    entity.update(cx, |state, _| state.theme = theme);
    gpui::div().child(entity)
}
impl Controller {
    fn request(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        if self.commands.try_send(command).is_err() {
            self.busy = false;
            self.error = Some("连接器服务不可用。".into());
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/connector_adapter_tests.rs"]
mod adapter_tests;
