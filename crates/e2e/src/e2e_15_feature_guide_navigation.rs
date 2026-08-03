//! E2E-15：功能导览侧栏保持紧凑、稳定且符合 macOS 导航层级。

use app::OpenFeatureGuide;
use gpui::TestAppContext;

use crate::harness::{echo_factory, make_app};

/// E2E-FEATURE-GUIDE-NAV-01：桌面宽度下三项导览使用紧凑且左对齐的固定侧栏。
///
/// 数据构造（含关键数值的推导过程）：
///   desktop sidebar width = 220px
///   topic rows            = 3
///   row height            = 44px（内容 20px + 上下留白 12px × 2 = 44px）
///   row gap               = 4px
///   navigation stack      = rows(44px × 3) + gaps(4px × 2) = 140px
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在默认桌面测试窗口通过公开 OpenFeatureGuide action 打开导览。
///   2. 从真实 GPUI 渲染树读取侧栏与三个主题点击区的 bounds。
///   3. 逐项比较侧栏宽度、行高、水平对齐和垂直间距。
///
/// 预期结果：
///   - 正断言：侧栏固定为 220px，三项点击区分别为 44px 高。
///   - 正断言：三项拥有相同的 x 坐标和宽度，并以 4px 间距从上到下排列。
///   - 负断言：任何主题行都不得恢复为截图中松散的 54px 大按钮。
#[test]
fn e2e_15_feature_guide_navigation_uses_compact_aligned_rows() {
    let mut cx = TestAppContext::single();
    let (_app, cx) = make_app(&mut cx, echo_factory());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();

    let navigation = cx
        .debug_bounds("feature-guide-navigation")
        .expect("导览打开后必须渲染可定位的 desktop navigation 侧栏");
    let first = cx
        .debug_bounds("feature-guide-topic-0")
        .expect("侧栏必须渲染第 1 个 Live Node 主题点击区");
    let second = cx
        .debug_bounds("feature-guide-topic-1")
        .expect("侧栏必须渲染第 2 个 Workspace Layouts 主题点击区");
    let third = cx
        .debug_bounds("feature-guide-topic-2")
        .expect("侧栏必须渲染第 3 个 Agent Notifications 主题点击区");

    assert_eq!(
        navigation.size.width,
        gpui::px(220.0),
        "桌面导览侧栏应固定为 220px，避免导航区域挤占正文"
    );
    assert_eq!(
        first.size.height,
        gpui::px(44.0),
        "第 1 个主题应使用 44px 紧凑点击区，不得恢复成 54px 大按钮"
    );
    assert_eq!(
        second.size.height,
        gpui::px(44.0),
        "第 2 个主题应使用 44px 紧凑点击区，不得恢复成 54px 大按钮"
    );
    assert_eq!(
        third.size.height,
        gpui::px(44.0),
        "第 3 个主题应使用 44px 紧凑点击区，不得恢复成 54px 大按钮"
    );
    assert_eq!(
        first.origin.x, second.origin.x,
        "第 1、2 个主题必须共享同一左边线，序号和标题才能稳定对齐"
    );
    assert_eq!(
        second.origin.x, third.origin.x,
        "第 2、3 个主题必须共享同一左边线，不能出现选中项横向跳动"
    );
    assert_eq!(
        first.size.width, second.size.width,
        "第 1、2 个主题必须具有相同点击宽度"
    );
    assert_eq!(
        second.size.width, third.size.width,
        "第 2、3 个主题必须具有相同点击宽度"
    );
    assert_eq!(
        second.origin.y - (first.origin.y + first.size.height),
        gpui::px(4.0),
        "第 1、2 个主题之间应保持 4px 原生侧栏间距"
    );
    assert_eq!(
        third.origin.y - (second.origin.y + second.size.height),
        gpui::px(4.0),
        "第 2、3 个主题之间应保持 4px 原生侧栏间距"
    );
    assert_ne!(
        first.size.height,
        gpui::px(54.0),
        "第 1 个主题不得继续使用旧版松散的 54px 行高"
    );
}
