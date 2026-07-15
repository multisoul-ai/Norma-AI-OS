use super::*;

/// Window inset 可见性：失焦的可见窗口继续动画，隐藏或最小化窗口停止动画。
///
/// 数据构造（含关键数值的推导过程）：
///   states = visible(1) × minimized(2) 的关键组合，共 3 个验收样本
///   allowed = visible && !minimized
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入可见且未最小化，模拟仍在屏幕上但可能失焦的窗口。
///   2. 输入不可见且未最小化，模拟被系统隐藏的窗口。
///   3. 输入可见但已最小化，模拟 Dock 中的窗口。
///
/// 预期结果：
///   - 正断言：失焦不参与判断，可见且未最小化时允许动画。
///   - 负断言：隐藏窗口不允许动画。
///   - 负断言：最小化窗口不允许动画。
#[test]
fn test_native_window_visibility_controls_voice_inset_animation() {
    assert!(
        native_window_allows_voice_inset(true, false),
        "可见且未最小化的窗口即使失焦，也必须继续显示 TTS 内缘光"
    );
    assert!(
        !native_window_allows_voice_inset(false, false),
        "系统隐藏的窗口不得继续计算或显示 TTS 内缘光"
    );
    assert!(
        !native_window_allows_voice_inset(true, true),
        "最小化到 Dock 的窗口不得继续计算或显示 TTS 内缘光"
    );
}
