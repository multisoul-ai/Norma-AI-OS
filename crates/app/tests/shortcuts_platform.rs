//! Exercise the production App registry with native GPUI keystrokes.
#[path = "../src/shortcuts.rs"]
mod shortcuts;

use gpui::Keystroke;
use shortcuts::{ShortcutCommandId, ShortcutSettingsState};

/// 场景：原生桌面用户以平台主修饰键新建工作区。
/// 数据构造：无偏好覆盖的生产注册表；macOS使用Cmd+N，Windows使用Ctrl+N。
/// 执行过程：解析真实GPUI按键并经生产command_for_keystroke分派。
/// 预期结果：命中新建工作区，不被Windows系统键或其他App命令替代。
#[test]
fn native_workspace_shortcut_uses_platform_primary() {
    let source = if cfg!(target_os = "macos") {
        "cmd-n"
    } else {
        "ctrl-n"
    };
    let key = Keystroke::parse(source).expect("平台默认快捷键必须可解析");
    let state = ShortcutSettingsState::default();
    assert_eq!(
        state.command_for_keystroke(&key),
        Some(ShortcutCommandId::NewWorkspace),
        "平台主修饰键必须触发新建工作区"
    );
    assert_eq!(
        state.command_for_keystroke(&Keystroke::parse("n").expect("裸键合法")),
        None,
        "输入工作区名称时裸字母不能触发App命令"
    );
}
