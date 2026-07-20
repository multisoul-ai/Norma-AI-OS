//! Local TTS 模型库的 Settings 用户可观察合约。

use crate::harness::{
    echo_factory, enter_canvas, make_app, make_app_with_settings_store, VoiceModelInstallE2eGuard,
};
use app::MultisoulApp;
use gpui::TestAppContext;

mod support;
use support::*;
mod catalog_disclosure;

/// E2E-TTS-3：ZipVoice 必须在参考声音导入成功后才允许选择，并在移除后立即失效。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog index = 2（Matcha=0、Kokoro=1、ZipVoice=2）
///   reference = 16,000 Hz × 3 s × mono = 48,000 samples × 2 = 96,000 PCM bytes
///   empty transcripts = 1；preview queues = before(0) + successful(1) = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装并激活 Matcha；再安装缺少参考声音的 ZipVoice，确认它不抢占当前声音；
///   2. 仅填路径时拒绝导入；补充准确转写后按 Enter 导入；
///   3. 试听并选择 ZipVoice，确认仍是 Draft，再用 Use This Voice 激活；
///   4. Replace 推进配置代际；Remove 清除受控副本和当前无效选择。
///
/// 预期结果：
///   - 正断言：Enter、Import、Preview、Select 与 Replace 都走同一受控参考配置；
///   - 负断言：空转写不得提交或读盘，缺少参考不得替换 Matcha，移除后不得残留配置。
#[test]
fn e2e_zipvoice_reference_gates_selection_and_remove_invalidates_it() {
    let _transport_guard = VoiceModelInstallE2eGuard::acquire();
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
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);

    click(&app, cx, "settings-tts-install-0");
    settle(cx);
    let matcha_draft = snapshot(&app, cx);
    assert_eq!(
        matcha_draft.selected_tts_model_id, None,
        "安装 Matcha 只应建立 Draft，不得在确认前激活本地模型"
    );
    assert_eq!(
        matcha_draft.tts_provider.picker_local_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "首次安装完成后必须把 Matcha 作为可确认的本地 Draft"
    );
    click(&app, cx, "settings-tts-picker-apply");
    settle(cx);
    let matcha = snapshot(&app, cx);
    assert_eq!(
        matcha.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "Use This Voice 后必须建立一个可观察的 Matcha 当前选择"
    );

    click(&app, cx, "settings-tts-change-voice");
    settle(cx);
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);

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
    let missing_transcript = snapshot(&app, cx);
    assert_eq!(
        missing_transcript.zipvoice_reference_draft_transcript, "",
        "仅填写 WAV 路径时准确转写草稿必须保持为空"
    );
    assert!(
        cx.debug_bounds("settings-zipvoice-reference-transcript-required")
            .is_some(),
        "路径已填但转写为空时必须就地显示 Required 提示"
    );
    assert!(
        cx.debug_bounds("settings-zipvoice-reference-import-disabled")
            .is_some(),
        "路径已填但转写为空时 Import 必须显示为禁用态"
    );
    click(&app, cx, "settings-zipvoice-reference-import-disabled");
    let rejected = snapshot(&app, cx);
    assert!(
        !rejected.zipvoice_reference_ready,
        "点击禁用态 Import 不得把空转写参考声音标记为 ready"
    );
    assert!(
        !controlled_root.exists(),
        "点击禁用态 Import 不得创建受控参考目录或读取源 WAV"
    );
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
    let drafted = snapshot(&app, cx);
    assert_eq!(
        drafted.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "选择 ZipVoice 只能更新 Draft，确认前不得替换当前 Matcha"
    );
    assert_eq!(
        drafted.tts_provider.picker_local_model_id.as_deref(),
        Some("zipvoice-distill-int8-zh-en-emilia"),
        "导入参考声音后必须能把 ZipVoice 设为本地 Draft"
    );
    assert_eq!(
        drafted.tts_provider.picker_local_voice_id.as_deref(),
        Some("cloned_reference"),
        "ZipVoice Draft 必须使用参考声音驱动的稳定 Voice ID"
    );
    click(&app, cx, "settings-tts-picker-apply");
    settle(cx);
    let selected = snapshot(&app, cx);
    assert_eq!(
        selected.selected_tts_model_id.as_deref(),
        Some("zipvoice-distill-int8-zh-en-emilia"),
        "Use This Voice 后必须激活 ZipVoice"
    );

    let selected_generation = selected.tts_configuration_generation;
    click(&app, cx, "settings-tts-change-voice");
    settle(cx);
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);
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
///   first install          = 1（无候选时建立 Matcha zh-en/default Draft）
///   second install         = 1（已有候选时保持未选择）
///   Kokoro voices          = 2（af_heart 推荐 + af_bella）
///   persisted settings IDs = 2（模型 ID + Voice ID）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过 TTS Change 与 Manage Models 进入本地目录，启用确定性假运输并安装 Matcha；
///   2. 确认安装只建立 Draft，再以 Use This Voice 激活 Matcha；
///   3. 重新打开目录，安装、删除并重装 Kokoro，再选择它，确认仍未替换当前 Matcha；
///   4. Apply Kokoro 后重新打开，试听并选择 Bella，确认 Draft/Apply 与持久化边界；
///   5. 再次打开目录，删除未选中的 Matcha，注入进行中的假下载并点击 Cancel。
///
/// 预期结果：
///   - 断言 A：首装只创建 default Draft，Apply 后才激活，后装不抢占已有候选；
///   - 断言 B：候选模型回退推荐 af_heart，候选 Voice Apply 后持久化 af_bella；
///   - 断言 C：试听不改写选择或 Dialogue，配置切换会使试听失效；
///   - 断言 D：选中模型无 Delete，取消下载后条目回到 Install 而非 Ready。
#[test]
fn e2e_tts_model_lifecycle_persists_voice_and_isolates_preview() {
    let _transport_guard = VoiceModelInstallE2eGuard::acquire();
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
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);

    click(&app, cx, "settings-tts-install-0");
    settle(cx);
    let first_draft = snapshot(&app, cx);
    assert_eq!(
        first_draft.tts_models[0].action, "selected",
        "首次成功安装时 Matcha 必须成为当前选择器中的 Draft"
    );
    assert_eq!(
        first_draft.selected_tts_model_id, None,
        "首次安装后不得在用户确认前保存 Matcha 当前选择"
    );
    assert_eq!(
        first_draft.tts_provider.picker_local_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "首次安装后必须建立 Matcha zh-en Draft"
    );
    assert_eq!(
        first_draft.tts_provider.picker_local_voice_id.as_deref(),
        Some("default"),
        "单 Voice Matcha 首装后必须使用 default Draft Voice"
    );
    click(&app, cx, "settings-tts-picker-apply");
    settle(cx);
    let first = snapshot(&app, cx);
    assert_eq!(
        first.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "Use This Voice 后必须保存 Matcha zh-en 的不可变模型 ID"
    );
    assert_eq!(
        first.selected_tts_voice_id.as_deref(),
        Some("default"),
        "Use This Voice 后必须激活 Matcha default Voice"
    );

    click(&app, cx, "settings-tts-change-voice");
    settle(cx);
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);

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
    let switched_draft = snapshot(&app, cx);
    assert_eq!(
        switched_draft.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "显式 Select 只应更新 Draft，确认前当前模型必须保持 Matcha"
    );
    assert_eq!(
        switched_draft.tts_provider.picker_local_model_id.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "显式 Select 后本地 Draft 必须切换到 Kokoro"
    );
    assert_eq!(
        switched_draft.tts_provider.picker_local_voice_id.as_deref(),
        Some("af_heart"),
        "旧 Matcha default 在 Kokoro Draft 中不可用时必须回退到推荐 af_heart"
    );
    assert!(
        !switched_draft.tts_models[1].deletable,
        "当前 Draft 模型必须在用户可见状态投影中禁用 Delete，避免产生无效候选"
    );
    click(&app, cx, "settings-tts-picker-apply");
    settle(cx);
    let switched = snapshot(&app, cx);
    assert_eq!(
        switched.selected_tts_model_id.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "Use This Voice 后必须激活 Kokoro 模型"
    );
    assert_eq!(
        switched.selected_tts_voice_id.as_deref(),
        Some("af_heart"),
        "Use This Voice 后必须激活推荐 af_heart"
    );

    click(&app, cx, "settings-tts-change-voice");
    settle(cx);
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);

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
    let bella_draft = snapshot(&app, cx);
    assert_eq!(
        bella_draft.selected_tts_voice_id.as_deref(),
        Some("af_heart"),
        "点击 Use 只应更新 Draft，确认前当前 Voice 必须保持 af_heart"
    );
    assert_eq!(
        bella_draft.tts_provider.picker_local_voice_id.as_deref(),
        Some("af_bella"),
        "点击 Use 后本地 Draft 必须切换到 Bella Voice"
    );
    assert!(
        !bella_draft.tts_preview_active,
        "Draft Voice 变更必须取消旧试听，禁止预览继续混音"
    );
    assert_eq!(
        bella_draft.tts_configuration_generation, preview_generation,
        "Draft Voice 变更不得推进当前 TTS generation"
    );
    click(&app, cx, "settings-tts-picker-apply");
    settle(cx);
    let bella = snapshot(&app, cx);
    assert_eq!(
        bella.selected_tts_voice_id.as_deref(),
        Some("af_bella"),
        "Use This Voice 后必须切换并保存用户指定的 Bella Voice"
    );
    assert_ne!(
        bella.tts_configuration_generation, preview_generation,
        "Apply Voice 配置必须推进 generation，使旧异步试听结果失效"
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

    click(&app, cx, "settings-tts-change-voice");
    settle(cx);
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);
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
