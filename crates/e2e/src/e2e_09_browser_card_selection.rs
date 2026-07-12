//! E2E-09：Browser 卡片内容区的选择状态闭环（L1 headless GPUI）。
//!
//! L1 不能构造 macOS 原生 WebView first responder；真实网页点击到 BrowserEvent 的桥接由
//! app crate B-11 单元测试覆盖。本用例覆盖该事件落入 Runtime 后的用户可见选择语义：
//! Browser 内容区被点中时选中它，点回画布空白处时清除选择。

use app::NewBrowser;
use canvas::{CanvasNodeKind, NodeId};
use gpui::{point, px, Modifiers, TestAppContext, VisualTestContext};
use runtime::InputMode;

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// E2E-09：用户选中 Browser 内容区后获得唯一选择，点击空白画布后清除选择。
///
/// 数据构造（含关键数值的推导过程）：
///   - `NewBrowser` 只创建 1 张 Browser 卡片；因此 Browser node_id 在 document 中唯一。
///   - 空白点击点 = canvas 左上角 + (2px, 2px)；Browser 创建时占用的是
///     `visible_canvas_rect().inset(layout_gap)`，layout_gap 远大于 2px，故该点不在卡片内。
///   - Browser 内容区 selector = `browser-native-surface`；L1 禁用 macOS child WebView，
///     但保留同一张卡片的 GPUI hitbox，保证点击和 Runtime 选择状态可确定性验证。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入空 Canvas 后通过公开 NewBrowser action 创建 Browser 卡片。
///   2. 点击 canvas 左上角空白处，清除 NewBrowser 默认选择。
///   3. 点击 Browser 内容区，画布路由选择该 Browser，渲染层据 selection 显示蓝色边框。
///   4. 再点空白画布，路由清除选择，卡片不应继续保留选中态。
///
/// 预期结果：
///   - 正断言：内容区点击后 selection 精确只含 Browser node_id。
///   - 正断言：焦点模式为 CanvasSelect，Browser 不会伪装成可接收 PTY 输入的 NodeFocused。
///   - 负断言：空白画布点击后 selection 不含 Browser node_id。
///   - 负断言：空白画布点击后 selection 为空，不得残留不可见的选中状态。
#[test]
fn e2e_09_browser_content_selection_is_visible_and_clearable() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let browser_id = create_browser(&app, cx);

    click_canvas_blank(cx);
    let selection_after_blank = read_runtime(&app, cx, |runtime| {
        runtime.selection.iter().collect::<Vec<_>>()
    });
    assert!(
        selection_after_blank.is_empty(),
        "点击空白画布后选择集必须为空，actual={selection_after_blank:?}"
    );
    assert!(
        !selection_after_blank.contains(&browser_id),
        "空白画布点击后不得残留 Browser({browser_id}) 的选中态，actual={selection_after_blank:?}"
    );

    click_debug(cx, "browser-native-surface");
    let (selection_after_browser, focus_mode) = read_runtime(&app, cx, |runtime| {
        (
            runtime.selection.iter().collect::<Vec<_>>(),
            runtime.focus.mode(),
        )
    });
    assert_eq!(
        selection_after_browser,
        vec![browser_id],
        "点击 Browser 内容区后 selection 必须唯一指向 Browser({browser_id})，actual={selection_after_browser:?}"
    );
    assert_eq!(
        focus_mode,
        InputMode::CanvasSelect,
        "Browser 内容区被选中后应保持 CanvasSelect，不能误进入 NodeFocused，actual={focus_mode:?}"
    );
    assert_ne!(
        focus_mode,
        InputMode::NodeFocused(browser_id),
        "Browser 不是 PTY live node，点击内容区后不得伪造 NodeFocused({browser_id})"
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

fn click_canvas_blank(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    let canvas = cx
        .debug_bounds("canvas")
        .expect("canvas 应在进入 workspace 后具有可点击 bounds");
    let position = point(canvas.left() + px(2.0), canvas.top() + px(2.0));
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx.debug_bounds(selector).unwrap_or_else(|| {
        panic!("selector `{selector}` 应在 Browser 卡片渲染后拥有 debug bounds")
    });
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}
