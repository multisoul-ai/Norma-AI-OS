//! gpui-component 集成冒烟：确认 init 与 Button 构造在钉死的 gpui 0.2.2 下可用。

use gpui::TestAppContext;
use gpui_component::button::Button;

use crate::init_components;

/// 场景：在 headless TestAppContext 中初始化组件库并构造带 label 的 Button。
///
/// 数据构造：空 TestAppContext；Button id `"smoke"`，label `"OK"`。
/// 执行过程：`init_components` → 再次 `init_components`（幂等）→ `Button::new(...).label(...)`。
/// 预期结果：
///   - 正断言：两次 init 不 panic；Button 可构造
///   - 负断言：init 标记全局已存在（第二次走早退，不会二次 `set_global` panic）
#[test]
fn init_components_is_idempotent_and_button_constructs() {
    let cx = TestAppContext::single();
    cx.update(|cx| {
        init_components(cx);
        assert!(
            cx.has_global::<crate::component::Theme>(),
            "首次 init 后应安装 gpui-component Theme 全局状态"
        );
        init_components(cx);
        let _button = Button::new("smoke").label("OK");
    });
}
