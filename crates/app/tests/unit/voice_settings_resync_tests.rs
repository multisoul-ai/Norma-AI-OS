use super::*;
use crate::settings::voice::state::SavedProviderProfile;
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState};

fn drain(state: &mut VoiceSettingsState) {
    for write in state.take_pending_writes() {
        write.execute().expect("后台写入应成功");
    }
}

/// VR-1：另一个窗口/项目写入的全局语音设置，过期实例重新同步后必须可见。
///
/// 数据构造（含关键数值的推导过程）：
///   同一份 app.sqlite 上加载两个独立的 VoiceSettingsState（模拟宿主与项目会话）。
///   实例 b 把桌面模式改为 Doubao、豆包交互改为单次按住说话，并落库；实例 a 不知情。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. a、b 同时从库加载，默认桌面模式为 Legacy。
///   2. b 修改并执行后台写入。
///   3. a 用库中最新快照调用 apply_saved_snapshot。
///
/// 预期结果：
///   - 正断言：a 返回 true，桌面模式为 Doubao，豆包交互为按住说话。
///   - 负断言：同步前 a 仍是旧值，证明测试确实覆盖了“过期”场景。
#[test]
fn test_resync_picks_up_settings_written_by_another_instance() {
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut stale = VoiceSettingsState::load_from_store(store.clone());
    let mut writer = VoiceSettingsState::load_from_store(store.clone());
    writer.select_desktop_mode(DesktopVoiceMode::Doubao);
    writer.select_doubao_interaction_mode(VoiceInteractionMode::SingleVoiceCommand);
    drain(&mut writer);
    assert_ne!(
        stale.values().desktop_mode(),
        DesktopVoiceMode::Doubao,
        "同步前过期实例不应已经看到新模式"
    );

    let snapshot = SavedVoiceSettings::load(&store).expect("应读取最新快照");
    let changed = stale.apply_saved_snapshot(snapshot, false);

    assert!(changed, "快照与内存不同，应报告已变化");
    assert_eq!(
        stale.values().desktop_mode(),
        DesktopVoiceMode::Doubao,
        "同步后应看到全局桌面模式"
    );
    assert_eq!(
        stale.values().doubao_interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "同步后应看到全局豆包按住说话"
    );
}

/// VR-2：本实例存在未落库的修改时，同步不得用旧库值覆盖它。
///
/// 数据构造：空库；实例先选择 Doubao 但不执行写入（pending_writes 非空）。
/// 执行过程：用库中快照（桌面模式仍为默认 Legacy）调用同步。
/// 预期结果：
///   - 正断言：返回 false，且桌面模式仍是用户刚选的 Doubao。
///   - 负断言：不得回退为 Legacy，也不得丢失待写入项。
#[test]
fn test_resync_never_clobbers_unsaved_local_edit() {
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.select_desktop_mode(DesktopVoiceMode::Doubao);

    let snapshot = SavedVoiceSettings::load(&store).expect("应读取快照");
    let changed = state.apply_saved_snapshot(snapshot, false);

    assert!(!changed, "存在待写入修改时应跳过同步");
    assert_eq!(
        state.values().desktop_mode(),
        DesktopVoiceMode::Doubao,
        "未保存的本地选择不能被旧库值覆盖"
    );
    assert!(
        state.settings_persistence_pending(),
        "待写入项必须保留以便稍后落库"
    );
}

/// VR-3：同步只刷新配置值，不得重置运行时就绪状态（麦克风授权）。
///
/// 数据构造：实例已确认麦克风 Ready；库里桌面模式被他处改为 Doubao。
/// 执行过程：同步最新快照。
/// 预期结果：
///   - 正断言：配置已更新为 Doubao。
///   - 正断言：麦克风仍为 Ready（否则每次切换项目都要重新授权/配置）。
#[test]
fn test_resync_preserves_runtime_readiness() {
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut stale = VoiceSettingsState::load_from_store(store.clone());
    stale.set_microphone_requirement(VoiceRequirementState::Ready);
    let mut writer = VoiceSettingsState::load_from_store(store.clone());
    writer.select_desktop_mode(DesktopVoiceMode::Doubao);
    drain(&mut writer);

    let snapshot = SavedVoiceSettings::load(&store).expect("应读取快照");
    stale.apply_saved_snapshot(snapshot, false);

    assert_eq!(
        stale.values().desktop_mode(),
        DesktopVoiceMode::Doubao,
        "同步后应采用全局保存的豆包模式"
    );
    assert_eq!(
        stale.requirement_state(VoiceRequirement::Microphone),
        VoiceRequirementState::Ready,
        "同步不能清掉已确认的麦克风授权"
    );
}

/// VR-4：凭据引用未变时保留已加载密钥；广播刷新（reload_credentials）时丢弃以便重新读取。
///
/// 数据构造：OpenAI 配置 credential_ref="acct"、已加载 api_key="secret"；
///   快照是库中形态（引用相同、api_key 为 None，密钥只在钥匙串里）。
/// 执行过程：先 reload_credentials=false 同步，再 reload_credentials=true 同步。
/// 预期结果：
///   - 第一次：credential_refs() 为空（密钥保留，无需再读钥匙串），也不报告变化。
///   - 第二次：credential_refs() 含 (OpenAiCompatible,"acct")，触发重新读取轮换后的密钥。
#[test]
fn test_resync_keeps_loaded_secret_unless_reload_requested() {
    let with_secret = SavedVoiceSettings {
        openai_profile: SavedProviderProfile {
            credential_ref: Some("acct".into()),
            api_key: Some("secret".into()),
            ..SavedProviderProfile::default()
        },
        ..SavedVoiceSettings::default()
    };
    let from_store = SavedVoiceSettings {
        openai_profile: SavedProviderProfile {
            credential_ref: Some("acct".into()),
            api_key: None,
            ..SavedProviderProfile::default()
        },
        ..SavedVoiceSettings::default()
    };
    let mut state = VoiceSettingsState::from_sources(with_secret, EnvVoiceSettings::default());

    let changed = state.apply_saved_snapshot(from_store.clone(), false);
    assert!(!changed, "仅密钥形态不同不应算配置变化");
    assert!(
        state.credential_refs().is_empty(),
        "引用未变时应保留已加载密钥"
    );

    state.apply_saved_snapshot(from_store, true);
    assert_eq!(
        state.credential_refs(),
        vec![(AgentProviderKind::OpenAiCompatible, "acct".to_string())],
        "广播刷新应丢弃旧密钥并要求重新读取"
    );
}

/// VR-5：后台读库期间发生过落库，读到的旧快照必须被丢弃，不能把用户刚改的值回滚。
///
/// 数据构造：空库；实例先记录 baseline 与 epoch，再选择 Doubao 并完整执行一轮落库
///   （take_pending_writes 与 record_persistence_result 各使 epoch 递增 1，共 +2）。
///   随后到达的快照是“写入之前读到的”旧值（桌面模式为默认 Legacy）。
/// 执行过程：落库完成后 pending 已清零、saved 与新值一致，仅靠 pending/整体比较无法发现过期。
/// 预期结果：
///   - 正断言：apply_saved_snapshot_if_unchanged 返回 false。
///   - 负断言：桌面模式仍是 Doubao，没有被旧快照改回 Legacy。
#[test]
fn test_resync_discards_snapshot_read_before_a_local_write_completed() {
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    let stale_snapshot = SavedVoiceSettings::load(&store).expect("应读取旧快照");
    let baseline = state.saved_snapshot();
    let epoch = state.persistence_epoch();
    let generation = state.begin_saved_snapshot_sync();

    state.select_desktop_mode(DesktopVoiceMode::Doubao);
    let writes = state.take_pending_writes();
    for write in writes {
        write.execute().expect("后台写入应成功");
    }
    state.record_persistence_result(Ok(()));
    let baseline_after_write = state.saved_snapshot();
    assert_ne!(baseline, baseline_after_write, "本地写入应已改变内存配置");

    let applied = state.apply_saved_snapshot_if_unchanged(
        &baseline_after_write,
        epoch,
        generation,
        stale_snapshot,
        false,
    );

    assert!(!applied, "读库期间发生过落库，旧快照必须被丢弃");
    assert_eq!(
        state.values().desktop_mode(),
        DesktopVoiceMode::Doubao,
        "用户刚保存的值不能被旧快照回滚"
    );
}

/// VR-6：异步加载钥匙串密钥只改变运行时密钥，不应使 baseline 比较失败而丢弃快照。
///
/// 数据构造：baseline 无密钥；实例随后“加载”到密钥（saved 中 api_key 变为 Some），
///   库中快照把桌面模式改成 Doubao，epoch 未变。
/// 预期结果：
///   - 正断言：快照被应用（返回 true），桌面模式为 Doubao。
///   - 正断言：已加载的密钥仍保留。
#[test]
fn test_resync_ignores_secret_loading_when_checking_baseline() {
    let baseline = SavedVoiceSettings {
        openai_profile: SavedProviderProfile {
            credential_ref: Some("acct".into()),
            api_key: None,
            ..SavedProviderProfile::default()
        },
        ..SavedVoiceSettings::default()
    };
    let mut loaded = baseline.clone();
    loaded.openai_profile.api_key = Some("secret".into());
    let mut state = VoiceSettingsState::from_sources(loaded, EnvVoiceSettings::default());
    let mut snapshot = baseline.clone();
    snapshot.desktop_mode = Some(DesktopVoiceMode::Doubao);

    let epoch = state.persistence_epoch();
    let generation = state.begin_saved_snapshot_sync();
    let applied =
        state.apply_saved_snapshot_if_unchanged(&baseline, epoch, generation, snapshot, false);

    assert!(applied, "仅密钥加载不应使快照被判定为过期");
    assert_eq!(
        state.values().desktop_mode(),
        DesktopVoiceMode::Doubao,
        "密钥加载不能阻止模式同步"
    );
    assert!(
        state.credential_refs().is_empty(),
        "已加载的密钥应保留，无需再读钥匙串"
    );
}

/// VR-7：provider / TTS 配置差异判定只对相关字段敏感，避免无谓的网络校验与 TTS 重建。
///
/// 预期结果：
///   - 仅桌面模式不同：两者都为 false。
///   - 改 OpenAI 模型：provider 差异为 true，TTS 差异为 false。
///   - 改 MiniMax 音色：TTS 差异为 true，provider 差异为 false。
#[test]
fn test_configuration_diff_helpers_are_field_scoped() {
    use crate::settings::voice::{provider_configuration_differs, tts_configuration_differs};
    let base = SavedVoiceSettings::default();

    let mode_only = SavedVoiceSettings {
        desktop_mode: Some(DesktopVoiceMode::Doubao),
        ..base.clone()
    };
    assert!(
        !provider_configuration_differs(&base, &mode_only),
        "桌面模式不属于 provider 配置"
    );
    assert!(
        !tts_configuration_differs(&base, &mode_only),
        "桌面模式不属于 TTS 配置"
    );

    let mut provider_changed = base.clone();
    provider_changed.openai_profile.model = Some("gpt-x".into());
    assert!(
        provider_configuration_differs(&base, &provider_changed),
        "改模型应触发 provider 重新校验"
    );
    assert!(
        !tts_configuration_differs(&base, &provider_changed),
        "改 provider 不应重建 TTS"
    );

    let tts_changed = SavedVoiceSettings {
        minimax_tts_voice_id: tts::TtsVoiceId::parse("v1").ok(),
        ..base.clone()
    };
    assert!(
        tts_configuration_differs(&base, &tts_changed),
        "改 MiniMax 音色应重建 TTS"
    );
    assert!(
        !provider_configuration_differs(&base, &tts_changed),
        "改音色不应重新校验 provider"
    );
}

/// 并发广播：接收者没有本地修改时，较新的快照不能被先到的远端快照阻挡。
///
/// 数据构造（含关键数值的推导过程）：
///   接收者初始 S0；两次读取都捕获 S0 和同一落库 epoch。
///   S1 只切换到 Doubao；S2 在 S1 上再切换到按住说话，共 2 次远端修改、0 次本地写入。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 两次同步读取在任一回调完成前开始。
///   2. 先处理 S1，再处理 S2，检查最终设置。
///
/// 预期结果：
///   - 正断言：最新 S2 被接受，桌面模式为 Doubao、交互模式为单次按住说话。
///   - 负断言：不能停留在 S1 的持续聆听模式，且没有产生本地落库任务。
#[test]
fn concurrent_resync_keeps_the_latest_remote_settings() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let baseline = state.saved_snapshot();
    let epoch = state.persistence_epoch();
    let first_generation = state.begin_saved_snapshot_sync();
    let latest_generation = state.begin_saved_snapshot_sync();
    let first = SavedVoiceSettings {
        desktop_mode: Some(DesktopVoiceMode::Doubao),
        ..baseline.clone()
    };
    let latest = SavedVoiceSettings {
        doubao_interaction_mode: Some(VoiceInteractionMode::SingleVoiceCommand),
        ..first.clone()
    };

    let stale_applied =
        state.apply_saved_snapshot_if_unchanged(&baseline, epoch, first_generation, first, false);
    let applied =
        state.apply_saved_snapshot_if_unchanged(&baseline, epoch, latest_generation, latest, false);

    assert!(applied, "最新远端快照不能被另一个远端回调造成的变化阻挡");
    assert!(!stale_applied, "较新的同步已经开始时不能先应用旧快照");
    assert_eq!(
        state.values().desktop_mode(),
        DesktopVoiceMode::Doubao,
        "最新快照应保留远端选择的 Doubao 模式"
    );
    assert_eq!(
        state.values().doubao_interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "接收者最终必须采用较新的按住说话模式"
    );
    assert_ne!(
        state.values().doubao_interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "不能停留在较旧快照的持续聆听模式"
    );
    assert!(
        !state.settings_persistence_pending(),
        "远端同步不能产生本地写入"
    );
}

/// 并发读取乱序返回：最新请求先完成后，旧请求不能回滚已经应用的新设置。
///
/// 数据构造：初始 S0；两次读取共享 S0 和同一 epoch，最新 S2 为 Doubao 按住说话，
///   旧 S1 为 Doubao 持续聆听，共 2 个同步请求、0 个本地写入。
///
/// 执行过程：
///   1. 开始旧请求和最新请求。
///   2. 先应用最新 S2，再应用迟到的旧 S1。
///
/// 预期结果：
///   - 正断言：S2 应用成功，最终保留按住说话。
///   - 负断言：S1 被拒绝，不产生本地落库任务。
#[test]
fn concurrent_resync_rejects_late_older_snapshot() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let baseline = state.saved_snapshot();
    let epoch = state.persistence_epoch();
    let older = state.begin_saved_snapshot_sync();
    let newer = state.begin_saved_snapshot_sync();
    let latest = SavedVoiceSettings {
        desktop_mode: Some(DesktopVoiceMode::Doubao),
        doubao_interaction_mode: Some(VoiceInteractionMode::SingleVoiceCommand),
        ..baseline.clone()
    };
    let old = SavedVoiceSettings {
        doubao_interaction_mode: Some(VoiceInteractionMode::ContinuousListening),
        ..latest.clone()
    };
    assert!(
        state.apply_saved_snapshot_if_unchanged(&baseline, epoch, newer, latest, false),
        "最新请求应成功应用"
    );
    assert!(
        !state.apply_saved_snapshot_if_unchanged(&baseline, epoch, older, old, false),
        "迟到的旧请求不得回滚最新设置"
    );
    assert_eq!(
        state.values().doubao_interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "乱序返回后应保留最新按住说话模式"
    );
    assert!(
        !state.settings_persistence_pending(),
        "读取不能产生本地写入"
    );
}
