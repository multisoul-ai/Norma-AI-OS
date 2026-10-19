use super::*;

/// AC12/AC13：GPT-Live拥有独立持久化模式，旧值和升级默认值保持兼容。
/// 数据构造：三种存储字符串、未知值和纯空白；不读真实用户设置。
/// 执行过程：解析后再序列化，并检查默认模式和未知值拒绝。
/// 预期结果：gpt_live独立往返，legacy/doubao不改变，升级默认仍Legacy。
#[test]
fn desktop_live_mode_roundtrips_without_changing_legacy_defaults() {
    for stored in ["legacy", "doubao", "gpt_live"] {
        let mode = DesktopVoiceMode::from_stored(stored).expect("已支持的模式必须可恢复");
        assert_eq!(mode.as_str(), stored, "每种模式应独立往返");
    }
    assert_eq!(
        DesktopVoiceMode::default(),
        DesktopVoiceMode::Legacy,
        "升级默认不变"
    );
    assert_eq!(
        DesktopVoiceMode::from_stored("realtime"),
        None,
        "不把其他协议当作Live模式"
    );
    assert_eq!(
        DesktopVoiceMode::from_stored(" "),
        None,
        "未知空白值不激活Live"
    );
    for stored in ["single_voice_command", "continuous_listening"] {
        assert_eq!(
            VoiceInteractionMode::from_stored(stored)
                .expect("既有交互模式")
                .as_str(),
            stored,
            "既有交互模式的存储值不变"
        );
    }
    for stored in ["hold", "toggle"] {
        assert_eq!(
            DictationTrigger::from_stored(stored)
                .expect("既有触发模式")
                .as_str(),
            stored,
            "既有快捷键触发方式不变"
        );
    }
}
