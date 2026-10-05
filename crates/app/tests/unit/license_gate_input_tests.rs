/// License Key 交互：激活壳必须提供可聚焦、可键盘编辑的真实输入框。
///
/// 数据构造（含关键数值的推导过程）：
///   input entity count = 1（激活壳只需一个 License Key 单行输入）
///   old static preview = key_preview（只能显示粘贴结果，无键盘输入能力）
///   removed copy      = "未激活无法进入工作台。开发可设 NORMA_SKIP_LICENSE=1。"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. GateRoot 创建 InputState entity → 获得 GPUI 原生焦点、IME 和键盘编辑
///   2. render 挂载 Input::new → 用户可点击并直接输入 Key
///   3. 用户通过系统 Cmd+V 粘贴 → InputState 原生编辑链路接收内容
///   4. 激活时读取 value → 提交用户当前可见内容
///
/// 预期结果：
///   - 正断言：GateRoot 持有 Entity<InputState> 并渲染 Input
///   - 正断言：激活读取通过输入状态
///   - 负断言：旧 key_preview 静态伪输入框不存在
///   - 负断言：页面不显示重复的“粘贴 Key”按钮
///   - 负断言：界面不再显示 NORMA_SKIP_LICENSE 开发提示
#[test]
fn test_license_gate_uses_editable_input_without_redundant_paste_or_skip_copy() {
    let source = include_str!("../../src/license_gate/gate_root.rs");

    assert!(
        source.contains("Entity<InputState>"),
        "GateRoot 必须持有一个真实的 gpui-component InputState entity"
    );
    assert!(
        source.contains("Input::new(&self.license_input)"),
        "激活壳必须渲染可聚焦、可键盘编辑的 Input 控件"
    );
    assert!(
        source.contains("self.license_input.read(cx).value()"),
        "激活请求必须读取输入框当前值，不得读取脱节的镜像字符串"
    );
    assert!(
        !source.contains("key_preview"),
        "License Key 不得继续使用无法键盘输入的 key_preview 静态伪输入框"
    );
    assert!(
        source.contains("license.enter_a_license_key"),
        "空值校验应提示用户输入 License Key，不应假设只能粘贴"
    );
    assert!(
        !source.contains("请先粘贴 License Key"),
        "支持键盘输入后不得继续显示“请先粘贴”的误导提示"
    );
    assert!(
        !source.contains("\"粘贴 Key\""),
        "InputState 已原生支持 Cmd+V，License 页面不得再显示重复的“粘贴 Key”按钮"
    );
    assert!(
        !source.contains("未激活无法进入工作台。开发可设 NORMA_SKIP_LICENSE=1。"),
        "License 页面必须移除面向开发者的 NORMA_SKIP_LICENSE 提示"
    );
}
