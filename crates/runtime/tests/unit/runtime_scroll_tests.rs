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
