use crate::app::MultisoulApp;
use crate::norma::transport::{bind_socket, NormaServerRequest};
use crossbeam_channel::{unbounded, Receiver};
use gpui::{Context, Window};
use norma::client::default_socket_path;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) struct NormaServer {
    pub(super) receiver: Receiver<NormaServerRequest>,
    socket_path: Option<PathBuf>,
}

impl NormaServer {
    pub(crate) fn start() -> Self {
        let (sender, receiver) = unbounded();
        let socket_path = default_socket_path();
        let bound_path = bind_socket(socket_path, sender);
        Self {
            receiver,
            socket_path: bound_path,
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.receiver.is_empty()
    }
}

impl Drop for NormaServer {
    fn drop(&mut self) {
        if let Some(path) = &self.socket_path {
            let _ = fs::remove_file(path);
        }
    }
}

impl MultisoulApp {
    pub(crate) fn start_norma_drain_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(12))
                .await;
            let go = this.update_in(cx, |app, window, cx| {
                app.drain_norma_server(window, cx);
            });
            if go.is_err() {
                break;
            }
        })
        .detach();
    }

    pub(crate) fn drain_norma_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Ok(request) = self.norma_server.receiver.try_recv() {
            let response = self.execute_card_command(request.request, window, cx);
            let _ = request.response.send(response);
            cx.notify();
        }

        if self.norma_server.has_pending() {
            cx.notify();
        }
    }
}
