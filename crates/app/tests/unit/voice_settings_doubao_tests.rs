use super::*;

/// 音色搜索支持中文名称、大小写不敏感的 ID 与语言，清空后恢复完整列表。
///
/// 数据构造（含关键数值的推导过程）：
///   查询“小何”与大写 XIAOHE 应命中同一官方音色；“阿拉伯”命中外语列表。
///   不存在的名称为空结果；没有预算或阈值参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在搜索字段输入中文名称、ID、语言 → 分别读取过滤后的可选音色。
///   2. 输入未知名称 → 没有结果；清空字段 → 恢复音色目录。
///
/// 预期结果：小何存在而 Vivi 不匹配；外语音色可发现；清空不报错且不改当前选择。
#[test]
fn test_doubao_voice_search_finds_name_id_and_language() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.begin_edit(VoiceSettingsField::DoubaoVoiceSearch);
    state.auto_save_edit_value(VoiceSettingsField::DoubaoVoiceSearch, "小何".into());
    assert!(
        state
            .filtered_doubao_voices()
            .iter()
            .any(|voice| voice.id == "zh_female_xiaohe_uranus_bigtts"),
        "名称搜索应找到小何"
    );
    assert!(
        !state
            .filtered_doubao_voices()
            .iter()
            .any(|voice| voice.id == "zh_female_vv_uranus_bigtts"),
        "小何搜索不能混入 Vivi"
    );
    state.auto_save_edit_value(VoiceSettingsField::DoubaoVoiceSearch, "XIAOHE".into());
    assert!(
        state
            .filtered_doubao_voices()
            .iter()
            .any(|voice| voice.id == "zh_female_xiaohe_uranus_bigtts"),
        "音色 ID 搜索应忽略大小写"
    );
    state.auto_save_edit_value(VoiceSettingsField::DoubaoVoiceSearch, "阿拉伯".into());
    assert!(
        state
            .filtered_doubao_voices()
            .iter()
            .any(|voice| voice.id == "ar_female_dina_uranus_bigtts"),
        "应能按语言找到 Dina"
    );
    state.auto_save_edit_value(
        VoiceSettingsField::DoubaoVoiceSearch,
        "no-such-voice".into(),
    );
    assert!(
        state.filtered_doubao_voices().is_empty(),
        "未知名称不能伪造搜索结果"
    );
    assert_eq!(
        state
            .auto_save_edit_value(VoiceSettingsField::DoubaoVoiceSearch, String::new())
            .as_deref(),
        Some(""),
        "清空搜索必须被接受"
    );
    assert!(
        state
            .filtered_doubao_voices()
            .iter()
            .any(|voice| voice.id == "zh_female_vv_uranus_bigtts"),
        "清空后应恢复 Vivi"
    );
    assert_eq!(
        state.values().doubao_voice().id,
        "zh_female_vv_uranus_bigtts",
        "搜索不能隐式切换当前音色"
    );
}

/// 无效的旧存储值不能成为实际请求中的音色。
///
/// 数据构造（含关键数值的推导过程）：保存一个目录外 speaker；没有预算或阈值参数。
/// 执行过程：加载存储投影 → 校验音色目录 → 恢复原默认 Vivi。
/// 预期结果：默认 Vivi 存在；目录外 speaker 不进入可运行配置。
#[test]
fn test_doubao_invalid_saved_voice_restores_vivi() {
    let saved = SavedVoiceSettings {
        doubao_speaker: Some("unknown-speaker".into()),
        ..Default::default()
    };
    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    assert_eq!(
        values.doubao_voice().id,
        "zh_female_vv_uranus_bigtts",
        "无法识别的存储值应恢复原默认 Vivi"
    );
    assert_ne!(
        values.doubao_voice().id,
        "unknown-speaker",
        "未知存储值不能进入实际 TTS 配置"
    );
}

/// 豆包音色选择：后台保存后重载音色，并保留原语音链路的独立设置。
///
/// 数据构造（含关键数值的推导过程）：
///   空 SQLite 设置使用现有默认 VV；选择官方 2.0 音色小何。
///   旧链路音色使用独立 ID legacy-voice；没有预算或阈值参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 加载设置 → 读取默认 VV；选择小何 → 更新界面并排队保存。
///   2. 执行后台写入并切换模式 → 重新从 SQLite 加载设置。
///   3. 尝试选择不兼容的 1.0 音色 → 拒绝修改当前选择。
///
/// 预期结果：
///   - 小何被保存并恢复，默认 VV 不再被选中；前台选择不直接写库。
///   - 旧链路音色保留；目录外音色不能覆盖小何。
#[test]
fn test_doubao_voice_selection_persists_without_overwriting_legacy_voice() {
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string(values::KEY_TTS_VOICE_ID, "legacy-voice")
        .expect("应保存旧链路音色");
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    assert_eq!(
        state.values().doubao_voice().id,
        "zh_female_vv_uranus_bigtts",
        "升级默认应保留 VV"
    );
    assert!(
        state.select_doubao_voice("zh_female_xiaohe_uranus_bigtts"),
        "官方 2.0 小何音色应可选择"
    );
    assert_eq!(
        state.values().doubao_voice().id,
        "zh_female_xiaohe_uranus_bigtts",
        "界面应立即反映小何选择"
    );
    assert!(
        store
            .get_string("voice.doubao.speaker")
            .expect("应可读取保存前设置")
            .is_none(),
        "前台选择不能同步写库"
    );
    for write in state.take_pending_writes() {
        write.execute().expect("后台音色写入应成功");
    }
    state.record_persistence_result(Ok(()));
    let mut reloaded = VoiceSettingsState::load_from_store(store);
    reloaded.select_desktop_mode(DesktopVoiceMode::Legacy);
    reloaded.select_desktop_mode(DesktopVoiceMode::Doubao);
    assert_eq!(
        reloaded.values().doubao_voice().id,
        "zh_female_xiaohe_uranus_bigtts",
        "重载与模式切换应保留小何"
    );
    assert_ne!(
        reloaded.values().doubao_voice().id,
        "zh_female_vv_uranus_bigtts",
        "重载不能恢复为硬编码 VV"
    );
    assert_eq!(
        reloaded
            .values()
            .tts_voice_id()
            .map(tts::TtsVoiceId::as_str),
        Some("legacy-voice"),
        "豆包选择不能覆盖旧链路音色"
    );
    assert!(
        !reloaded.select_doubao_voice("zh_female_shuangkuaisisi_moon_bigtts"),
        "1.0 音色不能用于固定 2.0 资源"
    );
    assert_eq!(
        reloaded.values().doubao_voice().id,
        "zh_female_xiaohe_uranus_bigtts",
        "无效选择不能改变已选音色"
    );
}

/// VS-D1：升级后仍选择原语音链路，单次／连续模式不被豆包选择覆盖。
///
/// 数据构造（含关键数值的推导过程）：空设置代表升级前没有顶层模式键；
/// 连续聆听是旧链路内部的一个已保存值，没有预算或阈值参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析空设置 → 顶层模式使用旧链路默认值。
///   2. 保存豆包顶层模式和连续聆听内部模式 → 重新解析设置。
///
/// 预期结果：默认不是豆包；切到豆包后，旧的连续聆听配置仍保留。
#[test]
fn test_desktop_voice_mode_keeps_legacy_interaction_setting() {
    let mut saved = SavedVoiceSettings::default();
    let initial = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    assert_eq!(
        initial.desktop_mode(),
        DesktopVoiceMode::Legacy,
        "升级默认必须保持原语音链路"
    );
    assert_ne!(
        initial.desktop_mode(),
        DesktopVoiceMode::Doubao,
        "升级默认不能自行启动豆包"
    );
    saved.desktop_mode = Some(DesktopVoiceMode::Doubao);
    saved.interaction_mode = Some(VoiceInteractionMode::ContinuousListening);
    let selected = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    assert_eq!(
        selected.desktop_mode(),
        DesktopVoiceMode::Doubao,
        "保存的顶层选择应恢复豆包"
    );
    assert_eq!(
        selected.interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "顶层切换不能抹掉原链路的连续聆听选择"
    );
}

/// VS-D2：豆包凭据只写 Keychain 队列，设置及 Debug 中只留非敏感引用。
///
/// 数据构造（含关键数值的推导过程）：1 条合成密钥；1 个固定 Keychain account；
/// 没有 ASR、TTS 或 Agent Provider 配置，因此豆包与旧链路的就绪判断分离。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 选择豆包并输入密钥，触发自动保存。
///   2. 检查待写队列、设置投影与 Debug 输出。
///   3. 清除密钥并检查设置投影。
///
/// 预期结果：密钥仅进入 Keychain 写任务；设置不含明文，清除后引用消失。
#[test]
fn test_doubao_credential_is_keychain_only_and_clearable() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.select_desktop_mode(DesktopVoiceMode::Doubao);
    state.begin_edit(VoiceSettingsField::DoubaoApiKey);
    let saved = state.auto_save_edit_value(
        VoiceSettingsField::DoubaoApiKey,
        "synthetic-secret-123".into(),
    );
    assert_eq!(
        saved.as_deref(),
        Some(""),
        "密钥输入保存后不能把明文传回输入组件"
    );
    assert_eq!(
        state.values().doubao_credential_ref(),
        Some("doubao-desktop"),
        "配置投影应只保存 Keychain account"
    );
    assert!(
        !format!("{state:?}").contains("synthetic-secret-123"),
        "Debug 不得输出豆包密钥"
    );
    let writes = state.take_pending_writes();
    assert!(writes.iter().any(|write| matches!(write, PendingVoiceSettingWrite::StoreCredential { account, secret, .. } if account == "doubao-desktop" && secret == "synthetic-secret-123")), "密钥应通过后台 SQLite 写任务落地");
    assert!(!writes.iter().any(|write| matches!(write, PendingVoiceSettingWrite::Set { value, .. } if value == "synthetic-secret-123")), "SQLite 设置不得保存密钥明文");
    state.record_persistence_result(Ok(()));
    state.clear_doubao_credential();
    assert!(
        state.values().doubao_credential_ref().is_none(),
        "清除后豆包凭据引用必须消失"
    );
    assert_ne!(
        state.values().doubao_credential_ref(),
        Some("doubao-desktop"),
        "清除后不能继续显示已配置"
    );
}

/// VS-D3：豆包模式不显示旧 TTS 扫描的“只能文本回复”提示。
///
/// 数据构造（含关键数值的推导过程）：一个未安装的旧 TTS 模型对应一条旧链路状态；
/// 两种顶层模式各运行一次扫描回调，没有预算或阈值参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 选择豆包后模拟旧 TTS 扫描状态 → 保留豆包设置的保存反馈。
///   2. 切回旧链路后再次报告相同扫描状态 → 显示旧 TTS 提示。
///   3. 再切到豆包 → 清掉旧模式留下的状态提示。
///
/// 预期结果：豆包不误报或保留旧 TTS 提示；旧模式继续提供原有缺模型提示。
#[test]
fn test_doubao_mode_ignores_legacy_tts_status() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.select_desktop_mode(DesktopVoiceMode::Doubao);
    state.report_tts_status(
        "No Local TTS model is selected; replies will stay text-only.",
        false,
    );
    assert!(
        !state
            .status()
            .is_some_and(|(message, _)| message.contains("Local TTS")),
        "豆包模式不能显示旧 TTS 缺失提示"
    );
    state.select_desktop_mode(DesktopVoiceMode::Legacy);
    state.report_tts_status(
        "No Local TTS model is selected; replies will stay text-only.",
        false,
    );
    assert!(
        state
            .status()
            .is_some_and(|(message, _)| message.contains("Local TTS")),
        "旧模式仍需显示本地 TTS 状态"
    );
    state.select_desktop_mode(DesktopVoiceMode::Doubao);
    assert_eq!(
        state.status().map(|(message, _)| message),
        Some("Saved for this session"),
        "切换到豆包应显示本次设置反馈，而不是旧 TTS 提示"
    );
}
