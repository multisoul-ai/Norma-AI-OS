use super::state::*;
use storage::DispatchEffectPreference as Pref;

fn pref(style: &str) -> Pref {
    Pref {
        style: style.into(),
        voice: None,
        idea: None,
        handoff: None,
    }
}

fn ws(id: &str) -> EffectsTarget {
    EffectsTarget::Workspace {
        workspace_id: id.into(),
    }
}

fn project(w: &str, a: &str) -> EffectsTarget {
    EffectsTarget::Project {
        workspace_id: w.into(),
        area_id: a.into(),
    }
}

/// 项目覆盖优先于工作空间；没有项目覆盖时跟随工作空间；两者都没有时没有设置。
///
/// 数据构造：工作空间 w1=ribbon；项目 (w1,a1)=fireworks；项目 (w1,a2) 无覆盖；工作空间 w2 无设置。
/// 预期结果：
///   - 正断言：(w1,a1)→fireworks，(w1,a2)→ribbon，仅工作空间 w1→ribbon。
///   - 负断言：w2 与未知项目返回 None；不会把 a1 的覆盖泄漏到 a2。
#[test]
fn effective_prefers_project_then_workspace() {
    let mut state = EffectsSettingsState::default();
    state.apply(&ws("w1"), Some(pref("ribbon")));
    state.apply(&project("w1", "a1"), Some(pref("fireworks")));
    assert_eq!(
        state
            .effective(Some("w1"), Some("a1"))
            .map(|p| p.style.as_str()),
        Some("fireworks")
    );
    assert_eq!(
        state
            .effective(Some("w1"), Some("a2"))
            .map(|p| p.style.as_str()),
        Some("ribbon")
    );
    assert_eq!(
        state.effective(Some("w1"), None).map(|p| p.style.as_str()),
        Some("ribbon")
    );
    assert!(
        state.effective(Some("w2"), Some("a1")).is_none(),
        "其他工作空间不受影响"
    );
    assert!(
        state.effective(None, None).is_none(),
        "没有上下文时没有设置"
    );
}

/// 选中的款式 id：工作空间默认流星；项目未覆盖时显示工作空间的款式。
///
/// 数据构造：空状态；再设置工作空间=vortex。
/// 预期结果：
///   - 正断言：空状态工作空间显示 meteor；设置后显示 vortex；跟随中的项目显示 vortex。
///   - 负断言：项目一旦有覆盖，显示覆盖值而不是工作空间值。
#[test]
fn selected_style_defaults_to_meteor_and_follows() {
    let mut state = EffectsSettingsState::default();
    assert_eq!(state.selected_style(&ws("w1")), "meteor");
    state.apply(&ws("w1"), Some(pref("vortex")));
    assert_eq!(state.selected_style(&ws("w1")), "vortex");
    assert_eq!(
        state.selected_style(&project("w1", "a1")),
        "vortex",
        "跟随中的项目显示工作空间款式"
    );
    state.apply(&project("w1", "a1"), Some(pref("plane")));
    assert_eq!(state.selected_style(&project("w1", "a1")), "plane");
    assert!(state.follows_workspace(&project("w1", "a2")));
    assert!(!state.follows_workspace(&project("w1", "a1")));
}

/// 换款式时保留来源覆盖；第一次保存不再把交接固定为旧的离子，交接跟随当前款式（与语音一致）。
///
/// 数据构造：无设置 → 选 ribbon；已有 {fireworks, voice=plane, handoff=lightning} → 选 ribbon。
/// 预期结果：
///   - 正断言：首次保存得到 {ribbon, handoff=None}；已有设置只改 style，voice/handoff 原样保留。
///   - 负断言：首次保存的语音与想法来源仍为 None（跟随）。
#[test]
fn picking_a_style_preserves_source_overrides() {
    let state = EffectsSettingsState::default();
    let first = state.picked(&ws("w1"), "ribbon");
    assert_eq!(
        first,
        Pref {
            style: "ribbon".into(),
            voice: None,
            idea: None,
            handoff: None
        }
    );

    let mut state = EffectsSettingsState::default();
    state.apply(
        &ws("w1"),
        Some(Pref {
            style: "fireworks".into(),
            voice: Some("plane".into()),
            idea: None,
            handoff: Some("lightning".into()),
        }),
    );
    let next = state.picked(&ws("w1"), "ribbon");
    assert_eq!(
        (
            next.style.as_str(),
            next.voice.as_deref(),
            next.handoff.as_deref()
        ),
        ("ribbon", Some("plane"), Some("lightning"))
    );
}

/// 单个来源的覆盖：设置、清除（跟随），不动其他字段。
///
/// 数据构造：工作空间 {ribbon}；语音设为 fireworks，再清除。
/// 预期结果：
///   - 正断言：设置后 voice=Some(fireworks)；清除后 voice=None。
///   - 负断言：style 与其他来源始终不变。
#[test]
fn source_override_can_be_set_and_cleared() {
    let mut state = EffectsSettingsState::default();
    state.apply(&ws("w1"), Some(pref("ribbon")));
    let with = state.with_source(&ws("w1"), Source::Voice, Some("fireworks"));
    assert_eq!(
        (
            with.style.as_str(),
            with.voice.as_deref(),
            with.idea.clone()
        ),
        ("ribbon", Some("fireworks"), None)
    );
    state.apply(&ws("w1"), Some(with));
    let cleared = state.with_source(&ws("w1"), Source::Voice, None);
    assert_eq!(
        (cleared.voice.clone(), cleared.style.as_str()),
        (None, "ribbon")
    );
}

/// 项目“跟随工作空间”：开启=清除覆盖；关闭=复制当前工作空间设置作为起点。
///
/// 数据构造：工作空间 {ribbon, handoff=ion}；项目无覆盖。
/// 预期结果：
///   - 正断言：关闭跟随得到与工作空间相同的设置；对已有覆盖的项目开启跟随得到 None。
///   - 负断言：工作空间无设置时关闭跟随得到默认 {meteor, handoff=None}，不是空值。
#[test]
fn follow_toggle_copies_or_clears() {
    let mut state = EffectsSettingsState::default();
    assert_eq!(
        state.unfollowed(&project("w1", "a1")),
        Pref {
            style: "meteor".into(),
            voice: None,
            idea: None,
            handoff: None
        }
    );
    let base = Pref {
        style: "ribbon".into(),
        voice: None,
        idea: None,
        handoff: Some("ion".into()),
    };
    state.apply(&ws("w1"), Some(base.clone()));
    assert_eq!(state.unfollowed(&project("w1", "a1")), base);
    state.apply(&project("w1", "a1"), Some(pref("plane")));
    assert!(!state.follows_workspace(&project("w1", "a1")));
    state.apply(&project("w1", "a1"), None);
    assert!(state.follows_workspace(&project("w1", "a1")));
}

/// 播放选项的键值往返，未知值回落默认。
///
/// 数据构造：3 个速度键 + 2 个未知键；倍率期望 1.33 / 1.0 / 0.75。
/// 预期结果：
///   - 正断言：键往返相等；倍率与原型一致。
///   - 负断言：未知键回落标准速度。
#[test]
fn playback_speed_keys_round_trip() {
    for speed in [
        PlaybackSpeed::Relaxed,
        PlaybackSpeed::Standard,
        PlaybackSpeed::Brisk,
    ] {
        assert_eq!(PlaybackSpeed::from_key(Some(speed.key())), speed);
    }
    assert_eq!(
        [
            PlaybackSpeed::Relaxed.multiplier(),
            PlaybackSpeed::Standard.multiplier(),
            PlaybackSpeed::Brisk.multiplier()
        ],
        [1.33, 1.0, 0.75]
    );
    assert_eq!(
        PlaybackSpeed::from_key(Some("warp")),
        PlaybackSpeed::Standard
    );
    assert_eq!(PlaybackSpeed::from_key(None), PlaybackSpeed::Standard);
}

/// 解析成 motion 的选择：款式、来源覆盖与随机都按偏好生效。
///
/// 数据构造：工作空间 {ribbon, handoff=ion}；项目 (w1,a1)={random}。
/// 预期结果：
///   - 正断言：w1 的语音是声纹、交接是离子；a1 的语音与交接都是随机（相同 task_ref 得到同一款）。
///   - 负断言：没有任何设置时仍是默认（语音流星、交接离子）。
#[test]
fn selection_resolves_from_effective_preference() {
    use crate::motion::{DispatchEffectStyle as S, DispatchVisualOrigin as O};
    let mut state = EffectsSettingsState::default();
    state.apply(
        &ws("w1"),
        Some(Pref {
            style: "ribbon".into(),
            voice: None,
            idea: None,
            handoff: Some("ion".into()),
        }),
    );
    state.apply(&project("w1", "a1"), Some(pref("random")));
    let w = state.selection_for(Some("w1"), None);
    assert_eq!(
        (
            w.style_for(O::CommandDock, "t"),
            w.style_for(O::AgentNode(1), "t")
        ),
        (S::Ribbon, S::Ion)
    );
    let a = state.selection_for(Some("w1"), Some("a1"));
    assert_eq!(a.style_for(O::CommandDock, "x"), S::random_for("x"));
    assert_eq!(a.style_for(O::AgentNode(1), "x"), S::random_for("x"));
    let none = state.selection_for(Some("other"), None);
    assert_eq!(
        (
            none.style_for(O::CommandDock, "t"),
            none.style_for(O::AgentNode(1), "t")
        ),
        (S::Meteor, S::Meteor)
    );
}

/// 特效文案里的占位符必须是 `%{name}` 语法，且在中英文里一致；用真实的格式化函数验证被替换。
///
/// 数据构造：3 个带占位符的键（saved、follow_current_now、random_now），参数 scope=工作区A / style=烟花。
/// 预期结果：
///   - 正断言：格式化结果包含参数值，不再含 `%{`。
///   - 负断言：没有任何 effects 文案残留单花括号 `{name}`（这种写法不会被替换）。
#[test]
fn effects_copy_substitutes_placeholders() {
    use ui::i18n::{format, text_for, Locale};
    let saved = format(
        "effects.saved",
        &[("scope", "工作区A".into()), ("style", "烟花".into())],
    );
    assert!(
        saved.contains("工作区A") && saved.contains("烟花") && !saved.contains("%{"),
        "{saved}"
    );
    let now = format("effects.follow_current_now", &[("style", "烟花".into())]);
    assert!(now.contains("烟花") && !now.contains('{'), "{now}");
    let random = format("effects.random_now", &[("style", "烟花".into())]);
    assert!(random.contains("烟花") && !random.contains('{'), "{random}");
    for locale in [Locale::ZhCn, Locale::En] {
        for key in [
            "effects.title",
            "effects.subtitle",
            "effects.library_hint",
            "effects.sources_hint",
            "effects.line_fireworks",
        ] {
            let copy = text_for(locale, key);
            assert!(!copy.contains('{'), "{key} 含未替换的花括号：{copy}");
        }
    }
}
