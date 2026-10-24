//! 后台发送线程：只做队列推进与网络 IO，永不运行在 GPUI 前台。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use crate::clock::unix_ms;
use crate::pipeline::Pipeline;
use crate::queue::Queued;
use crate::sink::Sink;

pub enum Msg {
    Event(Queued),
    SetEnabled(bool),
    /// 立即尽力发送；完成后通过 Sender 通知。
    Flush(Sender<()>),
    /// 先尽力发送，再关闭并清空队列（用于用户退出统计）。
    OptOut,
}

const POLL: Duration = Duration::from_secs(1);

pub fn run<S: Sink>(mut pipeline: Pipeline<S>, rx: Receiver<Msg>, marker: PathBuf) {
    loop {
        match rx.recv_timeout(POLL) {
            Ok(Msg::Event(item)) => pipeline.push(item),
            Ok(Msg::SetEnabled(enabled)) => {
                pipeline.set_enabled(enabled);
                let _ = std::fs::remove_file(&marker);
            }
            Ok(Msg::Flush(done)) => {
                pipeline.flush(unix_ms());
                let _ = done.send(());
            }
            Ok(Msg::OptOut) => {
                pipeline.flush(unix_ms());
                pipeline.set_enabled(false);
                if let Some(dir) = marker.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&marker, b"1");
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                pipeline.flush(unix_ms());
                return;
            }
        }
        pipeline.tick(unix_ms());
    }
}
