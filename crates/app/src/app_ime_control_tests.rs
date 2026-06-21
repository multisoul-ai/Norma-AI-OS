use super::*;
use canvas::CanvasRect;

/// InputHandler 传来的单独换行控制文本不应写入 PTY，真实文本换行仍允许。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id 来自 spawn_terminal，document.next_id 初始为 1，因此 node_id = 1
///   lf_text = "\n" → 单独 LF(0x0a)，常见于 Enter 被文本化的 input callback
///   cr_text = "\r" → 单独 CR(0x0d)，另一类 Enter 文本化形式
///   command_text = "printf OK; exit\n" → 包含换行但长度 > 1，是真实命令文本提交
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 新建 /bin/sh terminal 并 focus node_id
///   2. commit_text_to_focused_node(Canvas, lf_text) → 应被识别为控制键文本回调并忽略
///   3. commit_text_to_focused_node(Canvas, cr_text) → 同样忽略
///   4. commit_text_to_focused_node(Canvas, command_text) → 仍作为真实文本写入 PTY
///
/// 预期结果：
///   - 正断言：command_text 返回 true，证明包含换行的真实文本未被误杀
///   - 负断言：lf_text/cr_text 返回 false，避免 Enter 在 Codex/Claude Code 中变成文本区换行
///   - 正断言：runtime 仍只有一个 live node，忽略控制文本不会创建或删除节点
#[test]
fn test_single_newline_text_commit_is_ignored_but_multichar_text_is_allowed() {
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_terminal(CanvasRect::new(0.0, 0.0, 640.0, 360.0), "/bin/sh", 80, 24)
        .expect("test should spawn /bin/sh terminal");
    runtime.focus.focus_node(node_id);

    let lf_committed = commit_text_to_focused_node(&mut runtime, AppScreen::Canvas, "\n");
    let cr_committed = commit_text_to_focused_node(&mut runtime, AppScreen::Canvas, "\r");
    let command_committed =
        commit_text_to_focused_node(&mut runtime, AppScreen::Canvas, "printf OK; exit\n");

    assert!(
        !lf_committed,
        "单独 LF 文本提交应被忽略，否则 Enter 会作为文本换行写入 TUI"
    );
    assert!(
        !cr_committed,
        "单独 CR 文本提交应被忽略，Enter 控制键应由 keydown 路径处理"
    );
    assert!(
        command_committed,
        "包含换行的真实多字符文本仍应允许写入 PTY，避免破坏粘贴/批量输入"
    );
    assert_eq!(
        runtime.live_len(),
        1,
        "忽略单独换行文本不应创建或删除 live node"
    );

    runtime.kill_all();
}
