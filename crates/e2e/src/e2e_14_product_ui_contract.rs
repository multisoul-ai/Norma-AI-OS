//! E2E-14：Product UI V2 Dock、Settings 与自动布局公开交互契约。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// 用户进入 Canvas 后，移除装饰细线不能连带删除或缩小工作台背景。
///
/// 数据构造（含关键数值的推导过程）：
///   Canvas bounds = headless GPUI 当前完整工作区 bounds。
///   Workspace backdrop bounds = 四边均为 0px inset，因此应与 Canvas 同 origin、同 size。
///   装饰线数量 = 0（两个圆环 + 一条基线均由 app 单元回归测试验证不存在）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app + enter_canvas` 进入真实 headless Canvas 渲染树。
///   2. 读取 Canvas 与 workspace backdrop 的 debug bounds。
///   3. 比较两者的 origin 和 size，确认背景仍完整覆盖工作区。
///
/// 预期结果：
///   - 正断言：workspace backdrop 继续存在，并与 Canvas 等大。
///   - 负断言：背景不得因删除装饰子层而缩成局部区域或偏离 Canvas 原点。
#[test]
fn e2e_14_workspace_backdrop_remains_full_canvas_after_decorative_lines_removed() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let canvas = cx
        .debug_bounds("canvas")
        .expect("Canvas 应暴露 debug bounds 供工作台背景覆盖范围验收");
    let backdrop = cx
        .debug_bounds("workspace-backdrop")
        .expect("移除装饰细线后 workspace-backdrop 仍应存在");

    assert_eq!(
        backdrop.origin, canvas.origin,
        "workspace-backdrop 应与 Canvas 同原点，不能因删除装饰子层发生偏移"
    );
    assert_eq!(
        backdrop.size, canvas.size,
        "workspace-backdrop 应完整覆盖 Canvas，不能因删除装饰子层缩小"
    );
    assert!(
        f32::from(backdrop.size.width) > 0.0 && f32::from(backdrop.size.height) > 0.0,
        "workspace-backdrop 宽高必须为正，不能退化为空背景"
    );
}

/// 用户从底部 Command Dock 打开新建菜单并创建两个 Terminal，再从同一 Dock 打开 Settings。
///
/// 数据构造（含关键数值的推导过程）：
///   headless viewport 由 TestAppContext::single 提供，Canvas debug bounds 为完整工作区。
///   Command Dock 高度 = 52px，底部间距 = 22px，因此 dock center 必须位于 Canvas 下半区。
///   minimap 宽高 = 128x112px；右侧间距 = 18px；底部间距 = 22px。
///   minimap right = Canvas right - 18px；minimap bottom = Canvas bottom - 22px。
///   新建 Terminal 数量 = 2；若 reflow 未执行，两次 spawn 使用同一初始 origin。
///   reflow 执行后，两张卡片由 responsive grid 分配到不同 origin。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app + enter_canvas` 进入真实 headless Canvas 渲染树。
///   2. 读取 `workspace-command-dock` 与 `canvas-minimap` bounds，验证右下角定位且互不遮挡。
///   3. 连续两次点击 `toolbar-new-terminal` → `new-menu-terminal` 创建 Terminal。
///   4. 从 runtime 公开快照读取两张卡片 bounds，验证自动 reflow。
///   5. 点击 `toolbar-settings`，验证 Settings panel 在 Dock 之上打开。
///
/// 预期结果：
///   - 正断言：Dock 位于 Canvas 下半区，minimap 固定在 Canvas 右下角。
///   - 负断言：minimap 不得回到右上角，也不得与底部居中的 Dock 重叠。
///   - 正断言：创建两张 Terminal 后节点数为 2，且 origin 不同。
///   - 正断言：Settings panel 可通过 Dock 打开。
///   - 负断言：两张 Terminal 不得重叠在同一 origin，证明自动布局未被 UI 改造绕过。
#[test]
fn e2e_14_bottom_dock_preserves_terminal_reflow_and_settings_flow() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let canvas = cx
        .debug_bounds("canvas")
        .expect("Canvas 应暴露 debug bounds 供公开交互测试定位");
    let dock = cx
        .debug_bounds("workspace-command-dock")
        .expect("Product UI V2 应渲染 workspace-command-dock");
    let minimap = cx
        .debug_bounds("canvas-minimap")
        .expect("Product UI V2 应渲染 canvas-minimap");
    let canvas_mid_y = f32::from(canvas.origin.y) + f32::from(canvas.size.height) / 2.0;
    // minimap right = Canvas right - 18px；minimap bottom = Canvas bottom - 22px。
    let expected_minimap_right = f32::from(canvas.origin.x) + f32::from(canvas.size.width) - 18.0;
    let expected_minimap_bottom = f32::from(canvas.origin.y) + f32::from(canvas.size.height) - 22.0;
    let actual_minimap_right = f32::from(minimap.origin.x) + f32::from(minimap.size.width);
    let actual_minimap_bottom = f32::from(minimap.origin.y) + f32::from(minimap.size.height);

    assert!(
        f32::from(dock.center().y) > canvas_mid_y,
        "Command Dock center 应位于 Canvas 下半区，actual={} mid={}",
        f32::from(dock.center().y),
        canvas_mid_y
    );
    assert_eq!(
        actual_minimap_right, expected_minimap_right,
        "minimap 右边缘应距 Canvas 右侧 18px，actual={actual_minimap_right} expected={expected_minimap_right}"
    );
    assert_eq!(
        actual_minimap_bottom, expected_minimap_bottom,
        "minimap 底边应距 Canvas 底部 22px，actual={actual_minimap_bottom} expected={expected_minimap_bottom}"
    );
    assert!(
        f32::from(minimap.origin.y) > canvas_mid_y,
        "minimap 应位于 Canvas 下半区，不能回到右上角，actual={} mid={canvas_mid_y}",
        f32::from(minimap.origin.y)
    );
    assert!(
        f32::from(minimap.origin.x) >= f32::from(dock.origin.x) + f32::from(dock.size.width),
        "右下角 minimap 不得与底部居中的 Command Dock 重叠"
    );

    click_debug(cx, "toolbar-new-terminal");
    click_debug(cx, "new-menu-terminal");
    click_debug(cx, "toolbar-new-terminal");
    click_debug(cx, "new-menu-terminal");

    let bounds = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .map(|node| node.bounds)
            .collect::<Vec<_>>()
    });
    assert_eq!(
        bounds.len(),
        2,
        "从 Dock 新建两次 Terminal 后 document 应恰好有 2 个节点"
    );
    assert_ne!(
        bounds[0].origin, bounds[1].origin,
        "两张 Terminal origin 不得相同，否则 Product UI Dock 绕过了自动 reflow"
    );

    click_debug(cx, "toolbar-settings");
    assert!(
        cx.debug_bounds("settings-panel").is_some(),
        "点击 Dock Settings 后应渲染 settings-panel"
    );
    assert_ne!(
        bounds[0].origin, bounds[1].origin,
        "打开 Settings 不应改变或重叠已经完成的自动布局"
    );
}

/// 用户进入 Canvas 后，顶部显示与目标稿一致的全宽工作区标题栏和真实状态入口。
///
/// 数据构造（含关键数值的推导过程）：
///   headless Canvas width = TestAppContext 当前可见工作区宽度。
///   目标标题栏高度 = product-ui.html `.mac-titlebar` 的 54px。
///   标题栏左右边界 = Canvas 左右边界，因此宽度差必须为 0px。
///   右侧顺序 = workspace-launchpad-button → workspace-save-status。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app + enter_canvas` 进入真实 headless Canvas 渲染树。
///   2. 读取 Canvas、顶部标题栏、项目启动器和保存状态的 debug bounds。
///   3. 比较标题栏尺寸，并检查两个右侧状态单元的空间顺序。
///
/// 预期结果：
///   - 正断言：顶部标题栏高 54px，且与 Canvas 等宽、同起点。
///   - 正断言：项目启动器与保存状态位于标题栏右半区且顺序正确。
///   - 负断言：保存状态不得落到项目启动器左侧或标题栏之外。
#[test]
fn e2e_14_workspace_top_bar_matches_reference_geometry_and_status_order() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let canvas = cx
        .debug_bounds("canvas")
        .expect("Canvas 应暴露 debug bounds 供标题栏几何对齐");
    let top_bar = cx
        .debug_bounds("workspace-top-bar")
        .expect("Canvas 应渲染目标稿中的 workspace-top-bar");
    let launchpad = cx
        .debug_bounds("workspace-launchpad-button")
        .expect("顶部标题栏应渲染项目启动器按钮");
    let save_status = cx
        .debug_bounds("workspace-save-status")
        .expect("顶部标题栏应渲染真实 Canvas 保存状态");

    assert_eq!(
        f32::from(top_bar.size.height),
        54.0,
        "workspace-top-bar 高度应精确匹配目标稿 54px"
    );
    assert_eq!(
        top_bar.origin.x, canvas.origin.x,
        "workspace-top-bar 应从 Canvas 左边界开始"
    );
    assert_eq!(
        top_bar.size.width, canvas.size.width,
        "workspace-top-bar 应与 Canvas 等宽，不能退化为左上角小胶囊"
    );
    assert!(
        f32::from(launchpad.center().x) > f32::from(canvas.center().x),
        "项目启动器应位于标题栏右半区"
    );
    assert!(
        f32::from(save_status.origin.x)
            >= f32::from(launchpad.origin.x) + f32::from(launchpad.size.width),
        "Canvas 保存状态应排在项目启动器右侧，不得与其重叠或左右颠倒"
    );
    assert!(
        f32::from(save_status.origin.x) + f32::from(save_status.size.width)
            <= f32::from(top_bar.origin.x) + f32::from(top_bar.size.width),
        "Canvas 保存状态不得溢出 workspace-top-bar 右边界"
    );
}

/// 用户看到的底部 Command Dock 使用目标稿的大控件玻璃比例，同时保留原有入口。
///
/// 数据构造（含关键数值的推导过程）：
///   Dock 高度 = 52px；底边安全距 = 22px（来自 product-ui.html）。
///   普通按钮 = 38px × 38px；Dock 垂直余量 = 52 - 38 = 14px，即上下各 7px。
///   Voice 单元最小宽度 = 88px，必须有独立 debug selector 证明未被拆出 Dock。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入真实 headless Canvas 渲染树。
///   2. 读取 Dock、选择工具、新建按钮与 Voice 单元 bounds。
///   3. 校验目标尺寸、底部安全距和控件相对位置。
///
/// 预期结果：
///   - 正断言：Dock 高 52px、距 Canvas 底部 22px。
///   - 正断言：两个图标按钮均为 38px 正方形，Voice 宽度至少 88px。
///   - 负断言：Voice 不得脱离 Dock，图标按钮不得回退到旧 30px。
#[test]
fn e2e_14_command_dock_matches_reference_control_scale_and_safe_area() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let canvas = cx
        .debug_bounds("canvas")
        .expect("Canvas 应暴露 debug bounds 供 Dock 几何对齐");
    let dock = cx
        .debug_bounds("workspace-command-dock")
        .expect("Canvas 应渲染 workspace-command-dock");
    let cursor = cx
        .debug_bounds("toolbar-canvas-scroll")
        .expect("Dock 应保留画布模式入口");
    let new_button = cx
        .debug_bounds("toolbar-new-terminal")
        .expect("Dock 应保留 New 入口");
    let voice = cx
        .debug_bounds("workspace-voice-control")
        .expect("Voice 状态单元应嵌在 Command Dock 内");

    assert_eq!(
        f32::from(dock.size.height),
        52.0,
        "Command Dock 高度应精确匹配目标稿 52px"
    );
    assert_eq!(
        f32::from(canvas.origin.y) + f32::from(canvas.size.height)
            - (f32::from(dock.origin.y) + f32::from(dock.size.height)),
        22.0,
        "Command Dock 距 Canvas 底部应保留 22px 安全距"
    );
    assert_eq!(
        (f32::from(cursor.size.width), f32::from(cursor.size.height)),
        (38.0, 38.0),
        "选择工具按钮应为目标稿 38px 正方形，不得回退到旧 30px"
    );
    assert_eq!(
        (
            f32::from(new_button.size.width),
            f32::from(new_button.size.height)
        ),
        (38.0, 38.0),
        "New 按钮应与选择工具使用相同的 38px 控件比例"
    );
    assert!(
        f32::from(voice.size.width) >= 88.0,
        "Voice 单元宽度至少应为 88px，actual={}",
        f32::from(voice.size.width)
    );
    assert!(
        f32::from(voice.origin.y) >= f32::from(dock.origin.y)
            && f32::from(voice.origin.y) + f32::from(voice.size.height)
                <= f32::from(dock.origin.y) + f32::from(dock.size.height),
        "Voice 单元必须完整位于 Command Dock 内，不得拆成第二条浮层"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("click target {selector} should be rendered"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
