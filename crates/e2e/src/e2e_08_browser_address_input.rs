//! E2E-08：Browser 地址栏的系统文本输入闭环（L1 headless GPUI）。
//!
//! 浏览器网页本身是 macOS 原生 WebView，不能在 headless 测试中构造真实 first
//! responder；`new_with_pty_factory` 因此只为该测试构造路径禁用 native child view。
//! 本用例覆盖其上方 GPUI 地址栏的真实点击、平台文本输入、选区、复制、粘贴和 Enter
//! 提交。WebView 原生焦点释放由 app crate 的 B-7 单元测试覆盖两个后端调用链。

use app::NewBrowser;
use canvas::{CanvasNodeKind, NodeId};
use gpui::{ClipboardItem, Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// E2E-08：地址栏点击后可用平台文本输入写入 Unicode URL，并支持复制、粘贴替换和提交。
///
/// 数据构造（含关键数值的推导过程）：
///   - `NewBrowser` 创建 1 张 Browser 卡片；地址栏 debug selector 为
///     `browser-address`，来自卡片的真实渲染树。测试仅创建一张 Browser 卡片，
///     因而 selector 唯一。
///   - IME 文本 = `https://例子.test/搜索`，含 2 个中文字符，不能由 ASCII KeyDown
///     解析得到；用于验证平台文本输入路径。
///   - 粘贴文本 = `https://paste.example.test/path`，与 IME 文本完全不同，确保
///     `Cmd+A + Cmd+V` 是替换而非追加。
///   - `cmd-a` 选择整段文本，`cmd-c`/`cmd-v` 分别验证标准复制与写入命令；
///     `enter` 最后提交到 Browser node 的已保存 URL。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless app，进入空 Canvas，通过公开 `NewBrowser` action 创建卡片。
///   2. 点击真实地址栏 hitbox；InputState 获得焦点并注册平台文本输入 handler。
///   3. `cmd-a` 后 `simulate_input(IME 文本)`；按一次 Backspace 删除最后的“索”，
///      再用平台文本输入恢复它，读取剪贴板前先 `cmd-a + cmd-c`。
///   4. 写入粘贴文本并发送 `cmd-v enter`；输入实体替换内容、发出 Submit，
///      app 规范化 URL、更新 Browser node 并退出编辑态。
///
/// 预期结果：
///   - 正断言：复制结果精确等于 Unicode IME 文本，证明选区与复制命令走 InputState。
///   - 正断言：最终 Browser URL 精确等于粘贴 URL，证明粘贴与 Enter 提交生效。
///   - 负断言：最终 URL 不含 Unicode 旧文本，证明不是追加或遗留手工 draft。
///   - 负断言：最终 URL 不应是默认 Google 首页，证明用户写入确实覆盖了初始值。
#[test]
fn e2e_08_browser_address_accepts_ime_text_and_standard_editing_commands() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_browser(&app, cx);

    click_debug(cx, "browser-address");

    let ime_text = "https://例子.test/搜索";
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input(ime_text);
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("索");
    cx.simulate_keystrokes("cmd-a cmd-c");
    let copied = cx.update(|_window, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(
        copied.as_deref(),
        Some(ime_text),
        "Cmd+A + Cmd+C 必须从地址栏复制完整 Unicode IME 文本，actual={copied:?}"
    );

    let pasted_url = "https://paste.example.test/path";
    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(pasted_url.to_string()));
    });
    cx.simulate_keystrokes("cmd-v enter");
    cx.run_until_parked();

    let saved_url = read_runtime(&app, cx, |runtime| browser_url(runtime, node_id));
    assert_eq!(
        saved_url, pasted_url,
        "地址栏 Cmd+V + Enter 必须保存精确粘贴 URL，actual={saved_url}"
    );
    assert!(
        !saved_url.contains("例子.test"),
        "粘贴替换后不得残留先前 IME URL，actual={saved_url}"
    );
    assert_ne!(
        saved_url, "https://www.google.com",
        "用户输入后 Browser URL 不应仍是默认首页"
    );
}

fn create_browser(view: &gpui::Entity<app::MultisoulApp>, cx: &mut VisualTestContext) -> NodeId {
    let max_before = read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .map(|node| node.id)
            .max()
            .unwrap_or(0)
    });
    cx.dispatch_action(NewBrowser);
    cx.run_until_parked();
    read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .map(|node| node.id)
            .max()
            .filter(|node_id| *node_id > max_before)
            .expect("NewBrowser 应创建 id 大于已有 Browser 的新节点")
    })
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应在地址栏渲染后拥有 debug bounds"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

fn browser_url(runtime: &runtime::Runtime, node_id: NodeId) -> String {
    let node = runtime
        .document
        .node(node_id)
        .expect("NewBrowser 创建的节点必须仍在 document 中");
    let CanvasNodeKind::Browser(spec) = &node.kind else {
        panic!("id {node_id} 必须是 Browser 节点，actual={:?}", node.kind);
    };
    spec.url.clone()
}
