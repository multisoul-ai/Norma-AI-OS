use super::*;
use crate::live::{NodeInput, NodeView};
use canvas::{geometry::CanvasRect, AgentProfile, NodeType};
use terminal::{Key, KeyInput, PtyCommand, TerminalStatus};

/// workspace Runtime 新建 terminal 时应让 shell 从 workspace root 启动。
///
/// 数据构造（含关键数值推导）：
///   workspace root = temp_dir/multisoul-runtime-terminal-cwd-{pid}
///   terminal cols/rows = 200×24，200 列足够容纳 temp path，避免 PWD 输出换行
///   输入命令 = `printf 'RUNTIME_PWD:%s' "$PWD"; exit`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime::new_for_workspace(canonical root) → runtime.workspace_root = root
///   2. spawn_terminal("/bin/sh", 200, 24) → PtyCommand.cwd(root) → shell 从 root 启动
///   3. 逐字符发送 printf 命令并 Enter
///   4. drain_all 直到 terminal 退出或 3 秒超时
///   5. 汇总 terminal snapshot 文本
///
/// 预期结果：
///   - 正断言：runtime.workspace_root() 返回 canonical root
///   - 正断言：屏幕文本包含 `RUNTIME_PWD:{root}`
///   - 负断言：屏幕文本不应只包含测试进程当前目录
#[test]
fn test_workspace_runtime_spawns_terminal_in_workspace_root() {
    let workspace_root = std::env::temp_dir().join(format!(
        "multisoul-runtime-terminal-cwd-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&workspace_root);
    std::fs::create_dir(&workspace_root).expect("test should create workspace root");
    let canonical_root = workspace_root
        .canonicalize()
        .expect("workspace root should canonicalize");
    let process_cwd = std::env::current_dir().expect("test should read process cwd");
    let mut runtime = Runtime::new_for_workspace(canonical_root.clone());

    let node_id = runtime
        .spawn_terminal(CanvasRect::new(0.0, 0.0, 1200.0, 600.0), "/bin/sh", 200, 24)
        .expect("spawn_terminal in workspace should succeed");
    send_terminal_text(
        &mut runtime,
        node_id,
        "printf 'RUNTIME_PWD:%s' \"$PWD\"; exit",
    );

    let screen_text = drain_until_terminal_exits(&mut runtime, node_id);
    let _ = std::fs::remove_dir_all(&workspace_root);

    assert_eq!(
        runtime.workspace_root(),
        Some(canonical_root.as_path()),
        "Runtime::new_for_workspace 应保存 canonical workspace root"
    );
    assert!(
        screen_text.contains(&format!("RUNTIME_PWD:{}", canonical_root.display())),
        "terminal 应从 workspace root 启动，实际屏幕文本: {:?}",
        screen_text
    );
    assert!(
        !screen_text.contains(&format!("RUNTIME_PWD:{}", process_cwd.display())),
        "terminal 不应继续从测试进程 cwd 启动，否则 workspace cwd 未传入"
    );

    runtime.kill_all();
}

/// workspace Runtime 新建 agent-backed live node 时应继承 workspace root。
///
/// 数据构造（含关键数值推导）：
///   workspace root = temp_dir/multisoul-runtime-agent-cwd-{pid}
///   agent profile = Claude（profile 只影响 canvas 描述，测试命令替代真实 claude CLI）
///   command = /bin/sh -lc "printf 'AGENT_PWD:%s' \"$PWD\"; exit"
///   terminal cols/rows = 200×24，避免 temp path 换行
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime::new_for_workspace(root)
///   2. spawn_agent_with_command(Claude, command) → command.with_cwd(root)
///   3. drain_all 直到命令退出
///   4. 汇总 NodeView::Term snapshot 文本
///
/// 预期结果：
///   - 正断言：Agent 屏幕文本包含 `AGENT_PWD:{root}`
///   - 正断言：节点描述仍是 Agent
///   - 负断言：Agent 不应使用测试进程 cwd
#[test]
fn test_workspace_runtime_spawns_agent_command_in_workspace_root() {
    let workspace_root = std::env::temp_dir().join(format!(
        "multisoul-runtime-agent-cwd-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&workspace_root);
    std::fs::create_dir(&workspace_root).expect("test should create workspace root");
    let canonical_root = workspace_root
        .canonicalize()
        .expect("workspace root should canonicalize");
    let process_cwd = std::env::current_dir().expect("test should read process cwd");
    let mut runtime = Runtime::new_for_workspace(canonical_root.clone());
    let command = PtyCommand::new("/bin/sh").args(["-lc", "printf 'AGENT_PWD:%s' \"$PWD\"; exit"]);

    let node_id = runtime
        .spawn_agent_with_command(
            AgentProfile::Claude,
            CanvasRect::new(0.0, 0.0, 1200.0, 600.0),
            command,
            200,
            24,
        )
        .expect("spawn_agent_with_command in workspace should succeed");

    let screen_text = drain_until_terminal_exits(&mut runtime, node_id);
    let _ = std::fs::remove_dir_all(&workspace_root);
    let node = runtime
        .document
        .node(node_id)
        .expect("Agent node should remain in document after command exits");

    assert!(
        screen_text.contains(&format!("AGENT_PWD:{}", canonical_root.display())),
        "agent command 应从 workspace root 启动，实际屏幕文本: {:?}",
        screen_text
    );
    assert_eq!(
        node.kind.node_type(),
        NodeType::Agent,
        "spawn_agent_with_command 应保持 canvas 描述为 Agent"
    );
    assert!(
        !screen_text.contains(&format!("AGENT_PWD:{}", process_cwd.display())),
        "agent command 不应继续使用测试进程 cwd，否则 workspace cwd 未传入"
    );

    runtime.kill_all();
}

fn send_terminal_text(runtime: &mut Runtime, node_id: u64, text: &str) {
    for character in text.chars() {
        runtime
            .send_node_input(
                node_id,
                NodeInput::Key(KeyInput {
                    key: Key::Char(character),
                    ctrl: false,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("test should send character input to terminal");
    }
    runtime
        .send_node_input(
            node_id,
            NodeInput::Key(KeyInput {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .expect("test should send Enter to terminal");
}

fn drain_until_terminal_exits(runtime: &mut Runtime, node_id: u64) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut screen_text = String::new();
    while std::time::Instant::now() < deadline {
        runtime.drain_all();
        if let Some(NodeView::Term {
            snapshot, status, ..
        }) = runtime.node_view(node_id)
        {
            screen_text = snapshot
                .rows
                .iter()
                .flat_map(|row| row.iter().map(|cell| cell.c))
                .collect();
            if matches!(status, TerminalStatus::Exited { .. }) {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    screen_text
}
