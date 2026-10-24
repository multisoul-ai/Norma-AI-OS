use super::*;

/// Codex、OpenCode 的模型按后端分别保存，写入再读回不变；Claude Code 仍用原来的两项。
///
/// 数据构造：Codex 主会话 gpt-6-astra、审批不设；OpenCode 两项都设。
/// 预期结果：
///   - 正断言：读回的 backend_models 与写入一致；旧配置（没有这一项）读出为空
///   - 负断言：Codex 的模型不会写进 Claude 的 main_model
#[test]
fn backend_models_round_trip_per_backend() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = BrainSettings::default();
    settings.backend_models.insert(
        BackendKind::Codex,
        ModelPair {
            main: Some("gpt-6-astra".into()),
            approval: None,
        },
    );
    settings.backend_models.insert(
        BackendKind::OpenCode,
        ModelPair {
            main: Some("opencode/big-pickle".into()),
            approval: Some("opencode/beam-free".into()),
        },
    );
    save_settings_to(dir.path(), &settings).unwrap();
    let read = parse_settings(&std::fs::read_to_string(dir.path().join("config.json")).unwrap());
    assert_eq!(read.backend_models, settings.backend_models);
    assert_eq!(read.main_model, None, "Codex 的模型不能变成 Claude 的");
    assert!(parse_settings("{}").backend_models.is_empty());
}

/// 启动时按所选后端取模型：Claude Code 用原来的两项，其他后端用各自保存的。
///
/// 数据构造：Claude 主会话 opus；Codex 主会话 gpt-6-astra。
/// 预期结果：
///   - 正断言：选 Claude 得到 opus，选 Codex 得到 gpt-6-astra
///   - 负断言：选 OpenCode（没保存）得到空，交给 CLI 默认
#[test]
fn models_for_backend_follow_the_chosen_backend() {
    let mut settings = BrainSettings {
        main_model: Some("opus".into()),
        ..BrainSettings::default()
    };
    settings.backend_models.insert(
        BackendKind::Codex,
        ModelPair {
            main: Some("gpt-6-astra".into()),
            approval: None,
        },
    );
    assert_eq!(
        settings.models_for(BackendKind::ClaudeCode).main.as_deref(),
        Some("opus")
    );
    assert_eq!(
        settings.models_for(BackendKind::Codex).main.as_deref(),
        Some("gpt-6-astra")
    );
    assert_eq!(
        settings.models_for(BackendKind::OpenCode),
        ModelPair::default()
    );
}
