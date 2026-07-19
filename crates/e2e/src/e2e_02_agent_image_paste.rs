//! Agent 图片粘贴 L1 验收：不同 CLI 卡片必须使用各自支持的输入协议。

use std::time::Duration;

use canvas::NodeId;
use gpui::{ClipboardItem, Image, ImageFormat, TestAppContext, VisualTestContext};

use crate::harness::{
    create_claude_agent, create_codex_agent, create_cursor_agent, drain_until, enter_canvas,
    make_app, read_grid, scripted_factory,
};

/// Cursor 保持路径格式，Codex 与 Claude 使用各自的原生图片附件快捷键。
///
/// 数据构造（含关键数值的推导过程）：
///   clipboard image = PNG magic `\x89PNG`，共 4 bytes
///   native image shortcut = Ctrl+V = `'v'(0x76) & 0x1f = 0x16`，共 1 byte
///   native rule = `[0x16]` → `"[Image #1]\r\n"`，模拟 Codex/Claude 的原生附件占位
///   path rule = `"multisoul-clipboard-images"` → `"PATH_INPUT\r\n"`，模拟 Cursor 路径格式
///   Agent cards = 3：Cursor、Codex、Claude 各 1 张
///   每次 drain timeout = 500ms；500ms / 12ms ≈ 41 轮，足够 L1 同步回吐收敛
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以同一 scripted PTY 工厂启动 app，进入临时 workspace
///   2. 创建 Cursor，写入 PNG 后按 Cmd+V，读取该卡片 grid
///   3. 创建 Codex，写入同一 PNG 后按 Cmd+V，读取该卡片 grid
///   4. 创建 Claude，写入同一 PNG 后按 Cmd+V，读取该卡片 grid
///   5. 对比三张卡片的可观察输出，验证 profile 到输入协议的映射
///
/// 预期结果：
///   - 正断言：Cursor 出现 `PATH_INPUT`，保持当前可用的路径格式
///   - 负断言：Cursor 不出现 `[Image #1]`，不得被错误切换成原生快捷键
///   - 正断言：Codex 与 Claude 都出现 `[Image #1]`，证明原生图片入口被触发
///   - 负断言：Codex 与 Claude 都不出现 `PATH_INPUT`，不得继续显示裸路径
#[test]
fn e2e_02_agent_image_paste_uses_profile_specific_protocols() {
    let mut cx = TestAppContext::single();
    let rules = vec![
        (vec![0x16], b"[Image #1]\r\n".to_vec()),
        (
            b"multisoul-clipboard-images".to_vec(),
            b"PATH_INPUT\r\n".to_vec(),
        ),
    ];
    let (app, cx) = make_app(&mut cx, scripted_factory(rules));
    let workspace = tempfile::TempDir::new().expect("Agent 图片粘贴测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let image = Image::from_bytes(ImageFormat::Png, b"\x89PNG".to_vec());

    let cursor = create_cursor_agent(&app, cx);
    let cursor_text = paste_image_and_observe(&app, cx, cursor, &image);
    assert!(
        cursor_text.contains("PATH_INPUT"),
        "Cursor 图片粘贴应保持路径输入格式，实际 grid: {cursor_text:?}"
    );
    assert!(
        !cursor_text.contains("[Image #1]"),
        "Cursor 不应被错误切换成 Codex/Claude 原生附件快捷键，实际 grid: {cursor_text:?}"
    );

    let codex = create_codex_agent(&app, cx);
    let codex_text = paste_image_and_observe(&app, cx, codex, &image);
    assert!(
        codex_text.contains("[Image #1]"),
        "Codex 图片粘贴应进入原生附件管线，实际 grid: {codex_text:?}"
    );
    assert!(
        !codex_text.contains("PATH_INPUT"),
        "Codex 图片粘贴不得继续发送临时路径，实际 grid: {codex_text:?}"
    );

    let claude = create_claude_agent(&app, cx);
    let claude_text = paste_image_and_observe(&app, cx, claude, &image);
    assert!(
        claude_text.contains("[Image #1]"),
        "Claude 图片粘贴应进入原生附件管线，实际 grid: {claude_text:?}"
    );
    assert!(
        !claude_text.contains("PATH_INPUT"),
        "Claude 图片粘贴不得继续发送临时路径，实际 grid: {claude_text:?}"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

fn paste_image_and_observe(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut VisualTestContext,
    node_id: NodeId,
    image: &Image,
) -> String {
    cx.update(|_window, cx| cx.write_to_clipboard(ClipboardItem::new_image(image)));
    cx.simulate_keystrokes("cmd-v");

    // 500ms / 12ms ≈ 41 轮；出现任一协议标记即可停止，避免失败分支等满超时。
    drain_until(
        app,
        cx,
        |runtime| {
            let snapshot = match runtime.node_view(node_id) {
                Some(runtime::NodeView::Term { snapshot, .. }) => snapshot,
                _ => return None,
            };
            let text: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            (text.contains("[Image #1]") || text.contains("PATH_INPUT")).then_some(text)
        },
        Duration::from_millis(500),
    )
    .unwrap_or_else(|| {
        let grid =
            read_grid(app, cx, node_id).expect("Agent 图片粘贴超时后仍应能读取 terminal grid");
        let text: String = grid.rows.iter().flatten().map(|cell| cell.c).collect();
        panic!("500ms 内未观察到任何图片输入协议标记，实际 grid: {text:?}")
    })
}
