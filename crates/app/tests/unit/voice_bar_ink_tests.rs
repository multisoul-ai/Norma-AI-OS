use super::*;

/// 场景：语音视觉状态变化不能丢失浅色 Dock 的前景色。
/// 数据构造：六主题、Idle/Recording/Processing/Success/Error 五状态。
/// 执行过程：读取实际 voice_palette，逐状态检查正文和非空态反馈底色。
/// 预期结果：浅色用深字，深色保持白字，反馈底色保持可辨，不修改语音行为。
#[test]
fn voice_status_ink_follows_the_dock_surface() {
    for id in ui::ThemeId::ALL {
        let theme = Theme::for_id(id);
        for state in [
            VoiceVisual::Idle,
            VoiceVisual::Recording,
            VoiceVisual::Processing,
            VoiceVisual::Success,
            VoiceVisual::Error,
        ] {
            let palette = voice_palette(theme, state, None);
            let expected = if id.is_dark() {
                0xffffffff
            } else {
                theme.text_primary
            };
            assert_eq!(
                palette.text, expected,
                "{id:?} 语音文字应保持与表面相容的极性"
            );
            assert_ne!(palette.text, theme.dock, "{id:?} 语音文字不能与底色重合");
            if !matches!(state, VoiceVisual::Idle) {
                assert_eq!(palette.fill & 255, 0x12, "{id:?} 非空态保持反馈底色");
            }
        }
    }
}
