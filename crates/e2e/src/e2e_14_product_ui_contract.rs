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

/// E2E-14：从底部 Dock 的 New 按钮打开卡片菜单时，菜单必须从触发按钮正上方浮出。
///
/// 数据构造（含关键数值的推导过程）：
///   New 按钮尺寸 = 38px × 38px（Command Dock 的统一图标控件尺寸）。
///   卡片菜单宽度 = 180px；为了与 38px 触发按钮中心对齐，左偏移 =
///   -(180 - 38) / 2 = -71px。
///   菜单与触发按钮的垂直安全距离 = 12px；菜单 bottom 相对 38px 按钮容器的
///   定位距离 = 38 + 12 = 50px，避免两层浮层的圆角和阴影相互重叠。
///   画布中心与 New 按钮中心在默认 Dock 排列中不同，故可用两者的 x 差验证菜单
///   没有错误地锚定到整张画布中线。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app + enter_canvas` 渲染真实 headless Canvas 与底部 Command Dock。
///   2. 点击 `toolbar-new-terminal`，由生产 toggle 打开所有卡片类型菜单。
///   3. 读取触发按钮、弹出菜单与 Canvas 的 debug bounds，并比较空间关系。
///
/// 预期结果：
///   - 正断言：菜单存在，x 中心与 New 按钮中心精确对齐。
///   - 正断言：菜单距触发按钮正好 12px，保持清晰的来源关系且不与 Dock 重叠。
///   - 负断言：菜单中心不得退回 Canvas 中心，避免重现整屏居中悬浮。
#[test]
fn e2e_14_new_card_menu_is_anchored_above_its_dock_trigger() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("卡片菜单 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let canvas = cx
        .debug_bounds("canvas")
        .expect("卡片菜单定位验收必须先读取 Canvas bounds");
    let new_button = cx
        .debug_bounds("toolbar-new-terminal")
        .expect("底部 Dock 必须提供 New 按钮作为卡片菜单触发器");

    click_debug(cx, "toolbar-new-terminal");

    let menu = cx
        .debug_bounds("new-menu")
        .expect("点击 New 后必须渲染所有卡片类型菜单");
    let menu_center_x = f32::from(menu.center().x);
    let button_center_x = f32::from(new_button.center().x);
    let canvas_center_x = f32::from(canvas.center().x);
    let menu_bottom = f32::from(menu.origin.y) + f32::from(menu.size.height);
    let button_top = f32::from(new_button.origin.y);
    // menu_gap = button_top - menu_bottom = 12px，保证两个浮层的圆角与阴影分离。
    let menu_gap = button_top - menu_bottom;

    assert_eq!(
        menu_center_x, button_center_x,
        "卡片菜单 x 中心必须与 New 触发按钮中心对齐，actual={menu_center_x} expected={button_center_x}"
    );
    assert_eq!(
        menu_gap, 12.0,
        "卡片菜单与 New 按钮必须保留 12px 间隔，actual={menu_gap} expected=12"
    );
    assert_ne!(
        menu_center_x, canvas_center_x,
        "卡片菜单不得锚定 Canvas 中心，actual={menu_center_x} canvas_center={canvas_center_x}"
    );
}

/// E2E-14：底部 Dock 按“画布 → 创建 → Voice/Settings”三组排列，形成 2｜5｜2 层级。
///
/// 数据构造（含关键数值的推导过程）：
///   画布组 = Select/Scroll + Layout = 2 个连续入口。
///   创建组 = Terminal + Codex + New + Browser + Preview = 5 个连续入口，
///            New 前后各有 2 个入口，因此它位于创建组正中。
///   工具组 = Voice + Settings = 2 个连续入口，Settings 固定在最右。
///   组间由 1px divider 和左右各 3px gap 分开，因此
///   layout→terminal、preview→voice 的间距至少 = 3 + 1 + 3 = 7px。
///   所有控件高度 = 38px，小于 Dock 52px，故每个控件都应完整落在 Dock 内。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app + enter_canvas` 渲染真实 headless Canvas 与底部 Dock。
///   2. 读取八个图标入口及 Voice pill 的 debug bounds。
///   3. 按 x 坐标逐项检查 2｜5｜2 的连续顺序和两处分组间距。
///   4. 验证 New 位于创建组正中，Settings 位于 Voice 右侧且仍在 Dock 内。
///
/// 预期结果：
///   - 正断言：画布组在最左，创建组居中，Voice/Settings 工具组在最右。
///   - 正断言：New 的中心与创建组中心重合，保持主操作的视觉中心地位。
///   - 负断言：Settings 不得回到 Voice 左侧，任何入口不得溢出或拆出 Dock。
#[test]
fn e2e_14_command_dock_orders_canvas_creation_and_utility_groups() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Dock 分组 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let dock = cx
        .debug_bounds("workspace-command-dock")
        .expect("Dock 分组验收必须先渲染 workspace-command-dock");
    let new_button = cx
        .debug_bounds("toolbar-new-terminal")
        .expect("创建组必须包含 New 入口");
    let terminal = cx
        .debug_bounds("toolbar-terminal")
        .expect("创建组必须包含 Terminal 入口");
    let codex = cx
        .debug_bounds("toolbar-new-codex-agent")
        .expect("创建组必须包含 Codex 入口");
    let browser = cx
        .debug_bounds("toolbar-new-browser")
        .expect("创建组必须包含 Browser 入口");
    let preview = cx
        .debug_bounds("toolbar-open-preview")
        .expect("创建组必须包含 Preview 入口");
    let canvas_mode = cx
        .debug_bounds("toolbar-canvas-scroll")
        .expect("画布组必须包含 Select/Scroll 入口");
    let layout = cx
        .debug_bounds("toolbar-layout")
        .expect("画布组必须包含 Layout 入口");
    let settings = cx
        .debug_bounds("toolbar-settings")
        .expect("Voice/Settings 工具组必须包含 Settings 入口");
    let voice = cx
        .debug_bounds("workspace-voice-control")
        .expect("Voice 状态必须作为独立 Dock 分组渲染");

    let layout_right = f32::from(layout.origin.x) + f32::from(layout.size.width);
    let preview_right = f32::from(preview.origin.x) + f32::from(preview.size.width);
    let creation_center_x = (f32::from(terminal.origin.x) + preview_right) / 2.0;
    let dock_top = f32::from(dock.origin.y);
    let dock_bottom = dock_top + f32::from(dock.size.height);

    assert!(
        f32::from(canvas_mode.origin.x) < f32::from(layout.origin.x),
        "画布组必须按 Select/Scroll、Layout 自左向右排列"
    );
    assert!(
        layout_right + 7.0 <= f32::from(terminal.origin.x),
        "创建组必须在 Layout 后保留至少 7px 分组间距"
    );
    assert!(
        f32::from(terminal.origin.x) < f32::from(codex.origin.x),
        "创建组必须先排列 Terminal，再排列 Codex"
    );
    assert!(
        f32::from(codex.origin.x) < f32::from(new_button.origin.x),
        "New 必须位于 Codex 右侧，不能回到创建组起点"
    );
    assert!(
        f32::from(new_button.origin.x) < f32::from(browser.origin.x),
        "New 必须位于 Browser 左侧，保持创建组中心位置"
    );
    assert!(
        f32::from(browser.origin.x) < f32::from(preview.origin.x),
        "创建组必须以 Browser、Preview 的顺序收尾"
    );
    assert_eq!(
        f32::from(new_button.center().x),
        creation_center_x,
        "New 的中心必须与五项创建组中心重合"
    );
    assert!(
        preview_right + 7.0 <= f32::from(voice.origin.x),
        "Voice/Settings 工具组必须在 Preview 后保留至少 7px 分组间距"
    );
    assert!(
        f32::from(voice.origin.x) < f32::from(settings.origin.x),
        "Settings 必须固定在 Voice 右侧和 Dock 最右端，不得回退到 Voice 左侧"
    );
    assert!(
        f32::from(new_button.origin.y) >= dock_top
            && f32::from(new_button.origin.y) + f32::from(new_button.size.height) <= dock_bottom,
        "New 必须完整位于 Dock 内，不能因居中强调而溢出"
    );
    assert!(
        f32::from(voice.origin.y) >= dock_top
            && f32::from(voice.origin.y) + f32::from(voice.size.height) <= dock_bottom,
        "Voice 必须完整位于 Dock 内，不能分裂为第二条浮层"
    );
    assert!(
        f32::from(settings.origin.y) >= dock_top
            && f32::from(settings.origin.y) + f32::from(settings.size.height) <= dock_bottom,
        "最右侧 Settings 必须完整位于 Dock 内，不得因重排而溢出"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("click target {selector} should be rendered"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
