use super::{Chord, ChordError};
use crate::shortcuts::{
    default_shortcut_entries, effective_shortcuts, validate_shortcut_change, ShortcutCommandId,
    ShortcutPreferences, ShortcutValidationError,
};

/// 场景：默认全局键 Ctrl+Alt+Cmd+Space 转为 Carbon 键码与修饰位。
///
/// 数据构造：GPUI 键位串 `ctrl-alt-cmd-space`，另取 `cmd-shift-k` 验证字母键。
/// 执行过程：调用 `Chord::parse`。
/// 预期结果：空格键码 49，修饰位 = controlKey 4096 + optionKey 2048 + cmdKey 256 = 6400；
/// ⌘⇧K 键码 40，修饰位 = 256 + shiftKey 512 = 768。
#[test]
fn chord_maps_gpui_keystrokes_to_carbon_codes() {
    assert_eq!(
        Chord::parse("ctrl-alt-cmd-space"),
        Ok(Chord {
            code: 49,
            modifiers: 6400
        }),
        "默认全局键应映射为空格 + ⌃⌥⌘"
    );
    assert_eq!(
        Chord::parse("cmd-shift-k"),
        Ok(Chord {
            code: 40,
            modifiers: 768
        }),
        "字母键应映射到 ANSI 键码"
    );
}

/// 场景：全局热键必须带修饰键、不能含 fn、且键位须有 Carbon 对应。
///
/// 数据构造：裸 F5、`fn-space`、无对应键码的 `cmd-é`。
/// 执行过程：逐个 `Chord::parse`。
/// 预期结果：分别得到 NoModifier、Unsupported、Unsupported，避免全局吞掉普通按键。
#[test]
fn chord_rejects_unsafe_global_shortcuts() {
    assert_eq!(
        Chord::parse("f5"),
        Err(ChordError::NoModifier),
        "裸键不得全局注册"
    );
    assert_eq!(
        Chord::parse("fn-space"),
        Err(ChordError::Unsupported),
        "Carbon 不支持 fn"
    );
    assert_eq!(
        Chord::parse("cmd-é"),
        Err(ChordError::Unsupported),
        "无键码的字符应拒绝"
    );
    assert_eq!(
        Chord::parse("???"),
        Err(ChordError::Unsupported),
        "非法串应拒绝"
    );
}

/// 场景：岛开合快捷键进入 Settings 注册表，且录制校验要求修饰键。
///
/// 数据构造：默认偏好的有效快捷键表。
/// 执行过程：检查默认项并校验裸 F5 与 ⌃⌥I。
/// 预期结果：默认可编辑且为 Ctrl+Alt+Cmd+Space；裸 F5 报 GlobalNeedsModifier；⌃⌥I 通过。
#[test]
fn toggle_island_shortcut_is_registered_and_requires_modifier() {
    let entry = default_shortcut_entries()
        .iter()
        .find(|entry| entry.command == ShortcutCommandId::ToggleIsland)
        .expect("灵动岛开合应出现在快捷键注册表");
    assert!(entry.editable, "灵动岛开合应可在 Settings 重录");
    assert_eq!(
        entry.default_keystroke,
        Some("ctrl-alt-cmd-space"),
        "默认键应为 ⌃⌥⌘Space"
    );
    let effective = effective_shortcuts(&ShortcutPreferences::default());
    assert_eq!(
        validate_shortcut_change(ShortcutCommandId::ToggleIsland, "f5", &effective),
        Err(ShortcutValidationError::GlobalNeedsModifier),
        "无修饰键的全局热键必须被拒绝"
    );
    assert_eq!(
        validate_shortcut_change(ShortcutCommandId::ToggleIsland, "ctrl-alt-i", &effective),
        Ok(()),
        "带修饰键的未占用组合应可保存"
    );
}
