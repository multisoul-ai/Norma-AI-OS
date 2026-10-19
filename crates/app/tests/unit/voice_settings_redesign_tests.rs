use crate::app::MultisoulApp;
use crate::settings::voice::DesktopVoiceMode;
use gpui::{px, size, AppContext, Modifiers, TestAppContext};

/// 语音页面以状态概览和完整宽度的任务卡片组织，模式切换仍写入真实设置存储。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离空数据库，语音能力未配置；窗口 1280×800px，为桌面基准。
///   任务卡片 = 识别(1) + AI连接(1) +声音(1) = 3；旧步进轴占宽 60px，应删除。
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Voice → 检查状态概览、三个任务卡片和旧向导元素。
///      浏览 Azure 配置 → 检查来源按钮与区域输入框不重叠，运行来源仍为本地。
///   2. 将麦克风和 ASR 标为就绪 → 点击下一步 → AI 连接输入框滚入视口。
///   3. 点击 Doubao → 后台保存 → 检查页面与数据库，再切回 Voice Commands。
///   4. 缩到 640×700px → 检查模式卡片仍位于内容视口中。
///
/// 预期结果：
///   - 正断言：概览可见；任务卡片对齐且等宽；豆包选择保存且可恢复。
///   - 负断言：没有步进轴、重复的向导标题或不可操作的 Finish Setup 按钮；模式不溢出。
#[test]
fn voice_redesign_task_layout_and_mode_persistence() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let settings = store.settings_store();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("voice-status-overview").is_some(),
        "未配置也应显示实际状态概览"
    );
    assert!(
        cx.debug_bounds("voice-status-next").is_some(),
        "未就绪概览应给出可操作的下一步入口"
    );
    assert!(
        cx.debug_bounds("voice-setup-stepper").is_none(),
        "任务设置不应继续显示安装向导步进轴"
    );
    assert!(
        cx.debug_bounds("voice-setup-title").is_none(),
        "Voice 页头下不得再重复大标题"
    );
    assert!(
        cx.debug_bounds("voice-setup-finish").is_none(),
        "未就绪时不能展示没有操作的完成按钮"
    );
    let recognition = cx
        .debug_bounds("voice-setup-requirement-asr")
        .expect("应保留识别卡片");
    let provider = cx
        .debug_bounds("voice-setup-requirement-provider")
        .expect("应保留连接卡片");
    let replies = cx
        .debug_bounds("voice-setup-requirement-tts")
        .expect("应保留声音卡片");
    assert_eq!(
        recognition.left(),
        provider.left(),
        "识别与连接应共用内容左边界"
    );
    assert_eq!(
        recognition.size.width, replies.size.width,
        "识别与声音卡片应共用完整内容宽度"
    );
    assert_eq!(
        recognition.size.width, provider.size.width,
        "识别与连接卡片应共用完整内容宽度"
    );
    let local_source = cx
        .debug_bounds("voice-setup-asr-source-local")
        .expect("应有本地来源入口");
    let azure_source = cx
        .debug_bounds("voice-setup-asr-source-azure")
        .expect("应有Azure来源入口");
    assert_eq!(
        local_source.top(),
        azure_source.top(),
        "桌面宽度下两个来源入口应并排，不应挤成重叠的换行"
    );
    assert!(
        azure_source.right() < recognition.right(),
        "来源入口不能超出卡片右边界"
    );
    cx.simulate_click(azure_source.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    let region = cx
        .debug_bounds("settings-voice-field-azure-region")
        .expect("应展示Azure区域输入框");
    assert!(
        region.top() >= azure_source.bottom(),
        "下一行表单不能覆盖识别来源按钮"
    );
    assert_eq!(
        host.read_with(cx, |app, _| app.voice_settings.values().asr_provider()),
        asr::AsrProviderId::LocalSherpa,
        "浏览Azure配置不能改变实际运行的本地识别来源"
    );
    host.update(cx, |app, cx| {
        let id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID).expect("ASR ID合法");
        app.voice_settings
            .set_microphone_requirement(crate::settings::voice::VoiceRequirementState::Ready);
        app.voice_settings.model_manager_mut().mark_ready(&id);
        app.voice_settings.select_asr_model(Some(id));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let next = cx
        .debug_bounds("voice-status-next")
        .expect("应给出连接AI的下一步入口");
    cx.simulate_click(next.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("应有视口");
    let provider_input = cx
        .debug_bounds("settings-voice-field-agent-base-url")
        .expect("应有连接地址输入框");
    assert!(
        provider_input.top() >= viewport.top() && provider_input.top() < viewport.bottom(),
        "下一步应直接定位到缺失的AI连接配置"
    );
    host.update(cx, |app, cx| {
        app.settings_viewport
            .content_scroll
            .set_offset(gpui::Point::default());
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let doubao = cx
        .debug_bounds("settings-voice-desktop-mode-doubao")
        .expect("应保留豆包模式入口");
    for (selector, button_selector) in [
        (
            "settings-voice-desktop-mode-legacy",
            "settings-voice-desktop-mode-legacy-button",
        ),
        (
            "settings-voice-desktop-mode-doubao",
            "settings-voice-desktop-mode-doubao-button",
        ),
        (
            "settings-voice-desktop-mode-gpt-live",
            "settings-voice-desktop-mode-gpt-live-button",
        ),
    ] {
        let card = cx.debug_bounds(selector).expect("模式卡片应存在");
        let button = cx.debug_bounds(button_selector).expect("模式按钮应存在");
        assert!(
            button.left() >= card.left() && button.right() <= card.right(),
            "{selector}按钮必须位于自身卡片内，不能覆盖相邻模式"
        );
    }
    assert!(
        doubao.top() >= viewport.top() && doubao.bottom() <= viewport.bottom(),
        "滚动重绘后豆包入口应完全位于可点击视口内"
    );
    cx.simulate_click(doubao.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        host.read_with(cx, |app, _| app.voice_settings.values().desktop_mode()),
        DesktopVoiceMode::Doubao,
        "点击必须切换真实运行模式"
    );
    assert_eq!(
        settings
            .get_string("voice.desktop.mode")
            .expect("读取模式")
            .as_deref(),
        Some("doubao"),
        "模式必须后台写入持久化存储"
    );
    assert!(
        cx.debug_bounds("settings-voice-doubao-readiness").is_some(),
        "豆包页应显示连接状态"
    );
    assert!(
        cx.debug_bounds("voice-setup-requirement-asr").is_none(),
        "豆包页不应显示旧链路识别配置"
    );
    assert!(
        cx.debug_bounds("voice-status-next").is_none(),
        "豆包模式不能显示跳转到传统链路配置的下一步按钮"
    );
    let legacy = cx
        .debug_bounds("settings-voice-desktop-mode-legacy")
        .expect("应可返回原链路");
    cx.simulate_click(legacy.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        settings
            .get_string("voice.desktop.mode")
            .expect("读取模式")
            .as_deref(),
        Some("legacy"),
        "返回原链路也必须保存"
    );
    cx.simulate_resize(size(px(640.0), px(700.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("应有内容视口");
    let mode = cx
        .debug_bounds("settings-voice-desktop-mode-doubao")
        .expect("窄窗口也应保留模式入口");
    assert!(
        mode.left() >= viewport.left() && mode.right() <= viewport.right(),
        "模式卡片不能溢出窄窗口内容视口"
    );
    let narrow_source = cx
        .debug_bounds("voice-setup-asr-source-azure")
        .expect("窄窗口应保留Azure入口");
    let narrow_card = cx
        .debug_bounds("voice-setup-requirement-asr")
        .expect("窄窗口应保留识别卡片");
    assert!(
        narrow_source.right() < narrow_card.right(),
        "识别来源控件不能溢出窄窗口卡片"
    );
}

/// 已就绪的语音仍可修改声音，取消草稿不会替换正在使用的音色，播报开关仍保存。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离数据库与 1280×800px 窗口；麦克风(1)+ASR(1)+AI(1)+TTS(1)=4 项就绪。
///   ASR = Zipformer Bilingual；TTS = Matcha Chinese-English 的 default 音色。
///   滚动距离由实际 max_offset.height 推导，不假设卡片高度。
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Voice 并建立四项就绪 → 从概览点击声音 Change。
///   2. 声音选择器滚入视口 → 点击 Cancel → 检查当前音色和草稿状态。
///   3. 点击任务播报开关 → 等待后台持久化 → 检查开关与数据库值。
///
/// 预期结果：
///   - 正断言：修改入口仍可用、声音配置滚入视口、开关更新并保存。
///   - 负断言：取消后没有选择器；原音色未被改写；旧手工开关不再承担交互。
#[test]
fn voice_redesign_ready_voice_change_cancel_and_preferences() {
    use crate::settings::voice::{VoiceEntryState, VoiceRequirementState};
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let settings = store.settings_store();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    let asr = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID).expect("ASR ID合法");
    let tts = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID).expect("TTS ID合法");
    let voice = tts::TtsVoiceId::parse("default").expect("音色ID合法");
    host.update(cx, |app, cx| {
        app.voice_settings
            .set_microphone_requirement(VoiceRequirementState::Ready);
        app.voice_settings
            .set_provider_requirement(VoiceRequirementState::Ready);
        app.voice_settings.model_manager_mut().mark_ready(&asr);
        app.voice_settings.select_asr_model(Some(asr));
        app.voice_settings.tts_model_manager_mut().mark_ready(&tts);
        app.voice_settings
            .select_tts_model_and_voice(Some(tts.clone()), Some(voice.clone()));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        host.read_with(cx, |app, _| app.voice_settings.readiness()),
        VoiceEntryState::Ready,
        "四项能力必须生成就绪页面"
    );
    assert!(
        cx.debug_bounds("voice-status-next").is_none(),
        "四项能力就绪后不能继续提示配置下一步"
    );
    assert!(
        cx.debug_bounds("settings-voice-mode-continuous").is_some(),
        "日常页面必须保留连续聆听模式"
    );
    let change = cx
        .debug_bounds("voice-ready-change-tts")
        .expect("应保留声音更改入口");
    cx.simulate_click(change.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("应有视口");
    let picker = cx
        .debug_bounds("settings-tts-picker")
        .expect("应打开声音选择器");
    assert!(
        picker.top() >= viewport.top() && picker.top() < viewport.bottom(),
        "声音入口应把配置滚入视口，不能要求用户从顶部查找"
    );
    let scroll = host.read_with(cx, |app, _| app.settings_viewport.content_scroll.clone());
    scroll.set_offset(gpui::point(px(0.0), -scroll.max_offset().height));
    cx.update(|window, cx| window.draw(cx).clear());
    let cancel = cx
        .debug_bounds("settings-tts-picker-cancel")
        .expect("应保留取消按钮");
    cx.simulate_click(cancel.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("settings-tts-picker").is_none(),
        "取消后必须关闭草稿选择器"
    );
    assert_eq!(
        host.read_with(cx, |app, _| app
            .voice_settings
            .values()
            .tts_model_id()
            .cloned()),
        Some(tts),
        "取消不能更改原TTS模型"
    );
    assert_eq!(
        host.read_with(cx, |app, _| app
            .voice_settings
            .values()
            .tts_voice_id()
            .cloned()),
        Some(voice),
        "取消不能更改原音色"
    );
    let before = host.read_with(cx, |app, _| {
        app.voice_settings
            .values()
            .agent_task_announcements_enabled()
    });
    let toggle = cx
        .debug_bounds("settings-agent-task-announcements-toggle")
        .expect("设置期间也应保留播报开关");
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    assert_ne!(
        host.read_with(cx, |app, _| app
            .voice_settings
            .values()
            .agent_task_announcements_enabled()),
        before,
        "组件开关必须更新实际设置"
    );
    assert_eq!(
        settings
            .get_string("voice.agent_task_announcements_enabled")
            .expect("读取播报设置")
            .as_deref(),
        Some(if before { "false" } else { "true" }),
        "播报开关必须后台保存"
    );
}

/// 设置操作通过 Enter、空格和鼠标触发同一回调，其他按键不触发操作。
///
/// 数据构造（含关键数值的推导过程）：
///   单按钮窗口，操作次数初始为 0；Enter(1)+空格(1)+鼠标(1)=3 次操作。
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立窗口初始焦点后用 Tab 遍历按钮；Escape 不触发操作。
///   2. Enter 与空格分别执行一次回调；鼠标点击仍执行同一回调。
///
/// 预期结果：
///   - 正断言：三种激活方式各增加一次计数。
///   - 负断言：Escape 不激活按钮，键盘事件不会重复执行回调。
#[test]
fn voice_redesign_actions_support_keyboard_and_mouse() {
    use crate::settings::voice::view::components::VoiceAction;
    use gpui::{div, prelude::*, Context, Render, Window};
    use std::{cell::Cell, rc::Rc};
    use ui::component::button::Button;

    struct ActionFixture(Rc<Cell<usize>>);
    impl Render for ActionFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let activations = self.0.clone();
            div().child(
                Button::new("voice-keyboard-action")
                    .debug_selector(|| "voice-keyboard-action".to_string())
                    .label("Activate")
                    .on_activate(move |_, _, _| activations.set(activations.get() + 1)),
            )
        }
    }
    let activations = Rc::new(Cell::new(0));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let (_, cx) = test.add_window_view(|window, cx| {
        let view = cx.new(|_| ActionFixture(activations.clone()));
        ui::component::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, _| window.focus_next());
    cx.simulate_keystrokes("tab escape");
    assert_eq!(activations.get(), 0, "Escape 不能执行设置操作");
    cx.simulate_keystrokes("enter");
    assert_eq!(activations.get(), 1, "Enter 应恰好激活一次设置操作");
    cx.simulate_keystrokes("space");
    assert_eq!(activations.get(), 2, "空格应激活一次，不得重复处理键盘事件");
    let action = cx
        .debug_bounds("voice-keyboard-action")
        .expect("应有设置按钮");
    cx.simulate_click(action.center(), Modifiers::none());
    assert_eq!(activations.get(), 3, "补充键盘行为后鼠标点击仍应正常");
}

/// 播报开关可通过键盘切换，状态和鼠标操作共用同一回调。
///
/// 数据构造（含关键数值的推导过程）：
///   初始关闭；空格(1)+Enter(1)+鼠标(1)=3 次切换，最终应开启。
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立窗口初始焦点后用 Tab 遍历开关；Escape 不执行回调。
///   2. 空格开启，重绘后 Enter 关闭，再重绘后鼠标开启。
///
/// 预期结果：
///   - 正断言：键盘与鼠标分别切换实际受控值，回调累计三次。
///   - 负断言：Escape 不切换，Enter 不重复执行且不保留过期值。
#[test]
fn voice_redesign_switch_supports_keyboard_and_mouse() {
    use crate::settings::voice::view::spoken_reply::toggle_row;
    use gpui::{prelude::*, Context, Render, Window};
    use std::{cell::Cell, rc::Rc};

    struct SwitchFixture(Rc<Cell<bool>>, Rc<Cell<usize>>);
    impl Render for SwitchFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let value = self.0.clone();
            let calls = self.1.clone();
            toggle_row(
                ui::Theme::dark(),
                "Announcement",
                "Toggle spoken replies",
                value.get(),
                "voice-keyboard-switch",
                move |next, _, cx| {
                    value.set(*next);
                    calls.set(calls.get() + 1);
                    cx.refresh_windows();
                },
            )
        }
    }
    let value = Rc::new(Cell::new(false));
    let calls = Rc::new(Cell::new(0));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let (_, cx) = test.add_window_view(|window, cx| {
        let view = cx.new(|_| SwitchFixture(value.clone(), calls.clone()));
        ui::component::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, _| window.focus_next());
    cx.simulate_keystrokes("tab escape");
    assert_eq!(calls.get(), 0, "Escape 不能切换播报开关");
    cx.simulate_keystrokes("space");
    assert!(value.get(), "空格应开启实际受控值");
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("enter");
    assert!(!value.get(), "重绘后 Enter 应关闭，不得使用上次的过期值");
    assert_eq!(calls.get(), 2, "两种键盘激活必须各执行一次");
    cx.update(|window, cx| window.draw(cx).clear());
    let toggle = cx
        .debug_bounds("voice-keyboard-switch")
        .expect("应保留开关");
    cx.simulate_click(toggle.center(), Modifiers::none());
    assert!(value.get(), "键盘操作后鼠标仍应开启开关");
    assert_eq!(calls.get(), 3, "鼠标操作不得重复执行开关回调");
}
