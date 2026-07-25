use crate::app::MultisoulApp;
use crate::norma::transport::{bind_socket, NormaServerRequest, SocketBinding};
use crossbeam_channel::{unbounded, Receiver};
use gpui::{Context, Window};
use norma::client::default_socket_path;
use std::time::Duration;

pub(crate) struct NormaServer {
    pub(super) receiver: Receiver<NormaServerRequest>,
    _socket_binding: Option<SocketBinding>,
}

impl NormaServer {
    pub(crate) fn start() -> Self {
        let (sender, receiver) = unbounded();
        let socket_path = default_socket_path();
        let socket_binding = bind_socket(socket_path, sender);
        Self {
            receiver,
            _socket_binding: socket_binding,
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.receiver.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::NormaServer;
    use crate::norma::transport::bind_socket;
    use crossbeam_channel::unbounded;
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;

    fn start_server_at(socket_path: PathBuf) -> NormaServer {
        let (sender, receiver) = unbounded();
        let socket_binding = bind_socket(socket_path, sender);
        NormaServer {
            receiver,
            _socket_binding: socket_binding,
        }
    }

    /// Socket 服务交接：旧窗口释放时不得删除新窗口已经重绑的服务端点
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   server count = launchpad(1) + workspace(1) = 2
    ///   shared socket paths = 1（两个窗口沿用同一个 Norma CLI 端点）
    ///   expected live endpoints after handoff = workspace(1)
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. Launchpad 服务绑定共享 socket → 路径可连接。
    ///   2. Workspace 服务删除旧路径并重绑 → 新监听器接管同一路径。
    ///   3. 释放 Launchpad 服务 → 旧所有者只能清理自己的绑定，不能删除新路径。
    ///   4. 释放 Workspace 服务 → 当前所有者清理自己仍持有的路径。
    ///
    /// 预期结果：
    ///   - 正断言：旧服务释放后，共享路径仍存在且可连接到 Workspace 服务。
    ///   - 负断言：Workspace 服务释放后，共享路径不再残留。
    #[test]
    fn dropping_replaced_server_preserves_replacement_socket() {
        let temp = tempfile::tempdir().expect("Socket 交接测试应创建隔离的临时目录");
        let socket_path = temp.path().join("norma.sock");
        let launchpad_server = start_server_at(socket_path.clone());
        let workspace_server = start_server_at(socket_path.clone());

        drop(launchpad_server);

        assert!(
            socket_path.exists(),
            "Launchpad 服务释放后不得删除 Workspace 已重绑的 socket 路径"
        );
        assert!(
            UnixStream::connect(&socket_path).is_ok(),
            "Launchpad 服务释放后，Norma CLI 应仍能连接 Workspace 服务"
        );

        drop(workspace_server);

        assert!(
            !socket_path.exists(),
            "Workspace 当前所有者释放后不应残留 Norma socket 路径"
        );
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
