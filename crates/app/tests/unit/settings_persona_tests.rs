use super::{load_profile, save_profile, PERSONA_SETTING_KEY};
use agent::{NormaPersona, PersonaTone};

/// 人设保存与重新加载走真实设置存储，不写入项目记忆。
///
/// 数据构造：隔离SQLite；专业风格(1)与中文补充说明(1)；初始没有人设配置。
/// 执行过程：加载默认→保存专业风格→重新创建SettingsStore→读取→尝试非法保存。
/// 预期结果：默认与保存值正确；非法保存失败且不会覆盖已保存的人设。
#[test]
fn persona_settings_survive_reload_and_invalid_save_preserves_previous_value() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths)
        .expect("应初始化SQLite")
        .settings_store();
    assert_eq!(
        load_profile(&store).expect("默认配置"),
        NormaPersona::default(),
        "首次应加载默认人设"
    );
    assert!(
        store
            .get_string(PERSONA_SETTING_KEY)
            .expect("读取配置")
            .is_none(),
        "读取默认值不得偷偷写入设置"
    );
    let profile = NormaPersona {
        tone: PersonaTone::Professional,
        instructions: "请用中文交流。".into(),
    };
    save_profile(&store, &profile).expect("合法配置应保存");
    let reopened = storage::SettingsStore::new(paths.app_db_path().to_path_buf());
    assert_eq!(
        load_profile(&reopened).expect("重新加载"),
        profile,
        "重启后应保留风格与说明"
    );
    let invalid = NormaPersona {
        instructions: "字".repeat(agent::MAX_PERSONA_INSTRUCTION_CHARS + 1),
        ..Default::default()
    };
    assert!(
        save_profile(&reopened, &invalid).is_err(),
        "超长说明应明确失败"
    );
    assert_eq!(
        load_profile(&reopened).expect("失败后重读"),
        profile,
        "非法输入不得覆盖原配置"
    );
    let memories = storage::MemoryStore::open(&paths).expect("应读取记忆库");
    assert!(
        memories
            .snapshot(&agent::MemoryScope::Personal)
            .expect("个人范围")
            .entries()
            .next()
            .is_none(),
        "人设不应混入个人记忆主题"
    );
}

/// 三个 Norma 导航直接切页，切页保留人设草稿，关闭会返回草稿并阻止丢失。
///
/// 数据构造：1280×800px 桌面窗口与隔离数据库；面板高度 = 800×0.7 = 560px；
///   人设默认暖心，草稿改为专业；300px 滚动覆盖表单底部超出视口的约 100px。
/// 执行过程：打开默认人设→点击记忆→返回人设→滚动表单→修改→点击记忆→尝试关闭→保存人设→关闭。
/// 预期结果：默认人设可见且记忆未渲染；表单可滚动而保存行始终可见；
///   页内旧标签不存在；导航直达；保存前无法关闭，保存后允许关闭。
#[test]
fn persona_navigation_preserves_draft_and_close_requires_resolution() {
    use crate::{app::MultisoulApp, settings::panel::SettingsSection};
    use gpui::{px, size, AppContext, Modifiers, TestAppContext};
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    let host = host.expect("应有宿主");
    host.update(cx, |app, _| app.settings_panel.open());
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("norma-persona").is_some(),
        "默认必须直接展示排在最前的人设页面"
    );
    assert!(
        cx.debug_bounds("norma-memory").is_none(),
        "默认人设页不应同时渲染记忆内容"
    );
    assert!(
        cx.debug_bounds("norma-tab-false").is_none(),
        "不应继续保留页内记忆标签"
    );
    let memory_destination = cx
        .debug_bounds("settings-section-norma-memory")
        .expect("应显示独立记忆子菜单");
    cx.simulate_click(memory_destination.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("norma-memory").is_some(),
        "点击 Memory 后应直接展示记忆内容"
    );
    let destination = cx
        .debug_bounds("settings-section-norma-persona")
        .expect("应显示独立人设子菜单");
    cx.simulate_click(destination.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        host.read_with(cx, |app, _| app.settings_panel.active_section),
        SettingsSection::NormaPersona,
        "人设子菜单必须直接切页"
    );
    assert!(
        cx.debug_bounds("norma-persona").is_some(),
        "人设内容必须实际渲染"
    );
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("应有设置视口");
    let actions = cx.debug_bounds("persona-actions").expect("应有保存操作");
    assert!(
        actions.bottom() <= viewport.bottom(),
        "桌面基准高度应直接显示保存操作，不能默认藏在屏幕下方：actions={actions:?}, viewport={viewport:?}"
    );
    let body = cx
        .debug_bounds("persona-content-scroll")
        .expect("缩小后的面板必须提供独立表单滚动区域");
    let example_before = cx.debug_bounds("persona-example").expect("应有示例");
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: body.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-300.0))),
        modifiers: Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let example_after = cx.debug_bounds("persona-example").expect("滚动后仍有示例");
    let actions_after = cx
        .debug_bounds("persona-actions")
        .expect("滚动后仍有保存行");
    assert!(
        example_after.top() < example_before.top(),
        "滚轮必须移动表单内容，不能仅裁掉超出的内容"
    );
    assert_eq!(actions_after, actions, "滚动表单不得移动底部保存行");
    assert!(
        cx.debug_bounds("persona-unsaved-prompt").is_none(),
        "没有尝试关闭时不应显示草稿警告"
    );
    let persona = host.read_with(cx, |app, _| {
        app.norma_persona.as_ref().expect("人设实体").clone()
    });
    persona.update(cx, |view, cx| {
        view.tone = PersonaTone::Professional;
        cx.notify();
    });
    let destination = cx
        .debug_bounds("settings-section-norma-memory")
        .expect("应显示独立记忆子菜单");
    cx.simulate_click(destination.center(), Modifiers::none());
    host.update(cx, |app, cx| app.close_settings_panel(cx));
    assert!(
        host.read_with(cx, |app, _| app.settings_panel.open),
        "有草稿时不得关闭设置"
    );
    assert_eq!(
        host.read_with(cx, |app, _| app.settings_panel.active_section),
        SettingsSection::NormaPersona,
        "关闭保护必须带回未保存的人设"
    );
    assert_eq!(
        persona.read_with(cx, |view, _| view.tone),
        PersonaTone::Professional,
        "切页不能重置人设草稿"
    );
    assert_ne!(
        persona.read_with(cx, |view, _| view.saved.tone),
        PersonaTone::Professional,
        "未点击保存时不应提前修改持久化配置"
    );
    assert!(
        persona.read_with(cx, |view, _| view.close_prompt),
        "必须给出明确的草稿操作提示"
    );
    cx.update(|window, cx| window.draw(cx).clear());
    let prompt = cx
        .debug_bounds("persona-unsaved-prompt")
        .expect("应渲染草稿关闭提示");
    assert!(
        prompt.top() >= viewport.top() && prompt.bottom() <= viewport.bottom(),
        "关闭被阻止时提示必须立即可见，不能藏在页面末尾"
    );
    persona.update(cx, |view, cx| view.save(cx));
    cx.run_until_parked();
    assert_eq!(
        persona.read_with(cx, |view, _| view.saved.tone),
        PersonaTone::Professional,
        "保存回执后应更新基线"
    );
    assert!(
        !persona.read_with(cx, |view, _| view.close_prompt),
        "保存成功后应解除关闭保护"
    );
    host.update(cx, |app, cx| app.close_settings_panel(cx));
    assert!(
        !host.read_with(cx, |app, _| app.settings_panel.open),
        "草稿已保存后应可以关闭设置"
    );
}
