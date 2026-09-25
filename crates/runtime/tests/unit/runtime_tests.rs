use super::*;
use crate::focus::InputMode;
use crate::live::{NodeError, NodeInput, NodeResize, NodeView};
use canvas::{geometry::CanvasRect, AgentProfile, NodeType};
use terminal::{Key, KeyInput, PtyCommand};

/// Runtime::new() 初始状态：document/selection/focus/live 均处于空/默认状态。
///
/// 数据构造：
///   Runtime::new() — 不加入任何节点或 live runtime
///
/// 执行过程：
///   1. Runtime::new() → 各字段初始化
///   2. 检查 document 节点列表为空
///   3. 检查 live_len=0
///   4. 检查 focus 模式为 CanvasSelect
///
/// 预期结果：
///   - 正断言：document.nodes() 为空，live_len() == 0
///   - 正断言：focus.mode() == CanvasSelect
///   - 负断言：document.nodes().len() != 1（未添加节点）
#[test]
fn test_runtime_new_is_empty() {
    let rt = Runtime::new();
    assert_eq!(rt.document.nodes().len(), 0, "新建 Runtime 文档应无节点");
    assert_eq!(rt.live_len(), 0, "新建 Runtime 不应注册任何 live node");
    assert_ne!(
        rt.document.nodes().len(),
        1,
        "未添加节点，文档不应有 1 个节点"
    );
    assert_eq!(
        rt.focus.mode(),
        InputMode::CanvasSelect,
        "新建 Runtime 焦点应为 CanvasSelect"
    );
}

/// spawn_terminal 同时注册 NodeId 绑定的 live node，并在 document 中创建 Term 描述节点。
///
/// 数据构造（含关键数值推导）：
///   bounds = CanvasRect::new(0, 0, 800, 600)
///   shell = "/bin/sh"，cols=80，rows=24
///   document.next_id 初始为 1，因此 node_id=1
///   runtime live map 使用 node_id 作为 key，因此 live_len=1 且 node_caps(1)=Caps::TERM
///
/// 执行过程：
///   1. spawn_terminal(bounds, "/bin/sh", 80, 24)
///      → TermLive::spawn(...)
///      → document.add_node(NodeKind::Term(TermSpec), bounds) → node_id=1
///      → live.insert(node_id, TermLive)
///   2. 查询 document.node(node_id)、node_caps(node_id)、node_view(node_id)
///
/// 预期结果：
///   - 正断言：spawn_terminal 返回 Ok(node_id)
///   - 正断言：document 节点类型是 NodeType::Term
///   - 正断言：live_len == 1，node_caps 可聚焦/接收键盘
///   - 负断言：node_caps(999) == Caps::NONE
#[test]
fn test_spawn_terminal_registers_live_node_by_node_id() {
    let mut rt = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 800.0, 600.0);
    let node_id = rt
        .spawn_terminal(bounds, "/bin/sh", 80, 24)
        .expect("spawn_terminal 应成功，返回 Ok(node_id)");

    let node = rt
        .document
        .node(node_id)
        .expect("spawn 返回的 node_id 应存在于 document");
    assert_eq!(
        node.kind.node_type(),
        NodeType::Term,
        "spawn_terminal 应创建纯描述的 Term 节点"
    );
    assert_eq!(
        rt.live_len(),
        1,
        "spawn_terminal 应注册 1 个以 NodeId 绑定的 live node"
    );
    assert!(
        rt.node_caps(node_id).focusable,
        "Term live node 应通过 caps 暴露可聚焦能力"
    );
    assert!(
        rt.node_caps(node_id).accepts_key,
        "Term live node 应通过 caps 暴露键盘输入能力"
    );
    assert_eq!(
        rt.node_caps(999),
        Caps::NONE,
        "不存在的 node_id 应返回 Caps::NONE"
    );
    assert!(
        matches!(rt.node_view(node_id), Some(NodeView::Term { .. })),
        "Term live node 应暴露 NodeView::Term 供 app/card 渲染"
    );

    rt.kill_all();
}

/// 两次 spawn_terminal 分配不同 NodeId，并注册两个 live node。
///
/// 数据构造（含关键数值推导）：
///   first bounds=(0,0,400,300)，second bounds=(400,0,400,300)
///   document.next_id 初始=1，因此 first→node_id=1，second→node_id=2
///
/// 执行过程：
///   1. spawn_terminal(first)  → node_id_a=1
///   2. spawn_terminal(second) → node_id_b=2
///   3. 检查 document 与 live registry
///
/// 预期结果：
///   - 正断言：node_id_a != node_id_b
///   - 正断言：document.nodes().len() == 2，live_len() == 2
///   - 负断言：document.nodes().len() != 1（不是只有一个节点）
#[test]
fn test_two_spawn_terminals_get_distinct_node_ids_and_live_nodes() {
    let mut rt = Runtime::new();
    let b1 = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
    let b2 = CanvasRect::new(400.0, 0.0, 400.0, 300.0);

    let id_a = rt
        .spawn_terminal(b1, "/bin/sh", 80, 24)
        .expect("第一个 spawn 应成功");
    let id_b = rt
        .spawn_terminal(b2, "/bin/sh", 80, 24)
        .expect("第二个 spawn 应成功");

    assert_ne!(id_a, id_b, "两个 spawn_terminal 应返回不同的 node_id");
    assert_eq!(
        rt.document.nodes().len(),
        2,
        "两次 spawn 后 document 应有 2 个节点"
    );
    assert_eq!(
        rt.live_len(),
        2,
        "两次 spawn 后 live registry 应有 2 个节点"
    );
    assert_ne!(
        rt.document.nodes().len(),
        1,
        "不应只有 1 个节点（两次 spawn 均应成功）"
    );

    rt.kill_all();
}

/// spawn_agent_with_command 注册 NodeId 绑定的 live node，并在 document 中创建 Agent 描述节点。
///
/// 数据构造（含关键数值推导）：
///   bounds = CanvasRect::new(0, 0, 640, 400)
///   profile = AgentProfile::Claude
///   command = /bin/sh -lc "printf AGENT_OK; exit"（测试替身，避免依赖本机 claude）
///   cols=80, rows=24
///   document.next_id 初始=1，因此 node_id=1
///
/// 执行过程：
///   1. spawn_agent_with_command(Claude, bounds, command, 80, 24)
///      → TermLive::spawn_command(...)
///      → document.add_node(NodeKind::Agent(AgentSpec{profile:Claude}), bounds)
///      → live.insert(node_id, TermLive)
///   2. 查询 document.node(node_id) 与 node_caps(node_id)
///
/// 预期结果：
///   - 正断言：document 节点是 Agent 且 profile=Claude
///   - 正断言：live_len == 1，Agent caps 可聚焦/接收键盘
///   - 负断言：节点不应是普通 Term 或 Rect
#[test]
fn test_spawn_agent_registers_live_node_and_agent_descriptor() {
    let mut rt = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let command = PtyCommand::new("/bin/sh").args(["-lc", "printf AGENT_OK; exit"]);

    let node_id = rt
        .spawn_agent_with_command(AgentProfile::Claude, bounds, command, 80, 24)
        .expect("spawn_agent_with_command 应成功");

    assert_eq!(
        rt.live_len(),
        1,
        "spawn Agent 后 live registry 应恰好有 1 个节点"
    );

    let node = rt
        .document
        .node(node_id)
        .expect("spawn 返回的 node_id 应能在 document 中查到");
    match &node.kind {
        CanvasNodeKind::Agent(agent) => {
            assert_eq!(
                agent.profile,
                AgentProfile::Claude,
                "Agent 节点 profile 应保存为 Claude"
            );
        }
        other => panic!("spawn_agent_with_command 不应创建非 Agent 节点: {other:?}"),
    }
    assert!(
        rt.node_caps(node_id).focusable,
        "Agent live node 应复用 terminal-backed 可聚焦能力"
    );
    assert!(
        rt.node_caps(node_id).accepts_key,
        "Agent live node 应复用 terminal-backed 键盘输入能力"
    );

    rt.kill_all();
}

/// Terminal zsh 启动命令：应以 login shell 读取用户 profile 环境。
///
/// 数据构造：
///   shell = "/bin/zsh"
///   预期命令 = /bin/zsh -l
///   `-l` 让 zsh 先读 ~/.zprofile，再读 ~/.zshrc；Homebrew PATH 通常在 profile 中初始化
///
/// 执行过程：
///   1. terminal_shell_command("/bin/zsh") → 构造 Terminal 节点实际使用的 PtyCommand
///   2. 检查 program 与 args
///
/// 预期结果：
///   - 正断言：program 保持 "/bin/zsh"
///   - 正断言：args 精确为 ["-l"]，确保读取 ~/.zprofile
///   - 负断言：args 不应为空，否则 GUI 环境下 ~/.zshrc 中的 brew 工具可能找不到
#[test]
fn test_terminal_shell_command_uses_login_zsh() {
    let command = terminal_shell_command("/bin/zsh");

    assert_eq!(
        command.program, "/bin/zsh",
        "Terminal zsh 命令应保留用户 shell 路径"
    );
    assert_eq!(
        command.args,
        vec!["-l"],
        "Terminal zsh 应带 -l，以读取 ~/.zprofile 中的 Homebrew PATH 初始化"
    );
    assert!(
        !command.args.is_empty(),
        "Terminal zsh args 不应为空，否则只会读取 ~/.zshrc 而跳过 ~/.zprofile"
    );
}

/// 非 zsh shell 启动命令：不应强行追加 zsh 专用 login 参数。
///
/// 数据构造：
///   shell = "/bin/sh"
///   预期命令 = /bin/sh（无参数）
///   测试中的 /bin/sh 替身命令依赖精确 argv，不能被追加 -l
///
/// 执行过程：
///   1. terminal_shell_command("/bin/sh") → 构造 Terminal 节点实际使用的 PtyCommand
///   2. 检查 program 与 args
///
/// 预期结果：
///   - 正断言：program 保持 "/bin/sh"
///   - 正断言：args 为空，保持非 zsh shell 的原始行为
///   - 负断言：args 不应包含 "-l"，避免改变 /bin/sh 测试替身语义
#[test]
fn test_terminal_shell_command_preserves_non_zsh_shell_args() {
    let command = terminal_shell_command("/bin/sh");

    assert_eq!(
        command.program, "/bin/sh",
        "非 zsh Terminal 命令应保留原始 shell 路径"
    );
    assert_eq!(
        command.args.len(),
        0,
        "非 zsh shell 不应被追加 zsh 专用参数"
    );
    assert!(
        !command.args.iter().any(|arg| arg == "-l"),
        "非 zsh shell 不应包含 -l，避免改变测试替身或用户自定义 shell 行为"
    );
}

/// resize_node 通过 NodeResize::TermGrid 到达 terminal-backed live node。
///
/// 数据构造（含关键数值推导）：
///   初始终端尺寸 80×24
///   resize 目标尺寸 100×30
///   node_id 来自 spawn_terminal，document.next_id 初始为 1，因此 node_id=1
///
/// 执行过程：
///   1. spawn_terminal(..., 80, 24)
///   2. resize_node(node_id, TermGrid{100,30})
///   3. 查询 node_view(node_id) 验证 snapshot cols/lines
///
/// 预期结果：
///   - 正断言：resize_node 返回 Ok
///   - 正断言：NodeView::Term snapshot cols=100, lines=30
///   - 负断言：snapshot cols 不应仍为 80
#[test]
fn test_resize_node_reaches_terminal_live_node() {
    let mut rt = Runtime::new();
    let node_id = rt
        .spawn_terminal(CanvasRect::new(0.0, 0.0, 800.0, 600.0), "/bin/sh", 80, 24)
        .expect("spawn_terminal 应成功");

    rt.resize_node(
        node_id,
        NodeResize::TermGrid {
            cols: 100,
            rows: 30,
        },
    )
    .expect("resize_node 应成功");

    let Some(NodeView::Term { snapshot, .. }) = rt.node_view(node_id) else {
        panic!("resize 后 node_view 应仍为 NodeView::Term");
    };
    assert_eq!(snapshot.cols, 100, "resize 后 snapshot.cols 应为 100");
    assert_eq!(snapshot.lines, 30, "resize 后 snapshot.lines 应为 30");
    assert_ne!(snapshot.cols, 80, "resize 后 cols 不应仍为初始值 80");

    rt.kill_all();
}

/// send_node_input 将 focused node 的按键输入写入 live node。
///
/// 数据构造（含关键数值推导）：
///   node_id 来自 spawn_terminal
///   输入 KeyInput::Enter → terminal::encode 后为 CR(0x0d)
///   本测试只验证 runtime dispatch 返回 Ok，不验证 shell 输出
///
/// 执行过程：
///   1. spawn_terminal 创建 terminal-backed live node
///   2. send_node_input(node_id, NodeInput::Key(Enter))
///   3. send_node_input(999, NodeInput::Key(Enter))
///
/// 预期结果：
///   - 正断言：真实 node_id 输入返回 Ok
///   - 负断言：缺失 node_id 返回 Err(NodeError::MissingNode)
#[test]
fn test_send_node_input_dispatches_to_live_node() {
    let mut rt = Runtime::new();
    let node_id = rt
        .spawn_terminal(CanvasRect::new(0.0, 0.0, 800.0, 600.0), "/bin/sh", 80, 24)
        .expect("spawn_terminal 应成功");
    let input = NodeInput::Key(KeyInput {
        key: Key::Enter,
        ctrl: false,
        alt: false,
        shift: false,
    });

    let existing = rt.send_node_input(node_id, input.clone());
    let missing = rt.send_node_input(999, input);

    assert!(
        existing.is_ok(),
        "send_node_input(existing) 应返回 Ok，证明输入已 dispatch 到 live node"
    );
    assert!(
        matches!(missing, Err(NodeError::MissingNode)),
        "send_node_input(999) 应返回 MissingNode，实际 {:?}",
        missing
    );

    rt.kill_all();
}

/// remove_node 删除 live-backed 卡片时同步清理 live node、选择与焦点。
///
/// 数据构造（含关键数值推导）：
///   first:  bounds=(0,0,400,300), node_id=1
///   second: bounds=(400,0,400,300), node_id=2
///   document.next_id 初始=1，连续 spawn_terminal 后 first→1、second→2
///
/// 执行过程：
///   1. spawn first/second → document=[first, second], live={first,second}
///   2. selection.select_only(first)，focus_node(first)
///   3. remove_node(first) → 删除 document 节点，kill/drop live(first)，清除 first 选择，退出焦点
///   4. 查询 first/second 的节点、live 能力、选择与焦点状态
///
/// 预期结果：
///   - 正断言：remove_node(first) 返回 true，first 节点和 live node 均不存在
///   - 正断言：second 节点与 live node 仍存在
///   - 正断言：删除聚焦节点后 focus 回到 CanvasSelect
///   - 负断言：selection 不再包含 first，remove_node(999) 返回 false
#[test]
fn test_remove_node_cleans_live_node_selection_and_focus() {
    let mut rt = Runtime::new();
    let first_bounds = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
    let second_bounds = CanvasRect::new(400.0, 0.0, 400.0, 300.0);

    let first = rt
        .spawn_terminal(first_bounds, "/bin/sh", 80, 24)
        .expect("first spawn_terminal 应成功");
    let second = rt
        .spawn_terminal(second_bounds, "/bin/sh", 80, 24)
        .expect("second spawn_terminal 应成功");
    rt.selection.select_only(first);
    rt.focus.focus_node(first);

    let removed = rt.remove_node(first);

    assert!(
        removed,
        "remove_node(first) 应返回 true，表示确实删除了节点"
    );
    assert!(
        rt.document.node(first).is_none(),
        "first 节点应已从 document 删除，卡片不应继续渲染"
    );
    assert_eq!(
        rt.node_caps(first),
        Caps::NONE,
        "first 对应 live node 应被移除，避免关闭卡片后子进程继续存活"
    );
    assert!(
        rt.document.node(second).is_some(),
        "删除 first 不应影响 second 节点"
    );
    assert!(
        rt.node_caps(second).focusable,
        "删除 first 不应移除 second 对应的 live node"
    );
    assert!(
        !rt.selection.contains(first),
        "selection 不应继续包含已删除的 first 节点"
    );
    assert!(
        rt.selection.is_empty(),
        "删除唯一选中节点后 selection 应为空"
    );
    assert_eq!(
        rt.focus.mode(),
        InputMode::CanvasSelect,
        "删除聚焦节点后焦点应回到 CanvasSelect"
    );

    let missing_removed = rt.remove_node(999);
    assert!(
        !missing_removed,
        "remove_node(999) 应返回 false，不存在节点不能报告删除成功"
    );

    rt.kill_all();
}
