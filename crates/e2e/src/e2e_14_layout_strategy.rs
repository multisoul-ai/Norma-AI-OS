//! 底部布局策略菜单：真实点击控件并验证新卡片只追加到配置格子，不重写既有卡片。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{create_terminal, echo_factory, enter_canvas, make_app, read_runtime};

/// E2E-LAYOUT-00：布局菜单首次打开时应默认匹配“按列向右、每列两张、适应可视区域”。
///
/// 数据构造（含关键数值的推导过程）：
///   默认填充方向 = ColumnFirst（按列向右）
///   默认每列卡片数 = 2（截图中减号与加号之间显示 2）
///   默认尺寸策略 = FitViewport（适应可视区域）
///   用户操作次数 = 1 次点击 `toolbar-layout`，不点击任何布局选项
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建全新 App 并进入 Canvas，读取布局菜单打开前的策略快照。
///   2. 点击 `toolbar-layout` 打开布局菜单，不修改任何选项。
///   3. 再次读取策略快照，确认打开菜单不会改写默认值。
///
/// 预期结果：
///   - 正断言：菜单打开前后均为 column_first / 2 / fit_viewport。
///   - 正断言：点击工具栏后布局菜单保持打开。
///   - 负断言：默认方向不得是 row_first，默认尺寸不得是 fixed。
#[test]
fn e2e_layout_menu_defaults_to_reference_layout() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("默认布局 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let before_open = app.update(cx, |app, _| app.canvas_layout_e2e_snapshot());
    assert!(
        !before_open.menu_open,
        "首次进入 Canvas 时布局菜单不应自行展开"
    );
    assert_eq!(
        before_open.fill, "column_first",
        "全新 App 的默认布局必须按列向右"
    );
    assert_eq!(
        before_open.track_count, 2,
        "全新 App 的默认布局必须精确为每列两张"
    );
    assert_eq!(
        before_open.sizing, "fit_viewport",
        "全新 App 的默认卡片尺寸必须适应可视区域"
    );
    assert_ne!(
        before_open.fill, "row_first",
        "全新 App 不得继续使用旧的按行向下默认值"
    );
    assert_ne!(
        before_open.sizing, "fixed",
        "全新 App 不得默认使用固定卡片尺寸"
    );

    click_debug(cx, "toolbar-layout");
    let after_open = app.update(cx, |app, _| app.canvas_layout_e2e_snapshot());
    assert!(
        after_open.menu_open,
        "点击布局工具栏按钮后布局菜单必须保持打开"
    );
    assert_eq!(
        after_open.fill, before_open.fill,
        "只打开布局菜单不应改写默认填充方向"
    );
    assert_eq!(
        after_open.track_count, before_open.track_count,
        "只打开布局菜单不应改写默认每列卡片数"
    );
    assert_eq!(
        after_open.sizing, before_open.sizing,
        "只打开布局菜单不应改写默认尺寸策略"
    );
}

/// E2E-LAYOUT-01：用户选择“按列向右、每列两张、固定尺寸”后，第三张卡片应向右新增，前两张不得跳动或缩放。
///
/// 数据构造（含关键数值的推导过程）：
///   默认数量 = 2；点击加号 1 次再点击减号 1 次 → track_count = 2 + 1 - 1 = 2
///   填充方式 = ColumnFirst，因此 index=0 → row=0,col=0；index=1 → row=1,col=0；index=2 → row=0,col=1
///   固定尺寸宽度 = 360 screen px；gap = 8 screen px，因此第三张相对第一张 x 增量 = 360 + 8 = 368 px
///   创建次数 = 3；在创建第 3 张前记录前两张的 bounds，作为“不跳动”正反验收基线
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，点击底部首个 `toolbar-layout` 打开布局菜单。
///   2. 点击按列向右、数量加号、数量减号、固定尺寸；读取 App e2e snapshot。
///   3. 创建第 1、2 张 Terminal 并记录它们的 bounds。
///   4. 创建第 3 张 Terminal；读取三张卡片的最终 bounds。
///   5. 点击“重新整理当前卡片”，确认操作入口生效并关闭菜单。
///
/// 预期结果：
///   - 断言 A：菜单显示三项配置控件，且 snapshot 为 column_first / 2 / fixed。
///   - 断言 B：第 1、2 张在新增第 3 张后 bounds 完全不变。
///   - 断言 C：第 2 张位于第 1 张下方；第 3 张位于第 1 张右方，且三张尺寸一致。
///   - 断言 D：第 3 张不应落在第 1 张下方的第三行；重排后菜单不应继续展开。
#[test]
fn e2e_layout_menu_column_first_appends_without_moving_existing_cards() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("布局 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-layout");
    assert!(
        cx.debug_bounds("layout-menu").is_some(),
        "点击底部首个布局按钮后必须渲染布局设置菜单"
    );
    assert!(
        cx.debug_bounds("layout-menu-fill-column").is_some(),
        "布局菜单必须提供按列向右填充方式"
    );
    assert!(
        cx.debug_bounds("layout-menu-size-fixed").is_some(),
        "布局菜单必须提供固定尺寸选项"
    );
    assert!(
        cx.debug_bounds("layout-menu-reflow").is_some(),
        "布局菜单必须提供重新整理当前卡片按钮"
    );

    click_debug(cx, "layout-menu-fill-column");
    click_debug(cx, "layout-menu-track-increase");
    click_debug(cx, "layout-menu-track-decrease");
    click_debug(cx, "layout-menu-size-fixed");
    let configured = app.update(cx, |app, _| app.canvas_layout_e2e_snapshot());
    assert!(
        configured.menu_open,
        "选择布局选项后菜单应保持展开，方便连续调整配置"
    );
    assert_eq!(
        configured.fill, "column_first",
        "点击按列向右后策略必须切换为 column_first"
    );
    assert_eq!(
        configured.track_count, 2,
        "默认 2 先加一再减一后每列卡片数量必须精确回到 2"
    );
    assert_eq!(
        configured.sizing, "fixed",
        "点击固定尺寸后策略必须记录 fixed，而不能保留 fit_viewport"
    );
    assert_ne!(
        configured.fill, "row_first",
        "按列向右被选中后策略不得继续报告 row_first"
    );

    let first = create_terminal(&app, cx);
    let second = create_terminal(&app, cx);
    let before_third = read_runtime(&app, cx, |runtime| {
        (
            runtime
                .document
                .node(first)
                .expect("第 1 张卡片应存在")
                .bounds,
            runtime
                .document
                .node(second)
                .expect("第 2 张卡片应存在")
                .bounds,
        )
    });
    let third = create_terminal(&app, cx);
    let after_third = read_runtime(&app, cx, |runtime| {
        (
            runtime
                .document
                .node(first)
                .expect("第 1 张卡片应存在")
                .bounds,
            runtime
                .document
                .node(second)
                .expect("第 2 张卡片应存在")
                .bounds,
            runtime
                .document
                .node(third)
                .expect("第 3 张卡片应存在")
                .bounds,
        )
    });

    assert_eq!(
        after_third.0, before_third.0,
        "新增第 3 张后第 1 张 bounds 必须保持不变，不能缩放或跳动"
    );
    assert_eq!(
        after_third.1, before_third.1,
        "新增第 3 张后第 2 张 bounds 必须保持不变，不能缩放或跳动"
    );
    assert_eq!(
        after_third.1.origin.x, after_third.0.origin.x,
        "按列向右时第 2 张应与第 1 张同列，x 坐标必须相同"
    );
    assert!(
        after_third.1.origin.y > after_third.0.origin.y,
        "按列向右时第 2 张必须位于第 1 张下方的第二行"
    );
    assert!(
        after_third.2.origin.x > after_third.0.origin.x,
        "按列向右且每列两张时第 3 张必须向右进入第二列"
    );
    assert_eq!(
        after_third.2.origin.y, after_third.0.origin.y,
        "第 3 张应回到第一行，y 坐标必须与第 1 张相同"
    );
    assert_eq!(
        after_third.0.size, after_third.1.size,
        "固定尺寸下第 1、2 张卡片的尺寸必须相同"
    );
    assert_eq!(
        after_third.0.size, after_third.2.size,
        "固定尺寸下第 3 张必须与已有卡片保持同一尺寸"
    );
    assert_ne!(
        after_third.2.origin.x, after_third.0.origin.x,
        "第 3 张不应落在第一列，否则没有按两行上限向右累加"
    );
    assert_ne!(
        after_third.2.origin.y, after_third.1.origin.y,
        "第 3 张不应落到第三行方向，否则 ColumnFirst 公式错误"
    );

    click_debug(cx, "layout-menu-reflow");
    let after_reflow = app.update(cx, |app, _| app.canvas_layout_e2e_snapshot());
    assert!(
        !after_reflow.menu_open,
        "点击重新整理当前卡片后布局菜单应关闭，明确反馈该动作已提交"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("布局 e2e 点击前必须存在 debug selector {selector:?}"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}
