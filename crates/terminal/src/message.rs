//! crossbeam 通道消息：PTY 读线程 → GPUI app。

/// PTY 读线程向 app 投递的事件。
#[derive(Debug, Clone)]
pub enum PtyEvent {
    /// 从 PTY 读到的原始字节（未解析）。
    Output(Vec<u8>),
    /// 子进程已退出。code 为退出码（可能为 None）。
    Exited { code: Option<i32> },
}
