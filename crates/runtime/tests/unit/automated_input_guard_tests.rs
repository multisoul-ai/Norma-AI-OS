use super::*;
use crate::live::{Caps, LiveNode, NodeResize};
use std::sync::{Arc, Mutex};

/// 记录收到的输入，并声明「原生宿主尚未就绪」的桩节点。
struct PendingLive {
    written: Arc<Mutex<Vec<NodeInput>>>,
    pending: bool,
}

impl LiveNode for PendingLive {
    fn caps(&self) -> Caps {
        Caps::TERM
    }
    fn drain(&mut self) -> bool {
        false
    }
    fn native_question_host_pending(&self) -> bool {
        self.pending
    }
    fn resize(&mut self, _resize: NodeResize) -> Result<(), NodeError> {
        Ok(())
    }
    fn input(&mut self, input: NodeInput) -> Result<(), NodeError> {
        self.written.lock().unwrap().push(input);
        Ok(())
    }
    fn view(&self) -> Option<crate::NodeView> {
        None
    }
    fn kill(&mut self) {}
}

fn enter() -> NodeInput {
    NodeInput::Key(terminal::KeyInput {
        key: terminal::Key::Enter,
        ctrl: false,
        alt: false,
        shift: false,
    })
}

/// 场景：原生宿主未就绪时，自动化写入者（任务提交的 Enter、清理用 Ctrl+U、取消用 Ctrl+C）
///       不得写进原生 PTY——此时屏幕可能是更新菜单，一个 Enter 就会确认默认项。
/// 构造：node 挂一个声明 pending 的桩 LiveNode；另一个 node 的桩声明已就绪。
/// 执行：对两个 node 分别调用 `send_automated_input`；再对 pending 的 node 调 `send_node_input`（用户键盘路径）。
/// 预期：pending 的自动化写入被拒绝且桩没有收到任何输入；就绪的写入成功；
///       用户键盘路径不受影响（菜单必须可用上下键与回车操作）。
#[test]
fn automated_input_is_rejected_while_native_host_is_pending() {
    let mut runtime = Runtime::new();
    let pending_writes = Arc::new(Mutex::new(Vec::new()));
    let ready_writes = Arc::new(Mutex::new(Vec::new()));
    runtime.live.insert(
        1,
        Box::new(PendingLive {
            written: pending_writes.clone(),
            pending: true,
        }),
    );
    runtime.live.insert(
        2,
        Box::new(PendingLive {
            written: ready_writes.clone(),
            pending: false,
        }),
    );

    assert!(
        runtime.send_automated_input(1, enter()).is_err(),
        "宿主未就绪时自动化输入必须被拒绝"
    );
    assert!(
        pending_writes.lock().unwrap().is_empty(),
        "拒绝后不得有任何字节到达"
    );
    assert!(
        runtime.send_automated_input(2, enter()).is_ok(),
        "宿主就绪后放行"
    );
    assert_eq!(ready_writes.lock().unwrap().len(), 1);

    assert!(
        runtime.send_node_input(1, enter()).is_ok(),
        "用户键盘路径必须仍可回答原生菜单"
    );
    assert_eq!(pending_writes.lock().unwrap().len(), 1);
}
