//! 快捷键录制的规范写法，以及想法派发使用的智能体类型稳定名字（数字与 `⌘2…9` 的创建顺序一致）。

use norma::protocol::AgentProfile;

#[cfg(test)]
#[path = "../../tests/unit/idea_assign_shortcuts_tests.rs"]
mod tests;

/// 数字槽位对应的新建类型；`0` 与未绑定的槽位返回 `None`，由调用方使用默认类型。
pub(crate) fn new_agent_profile(slot: u8) -> Option<AgentProfile> {
    match slot {
        2 => Some(AgentProfile::Claude),
        3 => Some(AgentProfile::Codex),
        4 => Some(AgentProfile::Cursor),
        5 => Some(AgentProfile::OpenCode),
        6 => Some(AgentProfile::Grok),
        7 => Some(AgentProfile::Kimi),
        8 => Some(AgentProfile::Pi),
        9 => Some(AgentProfile::Hermes),
        _ => None,
    }
}

/// 把按键还原成规范写法再去匹配或保存。macOS 上 GPUI 对 shift+数字上报“上档字符且清掉 shift 位”
/// （US 布局 ⌃⇧3 是 `ctrl-#`），而默认键与偏好里存的是 `ctrl-shift-3`；只还原仅带 ⌃ 的情形。
/// 其他键盘布局的上档字符不同，那种布局下请在 Settings 里直接录制（录到什么就存什么）。
pub(crate) fn canonical_source(keystroke: &gpui::Keystroke) -> String {
    let modifiers = &keystroke.modifiers;
    let digit = (modifiers.control
        && !modifiers.shift
        && !modifiers.alt
        && !modifiers.platform
        && !modifiers.function)
        .then(|| ")!@#$%^&*(".find(keystroke.key.as_str()))
        .flatten()
        .filter(|_| keystroke.key.chars().count() == 1);
    match digit {
        Some(digit) => gpui::Keystroke {
            modifiers: gpui::Modifiers {
                shift: true,
                ..*modifiers
            },
            key: digit.to_string(),
            key_char: None,
        }
        .unparse(),
        None => keystroke.unparse(),
    }
}

/// 存储用的稳定名字（不依赖 `Debug` 输出，改名类型时不会悄悄改变存档）。
pub(crate) fn profile_key(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
        AgentProfile::InfCode => "infcode",
        AgentProfile::Kimi => "kimi",
        AgentProfile::ZCode => "zcode",
        AgentProfile::Pi => "pi",
        AgentProfile::Hermes => "hermes",
    }
}

pub(crate) fn profile_from_key(key: &str) -> Option<AgentProfile> {
    picker_profiles()
        .into_iter()
        .find(|profile| profile_key(*profile) == key)
}

/// `⌃⇧0` 类型选择器的列表：数字键 2…9 的类型在前（与 `⌘2…9` 一致），其后是没有数字键的类型。
pub(crate) fn picker_profiles() -> Vec<AgentProfile> {
    let mut all: Vec<AgentProfile> = (2..=9).filter_map(new_agent_profile).collect();
    all.extend([AgentProfile::ZCode, AgentProfile::InfCode]);
    all
}
