//! Local TTS 模型库的 Settings 用户可观察合约。

use crate::harness::{echo_factory, enter_canvas, make_app, make_app_with_settings_store};
use app::MultisoulApp;
use gpui::TestAppContext;
use std::sync::Mutex;

mod support;
use support::*;

// 每次用户操作最多派生 4 层任务（点击→后台 worker→前台状态→持久化）;
// 8 次 run_until_parked 留出两轮余量，且不推进时间或依赖真实网络。
const E2E_SETTLE_PASSES: usize = 8;
// Voice Settings 最多进行三次滚动校正；每次至少推进一个可见目标高度，三次覆盖
// 上方、下方与布局重排后的补偿。24px 使目标不贴住裁剪边缘。
const SETTINGS_SCROLL_ATTEMPTS: usize = 3;
const SETTINGS_SCROLL_MARGIN_PX: f32 = 24.0;
static TTS_FAKE_TRANSPORT_LOCK: Mutex<()> = Mutex::new(());

/// E2E-TTS-1：Voice Settings 必须展示官方 TTS 下载、选择、删除与试听入口。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entries = 3（Matcha zh-en + Kokoro + ZipVoice-Distill INT8）
///   initial selected = 0（新用户没有自动下载模型）
///   first entry actions = 1（Install）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. headless 打开 Canvas 与 Voice Settings，从 Ready 摘要点击 TTS Change 进入 Spoken Replies；
///   2. 读取 TTS 目录投影并查询第一个模型的安装按钮；
///   3. 保持未安装状态，不发起真实网络下载。
///
/// 预期结果：
///   - 正断言：目录精确暴露三个官方模型与 Install 控件；
///   - 负断言：新用户不得已有选择或自动下载。
#[test]
fn e2e_tts_catalog_exposes_explicit_install_without_auto_selection() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建 e2e workspace");
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(workspace.path().join("models"), cx)
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(&app, cx, "settings-section-voice");
    let snapshot = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        snapshot.active_section, "Voice",
        "点击 Voice 导航后必须显示 Voice 内容，snapshot: {:?}",
        snapshot
    );
    click(&app, cx, "voice-ready-change-tts");
    settle(cx);
    assert_eq!(
        snapshot.tts_models.len(),
        3,
        "TTS 目录应包含两个固定音色模型和一个实验性克隆模型"
    );
    assert_eq!(
        snapshot.selected_tts_model_id, None,
        "新用户不应自动选择或下载 TTS 模型"
    );
    assert_eq!(
        snapshot.tts_models[0].id, "matcha-icefall-zh-en",
        "第一个推荐目录项必须是不可变 ID 的 Matcha zh-en，而不是旧 Baker 模型"
    );
    assert_ne!(
        snapshot.tts_models[0].id, "matcha-icefall-zh-baker",
        "Voice Settings 不得继续向用户暴露旧 Matcha Baker 模型"
    );
    assert!(
        cx.debug_bounds("settings-tts-install-0").is_some(),
        "第一个未安装模型应提供 Install 按钮"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-1").is_some(),
        "第二个精选模型应出现在 Voice Settings"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-2").is_some(),
        "第三个实验性 ZipVoice 模型应出现在 Voice Settings"
    );
    assert_eq!(
        snapshot.tts_models[0].voices,
        vec!["default".to_string()],
        "单 Voice 的 Matcha 条目必须公开默认 Voice，而不能隐藏为模型文件细节"
    );
    assert_eq!(
        snapshot.tts_models[1].voices,
        vec!["af_heart".to_string(), "af_bella".to_string()],
        "多 Voice 的 Kokoro 条目必须公开两个可选择的目录 Voice"
    );
    assert_eq!(
        snapshot.tts_models[0].action, "install",
        "未安装模型必须只提供显式 Install，而不能误显示 Select、Delete 或 Cancel"
    );
    assert!(
        !snapshot.tts_models[0].deletable,
        "未安装模型不得暴露删除动作，因为没有可回收的受控目录"
    );
}

/// E2E-TTS-3：ZipVoice 必须在参考声音导入成功后才允许选择，并在移除后立即失效。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog index      = 2（Matcha=0、Kokoro=1、ZipVoice=2）
///   reference rate    = 16,000 Hz
///   reference seconds = 3 s
///   reference samples = 16,000 × 3 × 1 = 48,000 mono samples
///   reference bytes   = 48,000 × 2 = 96,000 PCM bytes
///   preview PCM queues = before(0) + successful preview(1) = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先安装并选中 Matcha，再用确定性假运输安装缺少参考声音的 ZipVoice；
///   2. 确认 ZipVoice 不抢占 Matcha，且 Settings 显示路径、转写和导入入口；
///   3. 输入合法路径与准确转写，确认输入期不创建副本，再按 Enter 导入；
///   4. 试听并选择 ZipVoice，随后点击 Replace，确认 TTS 配置代际推进；
///   5. 点击 Remove，确认受控副本消失且当前 ZipVoice 选择被清除。
///
/// 预期结果：
///   - 正断言：Enter、Import、Preview、Select 与 Replace 都走同一受控参考配置；
///   - 负断言：输入期不得读盘，缺少参考不得替换 Matcha，移除后不得残留配置。
#[test]
fn e2e_zipvoice_reference_gates_selection_and_remove_invalidates_it() {
    let _transport_guard = lock_tts_fake_transport();
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应创建隔离 ZipVoice e2e 数据目录");
    let reference_path = temp.path().join("speaker.wav");
    write_pcm_wav(&reference_path, 16_000, 1, 3);
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store);
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(temp.path().join("models"), cx);
        app.set_fake_tts_transport_for_e2e(true, false, cx);
    });
    enter_canvas(&app, cx, temp.path());
    cx.simulate_keystrokes("cmd-,");
    click(&app, cx, "settings-section-voice");
    click(&app, cx, "voice-ready-change-tts");
    settle(cx);

    click(&app, cx, "settings-tts-install-0");
    settle(cx);
    let matcha = snapshot(&app, cx);
    assert_eq!(
        matcha.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "前置安装必须先建立一个可观察的 Matcha 当前选择"
    );

    click(&app, cx, "settings-tts-install-2");
    settle(cx);
    let missing = snapshot(&app, cx);
    assert_eq!(
        missing.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "ZipVoice 安装完成但缺少参考声音时不得抢占当前 Matcha 模型"
    );
    assert_eq!(
        missing.selected_tts_voice_id.as_deref(),
        Some("default"),
        "ZipVoice 缺少参考声音时不得改写当前 Matcha Voice"
    );
    assert!(
        cx.debug_bounds("settings-zipvoice-reference-path")
            .is_some(),
        "Ready ZipVoice 卡片必须显示可编辑本地路径输入"
    );
    assert!(
        cx.debug_bounds("settings-zipvoice-reference-transcript")
            .is_some(),
        "Ready ZipVoice 卡片必须显示准确转写输入"
    );
    assert!(
        cx.debug_bounds("settings-zipvoice-reference-browse")
            .is_some(),
        "Ready ZipVoice 卡片必须显示原生文件选择入口"
    );
    assert!(
        cx.debug_bounds("settings-tts-select-2").is_none(),
        "缺少参考声音时 ZipVoice 不得显示可用 Select 控件"
    );

    let controlled_root = temp
        .path()
        .join("models")
        .join("tts")
        .join("voice-references")
        .join("zipvoice-distill-int8-zh-en-emilia");
    click(&app, cx, "settings-zipvoice-reference-path");
    cx.simulate_input(reference_path.to_string_lossy().as_ref());
    click(&app, cx, "settings-zipvoice-reference-transcript");
    cx.simulate_input("这段文字与参考声音完全一致。");
    let drafted = snapshot(&app, cx);
    assert_eq!(
        drafted.zipvoice_reference_draft_path,
        reference_path.to_string_lossy(),
        "路径 InputState 必须把平台文本输入同步到内存草稿，且此时尚未读取文件"
    );
    assert_eq!(
        drafted.zipvoice_reference_draft_transcript, "这段文字与参考声音完全一致。",
        "转写 InputState 必须把 Unicode 平台文本输入完整同步到内存草稿"
    );
    assert!(
        !controlled_root.exists(),
        "普通路径/转写 Change 期间不得读取源 WAV 或创建受控参考目录"
    );
    cx.simulate_keystrokes("enter");
    settle(cx);
    let imported = snapshot(&app, cx);
    assert!(
        imported.zipvoice_reference_ready,
        "合法参考声音后台导入完成后 snapshot 必须标记为 ready，snapshot: {imported:?}"
    );
    assert!(
        cx.debug_bounds("settings-tts-select-2").is_some(),
        "参考声音 ready 后 ZipVoice 必须显示 Select 控件"
    );
    assert!(
        cx.debug_bounds("settings-tts-preview-2-0").is_some(),
        "参考声音 ready 后 ZipVoice 必须显示克隆声音 Preview 控件"
    );
    let preview_queues_before = app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());
    click(&app, cx, "settings-tts-preview-2-0");
    settle(cx);
    let previewed = snapshot(&app, cx);
    let preview_queues_after = app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());
    assert_eq!(
        preview_queues_after,
        preview_queues_before + 1,
        "ZipVoice Preview 必须成功合成并向播放状态排队一段 PCM"
    );
    assert_eq!(
        previewed.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "试听 ZipVoice 不得在用户点击 Select 前改写当前 Matcha 选择"
    );

    click(&app, cx, "settings-tts-select-2");
    settle(cx);
    let selected = snapshot(&app, cx);
    assert_eq!(
        selected.selected_tts_model_id.as_deref(),
        Some("zipvoice-distill-int8-zh-en-emilia"),
        "导入参考声音后必须能显式选择 ZipVoice"
    );
    assert_eq!(
        selected.selected_tts_voice_id.as_deref(),
        Some("cloned_reference"),
        "ZipVoice 选择必须使用参考声音驱动的稳定 Voice ID"
    );

    let selected_generation = selected.tts_configuration_generation;
    click(&app, cx, "settings-zipvoice-reference-transcript");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("替换后的准确参考转写。");
    click(&app, cx, "settings-zipvoice-reference-import");
    settle(cx);
    let replaced = snapshot(&app, cx);
    assert_eq!(
        replaced.selected_tts_model_id.as_deref(),
        Some("zipvoice-distill-int8-zh-en-emilia"),
        "替换当前 ZipVoice 参考声音后必须保留模型选择"
    );
    assert_ne!(
        replaced.tts_configuration_generation, selected_generation,
        "Replace 必须推进 TTS 配置代际，使缓存的旧参考声音客户端失效"
    );

    click(&app, cx, "settings-zipvoice-reference-remove");
    settle(cx);
    let removed = snapshot(&app, cx);
    assert!(
        !removed.zipvoice_reference_ready,
        "移除后 snapshot 不得继续报告参考声音 ready"
    );
    assert_eq!(
        removed.selected_tts_model_id, None,
        "移除当前 ZipVoice 的参考声音后必须清除无效模型选择"
    );
    assert!(
        !controlled_root.join("reference.wav").exists(),
        "移除后受控参考 WAV 不得残留"
    );
    assert!(
        !controlled_root.join("reference.txt").exists(),
        "移除后准确转写不得残留"
    );
}

/// E2E-TTS-2：Voice Settings 的假模型运输必须覆盖安装、取消、选择、Voice 持久化、
/// 删除与隔离试听，而不依赖 GitHub、真实 ONNX 或声卡。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog models         = 3（本用例操作 Matcha/Kokoro，ZipVoice 保持未安装）
///   first install          = 1（无选择时自动选择 Matcha zh-en/default）
///   second install         = 1（已有选择时保持未选择）
///   Kokoro voices          = 2（af_heart 推荐 + af_bella）
///   persisted settings IDs = 2（模型 ID + Voice ID）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过 Ready 摘要的 TTS Change 进入 Spoken Replies，启用确定性假下载/假运行时校验，真实点击第一、第二个 Install；
///   2. 删除并重新安装未选中模型，再选择 Kokoro，验证默认 Voice 回退；
///   3. 试听 Bella 后切换到 Bella，检查试听代际失效、选择持久化及选中模型删除保护；
///   4. 删除未选中的 Matcha，注入进行中的假下载并点击 Cancel，确认没有 Ready 残留。
///
/// 预期结果：
///   - 断言 A：首装自动选中 default，后装不抢占已有选择；
///   - 断言 B：切换模型回退推荐 af_heart，切换 Voice 后持久化 af_bella；
///   - 断言 C：试听不改写选择或 Dialogue，配置切换会使试听失效；
///   - 断言 D：选中模型无 Delete，取消下载后条目回到 Install 而非 Ready。
#[test]
fn e2e_tts_model_lifecycle_persists_voice_and_isolates_preview() {
    let _transport_guard = lock_tts_fake_transport();
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应创建隔离 TTS e2e 数据目录");
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(temp.path().join("models"), cx)
    });
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, false, cx)
    });
    enter_canvas(&app, cx, temp.path());
    cx.simulate_keystrokes("cmd-,");
    click(&app, cx, "settings-section-voice");
    click(&app, cx, "voice-ready-change-tts");
    settle(cx);

    click(&app, cx, "settings-tts-install-0");
    settle(cx);
    let first = snapshot(&app, cx);
    assert_eq!(
        first.tts_models[0].action, "selected",
        "首次成功安装时 Matcha 必须自动成为已选模型"
    );
    assert_eq!(
        first.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "首次安装后必须保存 Matcha zh-en 的不可变模型 ID"
    );
    assert_eq!(
        first.selected_tts_voice_id.as_deref(),
        Some("default"),
        "单 Voice Matcha 首装后必须自动选择其 default Voice"
    );

    click(&app, cx, "settings-tts-install-1");
    settle(cx);
    let second = snapshot(&app, cx);
    assert_eq!(
        second.tts_models[0].action, "selected",
        "安装第二个模型不得抢占首个已选模型"
    );
    assert_eq!(
        second.tts_models[1].action, "select",
        "已有选择时第二个成功安装的模型必须保持可显式 Select"
    );
    assert!(
        second.tts_models[1].deletable,
        "未选中的 Ready Kokoro 模型必须可删除以回收磁盘空间"
    );

    click(&app, cx, "settings-tts-delete-1");
    settle(cx);
    let deleted = snapshot(&app, cx);
    assert_eq!(
        deleted.tts_models[1].action, "install",
        "删除未选中模型后条目必须回到 Install，而不是伪装成 Ready"
    );
    assert_eq!(
        deleted.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "删除未选中 Kokoro 不得清除当前 Matcha zh-en 选择"
    );

    click(&app, cx, "settings-tts-install-1");
    settle(cx);
    click(&app, cx, "settings-tts-select-1");
    settle(cx);
    let switched = snapshot(&app, cx);
    assert_eq!(
        switched.selected_tts_model_id.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "显式 Select 后必须切换到 Kokoro 模型"
    );
    assert_eq!(
        switched.selected_tts_voice_id.as_deref(),
        Some("af_heart"),
        "旧 Matcha default 在 Kokoro 中不可用时必须回退到推荐 af_heart"
    );
    assert!(
        !switched.tts_models[1].deletable,
        "已选中的模型必须在用户可见状态投影中禁用 Delete，避免产生无效选择"
    );

    click(&app, cx, "settings-tts-preview-1-1");
    settle(cx);
    let previewing = snapshot(&app, cx);
    assert_eq!(
        previewing.selected_tts_voice_id.as_deref(),
        Some("af_heart"),
        "试听 Bella 不得在用户确认前改写当前选择的 af_heart"
    );

    let preview_generation = previewing.tts_configuration_generation;
    click(&app, cx, "settings-tts-voice-1-1");
    settle(cx);
    let bella = snapshot(&app, cx);
    assert_eq!(
        bella.selected_tts_voice_id.as_deref(),
        Some("af_bella"),
        "点击 Use 后必须切换并保存用户指定的 Bella Voice"
    );
    assert!(
        !bella.tts_preview_active,
        "模型或 Voice 配置变更必须取消旧试听，禁止与后续回复混音"
    );
    assert_ne!(
        bella.tts_configuration_generation, preview_generation,
        "Voice 配置变更必须推进 generation，使旧异步试听结果失效"
    );
    let persisted = MultisoulApp::load_tts_selection_for_e2e(store.clone());
    assert_eq!(
        persisted.0.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "后台持久化后 SQLite 必须保留 Kokoro 模型 ID"
    );
    assert_eq!(
        persisted.1.as_deref(),
        Some("af_bella"),
        "后台持久化后 SQLite 必须保留 Bella Voice ID"
    );

    click(&app, cx, "settings-tts-delete-0");
    settle(cx);
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, true, cx)
    });
    click(&app, cx, "settings-tts-install-0");
    settle(cx);
    let cancelling = snapshot(&app, cx);
    assert_eq!(
        cancelling.tts_models[0].action, "cancel",
        "下载进行中必须公开 Cancel 控件，而不能显示 Ready 或 Select"
    );
    click(&app, cx, "settings-tts-cancel-0");
    settle(cx);
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(false, false, cx)
    });
    let cancelled = snapshot(&app, cx);
    assert_eq!(
        cancelled.tts_models[0].action, "install",
        "取消下载后模型必须回到 Install，不能遗留 Ready 清单或选择"
    );
    assert_eq!(
        cancelled.selected_tts_model_id.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "取消未选中模型的下载不得改变当前 Kokoro 选择"
    );
}
