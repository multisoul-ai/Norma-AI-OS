use super::*;
use crate::settings::voice::{EnvVoiceSettings, ProviderDiscoveryStatus, SavedVoiceSettings};
use gpui::TestAppContext;
use ui::i18n::{set_app_locale, Locale};

/// 多窗口已创建语音输入框立即更新占位符，新窗口继承语言，输入和焦点保持。
///
/// 数据构造：同一 App 的两个窗口，各自持有 Model 输入；第三窗口在切换后创建。
///   第一输入 = "my-model"（8 ASCII 字节），选择 3..8（model 部分）；共 3 个窗口。
/// 执行过程：英文创建 → 写入草稿与选区并聚焦 → 切中文 → 检查两窗口 → 创建第三窗口。
/// 预期结果：三个输入均显示中文占位；第一输入的内容、选区与焦点不变，不回填正式值。
#[test]
fn language_refresh_preserves_inputs_and_reaches_all_windows() {
    let mut test_cx = TestAppContext::single();
    test_cx.update(|cx| set_app_locale(Locale::En, cx));
    let factory = || {
        Box::new(|_cmd, _cols, _rows, tx| {
            Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
        }) as runtime::PtyFactory
    };
    let first_window =
        test_cx.add_window(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    let first = first_window.root(&mut test_cx).expect("窗口必须包含 App");
    let first_input = first.read_with(&test_cx, |app, _| {
        app.voice_settings_inputs.input(VoiceSettingsField::Model)
    });
    first_window
        .update(&mut test_cx, |_, window, cx| {
            first_input.update(cx, |input, cx| {
                input.sync_value("my-model".into(), cx);
                input.selected_range = 3..8;
                input.focus_handle.focus(window);
            })
        })
        .expect("第一窗口应能设置输入");
    let second_window =
        test_cx.add_window(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    let second = second_window.root(&mut test_cx).expect("窗口必须包含 App");
    let second_input = second.read_with(&test_cx, |app, _| {
        app.voice_settings_inputs.input(VoiceSettingsField::Model)
    });
    test_cx.update(|cx| set_app_locale(Locale::ZhCn, cx));
    test_cx.run_until_parked();
    assert_eq!(
        first_input.read_with(&test_cx, |input, _| input.placeholder.to_string()),
        "模型名称",
        "已有第一窗口应立即更新占位符"
    );
    assert_eq!(
        second_input.read_with(&test_cx, |input, _| input.placeholder.to_string()),
        "模型名称",
        "已有第二窗口不能残留英文"
    );
    assert_eq!(
        first_input.read_with(&test_cx, |input, _| input.content.to_string()),
        "my-model",
        "语言切换不能覆盖输入草稿"
    );
    assert_eq!(
        first_input.read_with(&test_cx, |input, _| input.selected_range.clone()),
        3..8,
        "语言切换必须保留选区"
    );
    assert!(
        first_window
            .update(&mut test_cx, |_, window, cx| first_input
                .read(cx)
                .focus_handle
                .is_focused(window))
            .expect("第一窗口应仍存在"),
        "原输入必须保持键盘焦点"
    );
    let third_window =
        test_cx.add_window(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    let third = third_window.root(&mut test_cx).expect("窗口必须包含 App");
    let third_input = third.read_with(&test_cx, |app, _| {
        app.voice_settings_inputs.input(VoiceSettingsField::Model)
    });
    assert_eq!(
        third_input.read_with(&test_cx, |input, _| input.placeholder.to_string()),
        "模型名称",
        "新窗口必须继承当前中文"
    );
    assert_ne!(
        third_input.read_with(&test_cx, |input, _| input.placeholder.to_string()),
        "Model name",
        "新窗口不能使用默认英文"
    );
    test_cx.update(|cx| set_app_locale(Locale::En, cx));
}

/// 已缓存的模型目录空状态与分类错误随语言变化，技术诊断保持原文。
///
/// 数据构造：空模型目录 0 项；ModelListUnavailable 原因为 HTTP 404，共两种已有状态。
/// 执行过程：英文时记录空目录 → 中文投影；记录分类错误 → 分别读取两种语言。
/// 预期结果：空目录提示变中文；目录错误两种语言均保留 HTTP 404，中文不含英文产品模板。
#[test]
fn language_refresh_updates_cached_provider_failures() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.record_provider_models(vec![]);
    let ProviderDiscoveryStatus::Failed(empty) = state.provider_discovery() else {
        panic!("空目录应产生产品错误状态");
    };
    assert!(
        empty.resolve_for(Locale::ZhCn).contains("未返回可选模型"),
        "缓存空目录提示应有中文投影"
    );
    assert!(
        !empty
            .resolve_for(Locale::ZhCn)
            .contains("Provider returned"),
        "中文缓存不能保留英文产品句子"
    );
    state.record_provider_catalog_error(agent::ProviderCatalogError::ModelListUnavailable(
        "HTTP 404".into(),
    ));
    let ProviderDiscoveryStatus::ManualRequired(error) = state.provider_discovery() else {
        panic!("目录不可用应允许手动输入");
    };
    assert!(
        error.resolve_for(Locale::ZhCn).contains("模型列表不可用"),
        "域错误应在展示层映射中文"
    );
    assert!(
        error
            .resolve_for(Locale::En)
            .contains("Model list unavailable"),
        "同一缓存应有英文模板"
    );
    assert!(
        error.resolve_for(Locale::ZhCn).contains("HTTP 404"),
        "原始技术诊断必须保留"
    );
    assert!(
        !error
            .resolve_for(Locale::ZhCn)
            .contains("Model list unavailable"),
        "切换后不能残留英文模板"
    );
}
