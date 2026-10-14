use canvas::{AgentProfile, CanvasRect};
use runtime::{NodeInput, Runtime};
/// 场景：桌面普通输入形成未提交草稿，连接器需要读状态而不读取终端全文。
/// 构造：临时echo PTY、Codex节点；执行：查询、粘贴草稿、查询。
/// 预期：初始无占用、粘贴后有待处理输入，原手机接管标志仍为false。
#[test]
fn connector_can_detect_desktop_draft_without_terminal_reads() {
    let dir = tempfile::tempdir().expect("临时项目");
    let mut runtime = Runtime::new_for_workspace(dir.path());
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("echo PTY");
    assert!(!runtime.agent_task_input_pending(node), "初始输入空闲");
    runtime
        .send_node_input(node, NodeInput::Paste("用户草稿".into()))
        .expect("用户输入");
    assert!(runtime.agent_task_input_pending(node), "草稿必须可检测");
    assert!(
        !runtime.agent_task_input_held(node),
        "普通桌面草稿不伪装手机接管"
    );
    runtime
        .send_agent_task_text(node, "自动派工正文")
        .expect("自动输入");
    assert!(
        !runtime.agent_task_input_pending(node),
        "自动正文不误报用户占用"
    );
    runtime
        .send_node_input(node, NodeInput::Paste("用户修改".into()))
        .expect("用户修改");
    assert!(
        runtime.agent_task_input_pending(node),
        "用户修改自动正文后重新占用"
    );
    runtime.kill_all();
}
