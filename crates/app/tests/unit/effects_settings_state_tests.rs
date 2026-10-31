use super::state::*;
use storage::DispatchEffectPreference as Pref;

/// 场景：全局选择转换成真实派发样式，按来源覆盖仍有效。
/// 数据构造：默认值，再设 ribbon + voice=fireworks + handoff=ion。
/// 执行过程：读取默认选择，再通过全局偏好生成 motion 选择并解析三个来源。
/// 预期结果：默认流星；之后语音烟花、想法声纹、交接离子；修改基础款式不丢来源覆盖。
#[test]
fn global_effect_preserves_source_overrides() {
    use crate::motion::{DispatchEffectStyle as S, DispatchVisualOrigin as O};
    let mut state = EffectsSettingsState::default();
    assert_eq!(state.selected_style(), "meteor", "无配置时使用流星");
    state.global = Some(Pref {
        style: "ribbon".into(),
        voice: Some("fireworks".into()),
        idea: None,
        handoff: Some("ion".into()),
    });
    let selection = state.selection();
    assert_eq!(
        selection.style_for(O::CommandDock, "t"),
        S::Fireworks,
        "语音使用全局来源覆盖"
    );
    assert_eq!(
        selection.style_for(O::AgentNode(1), "t"),
        S::Ion,
        "交接使用全局来源覆盖"
    );
    let picked = state.picked("plane");
    assert_eq!(picked.style, "plane", "基础款式改变");
    assert_eq!(picked.voice.as_deref(), Some("fireworks"), "保留语音覆盖");
    assert_eq!(picked.handoff.as_deref(), Some("ion"), "保留交接覆盖");
    assert_eq!(picked.idea, None, "未设置的来源仍跟随");
    let cleared = state.with_source(Source::Voice, None);
    assert_eq!(cleared.voice, None, "清除语音覆盖");
    assert_eq!(cleared.style, "ribbon", "清除来源不影响基础款式");
}

/// 场景：全局特效在没有工作空间的首页也可设置、撤销和重开恢复。
/// 数据构造：空隔离数据库、一个真实 App 状态对象，没有任何 workspace。
/// 执行过程：点烟花，等待后台保存，核对全局键；撤销，再重建 App 读取设置。
/// 预期结果：保存不依赖空间上下文，撤销删除全局键，重开使用默认且不会恢复旧款式。
#[test]
fn global_effect_can_be_saved_and_undone_without_workspace() {
    use crate::app::MultisoulApp;
    use gpui::{AppContext, TestAppContext};
    let dir = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).expect("初始化");
    let settings = storage::SettingsStore::new(paths.app_db_path().to_owned());
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone())
    });
    let second =
        cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    cx.update(|window, cx| {
        crate::workspace::desktop::sessions::register(
            settings.path().to_owned(),
            window.window_handle(),
            app.clone(),
            cx,
        );
        crate::workspace::desktop::sessions::register(
            settings.path().to_owned(),
            window.window_handle(),
            second.clone(),
            cx,
        );
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| app.pick_effect_style("fireworks", cx));
    cx.run_until_parked();
    let saved: Pref = serde_json::from_str(
        &settings
            .get_string(KEY_DISPATCH)
            .expect("读取")
            .expect("全局特效存在"),
    )
    .expect("JSON");
    assert_eq!(saved.style, "fireworks", "首页应能保存全局特效");
    second.update(cx, |app, _| {
        use crate::motion::{DispatchEffectStyle, DispatchReceiptVisualStatus};
        app.motion
            .spawn_dispatch_receipt("global-test", 1, DispatchReceiptVisualStatus::Submitted);
        let effects = app.motion.dispatch_receipt_presentations();
        assert_eq!(
            effects.first().map(|effect| effect.style),
            Some(DispatchEffectStyle::Fireworks),
            "另一个窗口实际派发也必须用新特效"
        );
    });
    assert!(
        store
            .desktop_workspace_store()
            .list()
            .expect("目录")
            .is_empty(),
        "设置特效不能偷偷创建空间"
    );
    app.update(cx, |app, cx| app.undo_effect_change(cx));
    cx.run_until_parked();
    assert_eq!(
        settings.get_string(KEY_DISPATCH).expect("撤销"),
        None,
        "撤销恢复未设置"
    );
    cx.update(|_, cx| cx.set_global(EffectsSettingsState::default()));
    let _reopened = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    app.update(cx, |_, cx| {
        let state = crate::app::effects_actions::effects_snapshot(cx);
        assert_eq!(state.selected_style(), "meteor", "重开恢复默认");
        assert!(!state.busy, "保存或撤销不得留下 busy");
    });
}

/// 场景：播放速度序列化、未知值回退。
/// 数据构造：三档标准速度及未知键 warp；倍率来自既有 1.33/1/0.75 设计值。
/// 执行过程：键值往返，读取未知键。
/// 预期结果：三档不变，未知键回标准。
#[test]
fn playback_speed_keys_round_trip() {
    for speed in [
        PlaybackSpeed::Relaxed,
        PlaybackSpeed::Standard,
        PlaybackSpeed::Brisk,
    ] {
        assert_eq!(
            PlaybackSpeed::from_key(Some(speed.key())),
            speed,
            "速度应往返"
        );
    }
    assert_eq!(
        [
            PlaybackSpeed::Relaxed.multiplier(),
            PlaybackSpeed::Standard.multiplier(),
            PlaybackSpeed::Brisk.multiplier()
        ],
        [1.33, 1.0, 0.75],
        "倍率不变"
    );
    assert_eq!(
        PlaybackSpeed::from_key(Some("warp")),
        PlaybackSpeed::Standard,
        "未知速度回标准"
    );
    assert_eq!(
        PlaybackSpeed::from_key(None),
        PlaybackSpeed::Standard,
        "未设置速度回标准"
    );
}

/// 场景：启动读取尚未返回时改速度，不得丢掉已经保存的全局派发特效。
/// 数据构造：数据库有声纹和语音烟花；创建 App 后、运行任务前改为轻快。
/// 执行过程：启动加载和速度保存交错，随后等后台完成并读取当前状态。
/// 预期结果：保留特效及来源覆盖，速度为用户刚选的轻快。
#[test]
fn global_effect_load_is_independent_of_early_speed_edit() {
    use crate::app::MultisoulApp;
    use gpui::TestAppContext;
    let dir = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).expect("初始化");
    let settings = storage::SettingsStore::new(paths.app_db_path().to_owned());
    settings
        .set_string(KEY_DISPATCH, r#"{"style":"ribbon","voice":"fireworks"}"#)
        .expect("旧配置");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update(cx, |app, cx| app.set_effect_speed(PlaybackSpeed::Brisk, cx));
    cx.run_until_parked();
    app.update(cx, |_, cx| {
        let state = crate::app::effects_actions::effects_snapshot(cx);
        assert_eq!(
            state.selected_style(),
            "ribbon",
            "改速度不能挡住派发特效加载"
        );
        assert_eq!(
            state.global.as_ref().and_then(|pref| pref.voice.as_deref()),
            Some("fireworks"),
            "来源覆盖不能丢失"
        );
        assert_eq!(
            state.speed,
            PlaybackSpeed::Brisk,
            "旧速度不能覆盖刚选的新速度"
        );
    });
}
