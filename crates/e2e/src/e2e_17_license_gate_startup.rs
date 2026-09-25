//! E2E-17：License Gate 无凭证启动与首帧渲染。

use app::{build_activation_root_e2e, build_configured_unlicensed_root_e2e};
use gpui::{px, Modifiers, TestAppContext};

const INTERNAL_BETA_LICENSE_KEY: &str = "KG-WACEHBCB-2ZL4AM23-JBJQJZBG-YVT55VDN";

/// E2E-17-default-off：默认关闭 licenses 时，无凭证启动也必须直接进入工作台。
///
/// 数据构造（含关键数值的推导过程）：
///   licenses feature = false（app 默认 features 不包含 `licenses`）
///   local status     = Invalid(Missing)（模拟首次安装且没有离线凭证）
///   activation roots = 0 个（默认关闭 License 后不显示激活壳）
///   workbench roots  = 1 个（无凭证也直接构造工作台）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. build_configured_unlicensed_root_e2e 使用当前构建开关和缺失凭证计算启动阶段
///   2. GateRoot 按启动阶段挂载工作台并完成首帧渲染
///   3. 查询稳定 debug selector，确认工作台存在且激活壳不存在
///
/// 预期结果：
///   - 正断言：gate-workbench 存在
///   - 负断言：activation-shell 与 license-key-input 均不存在
#[test]
fn e2e_17_default_disabled_license_opens_workbench_without_activation_shell() {
    let mut test_cx = TestAppContext::single();
    let (_root, window_cx) = test_cx.add_window_view(build_configured_unlicensed_root_e2e);

    assert!(
        window_cx.debug_bounds("gate-workbench").is_some(),
        "licenses 默认关闭时，本地凭证缺失也必须渲染 gate-workbench"
    );
    assert!(
        window_cx.debug_bounds("activation-shell").is_none(),
        "licenses 默认关闭时不得渲染 activation-shell"
    );
    assert!(
        window_cx.debug_bounds("license-key-input").is_none(),
        "licenses 默认关闭时不得渲染 License Key 输入框"
    );
}

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

/// E2E-17-prefill：内测激活页默认填入推广 License Key，并允许用户完整替换。
///
/// 数据构造（含关键数值的推导过程）：
///   activation window = 1 个（显式进入激活阶段，不读取开发机本地凭证）
///   license input     = 1 个（激活页唯一 License Key 输入框）
///   beta key          = `KG-WACEHBCB-2ZL4AM23-JBJQJZBG-YVT55VDN`（内测推广 Key）
///   replacement key   = `KG-PRIVATE-TEST-REPLACEMENT`（与推广 Key 不同的负对照）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造真实 License Gate 首帧，点击唯一 License Key 输入区域取得焦点
///   2. 发送 Cmd+A + Cmd+C，从系统剪贴板读取用户首屏可见的完整默认值
///   3. 再次全选并通过平台文本输入写入 replacement key，然后复制当前值
///   4. 对比两次剪贴板内容，确认默认值正确且 InputState 未被只读或锁死
///
/// 预期结果：
///   - 正断言：首次复制精确得到内测推广 Key
///   - 正断言：替换后精确得到 replacement key
///   - 负断言：替换后不得残留内测推广 Key
///   - 负断言：未点击激活时不得提前进入工作台
#[test]
fn e2e_17_license_input_prefills_beta_key_and_remains_editable() {
    let mut test_cx = TestAppContext::single();
    let (_root, window_cx) = test_cx.add_window_view(build_activation_root_e2e);
    let input = window_cx
        .debug_bounds("license-key-input")
        .expect("内测 License 默认值验收前必须渲染 License Key 输入框");

    window_cx.simulate_click(input.center(), Modifiers::none());
    window_cx.simulate_keystrokes("cmd-a cmd-c");
    let initial =
        window_cx.update(|_window, cx| cx.read_from_clipboard().and_then(|item| item.text()));

    assert_eq!(
        initial.as_deref(),
        Some(INTERNAL_BETA_LICENSE_KEY),
        "License 输入框首屏默认值必须精确等于内测推广 Key"
    );

    let replacement_key = "KG-PRIVATE-TEST-REPLACEMENT";
    window_cx.simulate_keystrokes("cmd-a");
    window_cx.simulate_input(replacement_key);
    window_cx.simulate_keystrokes("cmd-a cmd-c");
    let replaced =
        window_cx.update(|_window, cx| cx.read_from_clipboard().and_then(|item| item.text()));

    assert_eq!(
        replaced.as_deref(),
        Some(replacement_key),
        "用户必须能够完整替换内测推广 Key，InputState 不得被锁死"
    );
    assert_ne!(
        replaced.as_deref(),
        Some(INTERNAL_BETA_LICENSE_KEY),
        "替换后不得继续提交或复制旧的内测推广 Key"
    );
    assert!(
        window_cx.debug_bounds("gate-workbench").is_none(),
        "只编辑默认 License Key 且未点击激活时不得提前进入工作台"
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
