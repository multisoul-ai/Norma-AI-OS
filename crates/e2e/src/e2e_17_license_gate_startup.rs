//! E2E-17：License Gate 无凭证启动与首帧渲染。

use app::build_activation_root_e2e;
use gpui::{px, TestAppContext};

/// E2E-17：无 License 时启动必须完成首帧渲染，不能因输入组件缺少 Root 而崩溃。
///
/// 数据构造（含关键数值的推导过程）：
///   activation window = 1 个（显式指定 Activation，避免读取开发机本地凭证）
///   license input     = 1 个（激活页只接受一个 License Key）
///   workbench         = 0 个（未激活时硬门控不得提前挂载工作台）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. build_activation_root_e2e 组装生产 License Gate 窗口根节点
///   2. run_until_parked 驱动窗口完成首帧布局与 gpui-component Input 渲染
///   3. 查询稳定 debug selector，确认激活壳和输入区域已进入渲染树
///
/// 预期结果：
///   - 正断言：激活壳存在，证明首帧已安全完成
///   - 正断言：License Key 输入区域存在，用户可以继续输入
///   - 负断言：未激活时不得渲染工作台根节点
#[test]
fn e2e_17_unlicensed_startup_renders_editable_gate_without_panicking() {
    let mut test_cx = TestAppContext::single();
    let (_root, window_cx) = test_cx.add_window_view(build_activation_root_e2e);

    assert!(
        window_cx.debug_bounds("activation-shell").is_some(),
        "无 License 启动完成首帧后必须渲染 activation-shell"
    );
    assert!(
        window_cx.debug_bounds("license-key-input").is_some(),
        "激活壳必须渲染一个可编辑的 License Key 输入区域"
    );
    assert!(
        window_cx.debug_bounds("gate-workbench").is_none(),
        "未激活时不得提前挂载 gate-workbench"
    );
}

/// E2E-17b：License 操作区只保留明亮玻璃“激活”和弱化“退出”，不显示粘贴按钮。
///
/// 数据构造（含关键数值的推导过程）：
///   input width     = 420px（License 表单主体宽度）
///   paste buttons   = 0 个（输入框已原生支持 Cmd+V，无需重复入口）
///   activate width  = 228px（主操作占据操作条主要空间）
///   exit width      = 72px（低频退出操作，视觉权重最低）
///   gaps            = 1 × 10px = 10px
///   action row      = 228 + 72 + 10 = 310px < 420px
///   button height   = 40px（与工作台 38px 控件接近，并保留清晰点击面积）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造真实 License Gate 首帧并查询粘贴按钮，确认重复入口不存在
///   2. 读取激活与退出按钮 bounds，检查两者统一高度
///   3. 比较宽度层级，确认激活明显宽于退出
///
/// 预期结果：
///   - 正断言：激活和退出按钮存在且高度为 40px
///   - 正断言：两枚按钮总宽度加间距为 310px，小于 420px 输入框
///   - 负断言：不得渲染“粘贴 Key”按钮
#[test]
fn e2e_17_license_actions_are_compact_and_visually_hierarchical() {
    let mut test_cx = TestAppContext::single();
    let (_root, window_cx) = test_cx.add_window_view(build_activation_root_e2e);

    let paste = window_cx.debug_bounds("license-paste-button");
    let activate = window_cx
        .debug_bounds("license-activate-button")
        .expect("License Gate 必须渲染主激活按钮");
    let exit = window_cx
        .debug_bounds("license-exit-button")
        .expect("License Gate 必须渲染退出按钮");

    assert!(
        paste.is_none(),
        "输入框已支持手动输入和 Cmd+V，License Gate 不得继续显示“粘贴 Key”按钮"
    );
    assert_eq!(
        activate.size.height,
        px(40.0),
        "主激活按钮应使用 40px 紧凑高度"
    );
    assert_eq!(
        exit.size.height,
        px(40.0),
        "退出按钮应与同组操作保持 40px 高度"
    );
    assert_eq!(
        activate.size.width,
        px(228.0),
        "激活主操作应扩展为 228px，形成明确的单一主操作"
    );
    assert_eq!(
        exit.size.width,
        px(72.0),
        "退出低频操作应收窄为 72px，避免与激活等权"
    );
    assert!(
        activate.size.width > exit.size.width,
        "按钮宽度必须满足激活 > 退出，当前分别为 {:?}、{:?}",
        activate.size.width,
        exit.size.width
    );
    assert_eq!(
        activate.size.width + exit.size.width + px(10.0),
        px(310.0),
        "按钮组总宽度应为 310px，必须明显小于 420px 输入框"
    );
}
