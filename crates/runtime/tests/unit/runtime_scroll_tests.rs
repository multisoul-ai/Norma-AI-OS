use super::*;
use crate::live::{NodeError, NodeScroll, NodeView};
use canvas::{geometry::CanvasRect, AgentProfile};
use terminal::PtyCommand;

/// Runtime::scroll_node 应把 TermLines 委托给 terminal-backed Agent live node。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent command 输出 5 行：A0/B1/C2/D3/E4
///   terminal size = 4 cols × 3 rows
///   history_size = 输出行数(5) - viewport 行数(3) = 2
///   NodeScroll::TermLines(2) → display_offset 从 0 增到 2
///   NodeScroll::TermLines(-2) → display_offset 从 2 回到 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. spawn_agent_with_command(Codex, command, 4, 3) 创建 terminal-backed live node
///   2. drain_all 直到子进程退出并把 PTY 输出喂给 TerminalGrid
///   3. 读取 node_view，确认初始 display_offset=0 且 history_size=2
///   4. scroll_node(node, TermLines(2)) → live.scroll → session.scroll_display
///   5. 再次读取 node_view，确认 display_offset=2
///   6. scroll_node(node, TermLines(-2)) 回到底部
///
/// 预期结果：
///   - 断言 A：第一次 scroll_node 返回 true，说明 terminal viewport 确实移动
///   - 断言 B：display_offset 变为 2，能回看两行 history
///   - 断言 C：向下滚动返回 true 且 display_offset 回到 0
///   - 负断言：scroll 后不应仍停留在 display_offset=0，否则 app 滚轮只被消费但内容不动
#[test]
fn test_scroll_node_delegates_term_lines_to_agent_live_node() {
    let mut rt = Runtime::new();
    let command =
        PtyCommand::new("/bin/sh").args(["-lc", "printf 'A0\\r\\nB1\\r\\nC2\\r\\nD3\\r\\nE4'"]);
    let node = rt
        .spawn_agent_with_command(
            AgentProfile::Codex,
            CanvasRect::new(0.0, 0.0, 320.0, 140.0),
            command,
            4,
            3,
        )
        .expect("spawn_agent_with_command 应成功");

    for _ in 0..50 {
        rt.drain_all();
        if let Some(NodeView::Term { status, .. }) = rt.node_view(node) {
            if !matches!(status, terminal::TerminalStatus::Running) {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let before = match rt
        .node_view(node)
        .expect("Agent live node 应暴露 Term view")
    {
        NodeView::Term { snapshot, .. } => snapshot,
    };
    assert_eq!(
        before.history_size, 2,
        "5 行输出和 3 行 viewport 应形成 2 行 history"
    );
    assert_eq!(
        before.display_offset, 0,
        "刚输出完成时 terminal viewport 应停在底部 display_offset=0"
    );

    let moved_up = rt
        .scroll_node(node, NodeScroll::TermLines(2))
        .expect("terminal-backed Agent 应支持 TermLines scroll");
    let after_up = match rt.node_view(node).expect("scroll 后仍应有 Term view") {
        NodeView::Term { snapshot, .. } => snapshot,
    };

    assert!(moved_up, "向上滚动两行应返回 true，表示 viewport 已移动");
    assert_eq!(
        after_up.display_offset, 2,
        "scroll_node(TermLines(2)) 应把 display_offset 移到 2"
    );
    assert_ne!(
        after_up.display_offset, before.display_offset,
        "scroll 后 display_offset 不应仍为 0，否则内容不会回看历史"
    );

    let moved_down = rt
        .scroll_node(node, NodeScroll::TermLines(-2))
        .expect("terminal-backed Agent 应支持向下回到底部");
    let after_down = match rt.node_view(node).expect("回到底部后仍应有 Term view") {
        NodeView::Term { snapshot, .. } => snapshot,
    };

    assert!(
        moved_down,
        "向下滚动两行应返回 true，表示 viewport 已回到底部"
    );
    assert_eq!(
        after_down.display_offset, 0,
        "scroll_node(TermLines(-2)) 应把 display_offset 恢复到 0"
    );

    rt.kill_all();
}

/// Runtime::scroll_node 的 TermWheel 在普通 terminal 下必须退回本地 scrollback。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent command 输出 5 行：A0/B1/C2/D3/E4
///   terminal size = 4 cols × 3 rows
///   history_size = 输出行数(5) - viewport 行数(3) = 2
///   NodeScroll::TermWheel{lines=2,row=1,col=2} 在非 alt_screen 下应等价于 TermLines(2)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. spawn_agent_with_command(OpenCode, command, 4, 3) 创建 terminal-backed Agent
///   2. drain_all 直到输出进入 grid，初始 display_offset=0
///   3. scroll_node(node, TermWheel{lines=2,row=1,col=2})
///   4. 读取 node_view，确认 display_offset=2
///
/// 预期结果：
///   - 断言 A：TermWheel 返回 true，说明普通 terminal scrollback 移动
///   - 断言 B：display_offset 从 0 变成 2
///   - 负断言：OpenCode 普通屏幕不应因为新 wheel API 停在 display_offset=0
#[test]
fn test_term_wheel_preserves_scrollback_for_plain_opencode_agent() {
    let mut rt = Runtime::new();
    let command =
        PtyCommand::new("/bin/sh").args(["-lc", "printf 'A0\\r\\nB1\\r\\nC2\\r\\nD3\\r\\nE4'"]);
    let node = rt
        .spawn_agent_with_command(
            AgentProfile::OpenCode,
            CanvasRect::new(0.0, 0.0, 320.0, 140.0),
            command,
            4,
            3,
        )
        .expect("OpenCode 测试 Agent 应成功 spawn");

    for _ in 0..50 {
        rt.drain_all();
        if let Some(NodeView::Term { status, .. }) = rt.node_view(node) {
            if !matches!(status, terminal::TerminalStatus::Running) {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let before = match rt
        .node_view(node)
        .expect("OpenCode Agent live node 应暴露 Term view")
    {
        NodeView::Term { snapshot, .. } => snapshot,
    };
    assert_eq!(
        before.display_offset, 0,
        "测试前置条件：OpenCode 普通屏幕应停在底部 display_offset=0"
    );

    let moved = rt
        .scroll_node(
            node,
            NodeScroll::TermWheel {
                lines: 2,
                row: 1,
                col: 2,
            },
        )
        .expect("terminal-backed OpenCode Agent 应支持 TermWheel");
    let after = match rt.node_view(node).expect("TermWheel 后仍应有 Term view") {
        NodeView::Term { snapshot, .. } => snapshot,
    };

    assert!(
        moved,
        "普通屏幕 TermWheel 应返回 true，表示 scrollback 已移动"
    );
    assert_eq!(
        after.display_offset, 2,
        "普通屏幕 TermWheel(lines=2) 应回看两行 history"
    );
    assert_ne!(
        after.display_offset, before.display_offset,
        "OpenCode 普通屏幕不应因新 wheel API 停在底部，否则现有 scrollback 被破坏"
    );

    rt.kill_all();
}

/// alternate screen + SGR mouse 下 TermWheel 应写 PTY mouse bytes，而不是移动本地 scrollback。
///
/// 数据构造（含关键数值的推导过程）：
///   capture_path = 临时文件，dd 从 PTY stdin 读取 10 bytes 写入该文件
///   启动序列：ESC[?1049h(alt_screen) + ESC[?1000h(mouse mode) + ESC[?1006h(SGR mouse)
///   TermWheel{lines=1,row=2,col=4} → SGR mouse bytes = ESC[<64;5;3M，长度 10
///   坐标推导：col 4 → x=5；row 2 → y=3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. spawn OpenCode Agent 替身：先输出 TUI mode 序列，再用 dd 等待 10 bytes
///   2. drain_all 直到 snapshot.alt_screen/mouse_mode/sgr_mouse 都为 true
///   3. scroll_node(node, TermWheel{lines=1,row=2,col=4})
///   4. 等待 dd 退出并读取 capture_path
///
/// 预期结果：
///   - 断言 A：capture_path 内容精确为 ESC[<64;5;3M
///   - 断言 B：snapshot.display_offset 保持 0，说明没有错误走本地 scrollback
///   - 负断言：capture 不应是 0-based 坐标 ESC[<64;4;2M
#[test]
fn test_term_wheel_writes_sgr_mouse_bytes_in_alt_screen() {
    let mut rt = Runtime::new();
    let capture_path = std::env::temp_dir().join(format!(
        "multisoul-term-wheel-{}-{}.bin",
        std::process::id(),
        42
    ));
    let _ = std::fs::remove_file(&capture_path);
    let command = PtyCommand::new("/bin/sh").args([
        "-lc",
        &format!(
            "stty raw -echo; printf '\\033[?1049h\\033[?1000h\\033[?1006h'; dd bs=10 count=1 of='{}' 2>/dev/null",
            capture_path.display()
        ),
    ]);
    let node = rt
        .spawn_agent_with_command(
            AgentProfile::OpenCode,
            CanvasRect::new(0.0, 0.0, 320.0, 140.0),
            command,
            80,
            24,
        )
        .expect("OpenCode TUI 替身应成功 spawn");

    let mut saw_tui_mode = false;
    for _ in 0..50 {
        rt.drain_all();
        if let Some(NodeView::Term { snapshot, .. }) = rt.node_view(node) {
            if snapshot.alt_screen && snapshot.mouse_mode && snapshot.sgr_mouse {
                saw_tui_mode = true;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        saw_tui_mode,
        "测试前置条件：OpenCode TUI 替身应进入 alt_screen + SGR mouse mode"
    );

    let wrote = rt
        .scroll_node(
            node,
            NodeScroll::TermWheel {
                lines: 1,
                row: 2,
                col: 4,
            },
        )
        .expect("alt screen OpenCode Agent 应支持 TermWheel");
    let after_scroll = match rt.node_view(node).expect("TermWheel 后仍应有 Term view") {
        NodeView::Term { snapshot, .. } => snapshot,
    };

    for _ in 0..50 {
        rt.drain_all();
        if std::fs::metadata(&capture_path)
            .map(|metadata| metadata.len() >= 10)
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let captured = std::fs::read(&capture_path).expect("dd 应捕获 TermWheel 写入的 PTY bytes");

    assert!(
        !wrote,
        "TUI TermWheel 写入 PTY 后不应立即声明 terminal view 已刷新，应等待 drain 输出"
    );
    assert_eq!(
        captured,
        b"\x1b[<64;5;3M".to_vec(),
        "alt screen SGR mouse wheel up 应写入 ESC[<64;5;3M"
    );
    assert_eq!(
        after_scroll.display_offset, 0,
        "alt screen TUI wheel 不应移动本地 scrollback display_offset"
    );
    assert_ne!(
        captured,
        b"\x1b[<64;4;2M".to_vec(),
        "SGR mouse wheel 不应写入 0-based 坐标，否则 OpenCode 命中点会偏移"
    );

    rt.kill_all();
    let _ = std::fs::remove_file(&capture_path);
}

/// Runtime::scroll_node 对没有 live node 的目标应返回 MissingNode。
///
/// 数据构造（含关键数值的推导过程）：
///   missing_id = 999：document/live 中都不存在
///   preview_id：spawn_preview_from_path 只创建 CanvasNodeKind::Preview，不注册 live node
///   NodeScroll::TermLines(1)：尝试滚动一行
///
/// 执行过程（逐步说明系统如何处理）：
///   1. scroll_node(999, TermLines(1)) 查询 live registry
///   2. spawn_preview_from_path 创建 Preview 描述节点
///   3. scroll_node(preview_id, TermLines(1)) 再次查询 live registry
///
/// 预期结果：
///   - 断言 A：缺失 id 返回 NodeError::MissingNode
///   - 断言 B：Preview id 返回 NodeError::MissingNode，因为 Preview 没有 terminal live runtime
///   - 负断言：Preview 不应返回 Ok(true)，否则 app 会误以为有内部 terminal scrollback
#[test]
fn test_scroll_node_missing_and_preview_return_missing_node() {
    let mut rt = Runtime::new();
    let missing = rt.scroll_node(999, NodeScroll::TermLines(1));
    assert!(
        matches!(missing, Err(NodeError::MissingNode)),
        "缺失 live node 的滚动请求应返回 MissingNode，实际: {missing:?}"
    );

    let preview = rt.spawn_preview_from_path(
        std::path::PathBuf::from("/tmp/report.md"),
        None,
        CanvasRect::new(0.0, 0.0, 320.0, 220.0),
    );
    let preview_scroll = rt.scroll_node(preview, NodeScroll::TermLines(1));

    assert!(
        matches!(preview_scroll, Err(NodeError::MissingNode)),
        "Preview 没有 live node，应返回 MissingNode，实际: {preview_scroll:?}"
    );
    assert!(
        !matches!(preview_scroll, Ok(true)),
        "Preview 不应报告 terminal scroll 成功，否则画布会误以为内容已滚动"
    );
}
