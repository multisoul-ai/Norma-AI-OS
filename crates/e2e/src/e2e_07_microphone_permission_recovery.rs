//! E2E-07：语音麦克风权限拒绝后的恢复入口。
//!
//! 本测试在 headless GPUI 中验证 Norma OS 自身的 L1 交互：TCC 拒绝后渲染恢复按钮。
//! 点击该按钮会唤起 macOS System Settings，属于真实操作系统的 L3 流程，不能在
//! TestAppContext 中自动修改；因此本用例验证按钮出现在真实渲染树中，L3 由人工验收。

use gpui::TestAppContext;

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-07：仅已拒绝的麦克风权限应在语音栏渲染“打开麦克风设置”恢复按钮。
///
/// 数据构造（含关键数值的推导过程）：
///   Canvas workspace = tempfile 空目录（无持久化快照，确保 Canvas 初始稳定）
///   denied error     = SpeechError::MicrophoneDenied（TCC 已保存拒绝，不能再显示系统弹窗）
///   recovery target  = `voice-open-microphone-settings`（语音栏中唯一的 macOS 恢复按钮）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. make_app + enter_canvas 进入真实 headless Canvas 渲染树，尚未写入语音错误。
///   2. 检查恢复按钮不存在，证明普通 Voice idle 不会显示无关的系统设置跳转。
///   3. 注入 MicrophoneDenied 并通知重渲染，模拟 Cmd+Shift+V 的权限失败落点。
///   4. 从 GPUI debug bounds 读取恢复按钮，证明用户可实际点击该渲染元素。
///
/// 预期结果：
///   - 断言 A：普通 Canvas 中恢复按钮不存在，避免所有语音状态都诱导跳转系统设置。
///   - 断言 B：MicrophoneDenied 后恢复按钮存在，用户可从错误状态前往修复权限。
///   - 断言 C：按钮 bounds 宽高均大于零，避免只注册 selector 而没有可点击表面。
#[test]
fn e2e_07_microphone_denied_renders_a_clickable_settings_recovery_button() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temp workspace should be creatable");
    enter_canvas(&app, cx, workspace.path());

    assert!(
        cx.debug_bounds("voice-open-microphone-settings").is_none(),
        "普通 Voice idle 不应渲染麦克风系统设置恢复按钮"
    );

    app.update(cx, |app, cx| {
        app.show_microphone_permission_denied_for_e2e(cx)
    });
    cx.run_until_parked();

    let button = cx
        .debug_bounds("voice-open-microphone-settings")
        .expect("MicrophoneDenied 后语音栏应渲染可点击的系统设置恢复按钮");
    assert!(
        button.size.width > gpui::px(0.0),
        "恢复按钮宽度必须为正，避免用户无法命中点击目标"
    );
    assert!(
        button.size.height > gpui::px(0.0),
        "恢复按钮高度必须为正，避免用户无法命中点击目标"
    );
}
