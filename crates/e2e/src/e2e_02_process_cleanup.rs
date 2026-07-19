use gpui::TestAppContext;
use runtime::PtyFactory;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use terminal::{PtyBackend, PtyError};

use crate::harness::{create_terminal, enter_canvas, make_app, read_runtime};

struct KillRecordingBackend {
    killed: Arc<AtomicBool>,
}

impl PtyBackend for KillRecordingBackend {
    fn write(&mut self, _bytes: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {
        self.killed.store(true, Ordering::SeqCst);
    }
}

fn kill_recording_factory(killed: Arc<AtomicBool>) -> PtyFactory {
    Box::new(move |_command, _cols, _rows, _tx| {
        Ok(Box::new(KillRecordingBackend {
            killed: Arc::clone(&killed),
        }) as Box<dyn PtyBackend>)
    })
}

/// 用户通过 Cmd+D 删除 live Terminal 卡片时，App 必须同步触发 PTY 后端关闭请求。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Canvas 节点数 = 0
///   create_terminal 后节点数 = 0 + 1 = 1
///   Cmd+D 后节点数 = 1 - 1 = 0
///   killed 初始值 = false，只有 PtyBackend::kill 被调用时才变为 true
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入 KillRecordingBackend 并进入空 Canvas
///   2. 创建并聚焦一张 Terminal 卡片，确认后端尚未收到 kill
///   3. 模拟 Cmd+D → shortcut → close route → Runtime::remove_node → LiveNode::kill
///   4. 读取最终节点数与共享 killed 标志
///
/// 预期结果：
///   - 正断言：删除前存在且仅存在 1 个 live node
///   - 负断言：删除前 killed=false，创建卡片不能提前终止后端
///   - 正断言：删除后 killed=true，用户关闭动作已到达 PTY 生命周期边界
///   - 负断言：删除后节点数为 0，不能保留不可见 live node
#[test]
fn deleting_live_terminal_requests_pty_shutdown() {
    let mut test_cx = TestAppContext::single();
    let killed = Arc::new(AtomicBool::new(false));
    let (app, cx) = make_app(&mut test_cx, kill_recording_factory(Arc::clone(&killed)));
    let workspace = tempfile::tempdir().expect("process cleanup e2e should create workspace");
    enter_canvas(&app, cx, workspace.path());
    create_terminal(&app, cx);

    let live_before = read_runtime(&app, cx, |runtime| runtime.live_len());
    let killed_before = killed.load(Ordering::SeqCst);
    assert_eq!(live_before, 1, "创建 Terminal 后应恰好存在 1 个 live node");
    assert!(
        !killed_before,
        "创建 Terminal 不得提前调用 PtyBackend::kill"
    );

    cx.simulate_keystrokes("cmd-d");
    cx.run_until_parked();

    let live_after = read_runtime(&app, cx, |runtime| runtime.live_len());
    let killed_after = killed.load(Ordering::SeqCst);
    assert!(
        killed_after,
        "Cmd+D 删除 live Terminal 后必须调用 PtyBackend::kill"
    );
    assert_eq!(live_after, 0, "删除 Terminal 后不得保留不可见 live node");
}
