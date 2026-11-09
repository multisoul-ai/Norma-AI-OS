use super::*;

/// 迁移后的确认规则在App配置往返中保持范围，旧字符串规则继续全局生效。
/// 数据：一个全局Write规则和一个alpha专属Bash规则，另读一次旧字符串配置。
/// 执行：生产解析、序列化、再次解析，再按实际workspace匹配规则。
/// 预期：alpha受项目规则约束，beta不受它影响；全局规则覆盖两者及未来项目。
#[test]
fn migrated_confirmation_rules_keep_scope_through_settings_round_trip() {
    let settings = parse_settings(
        r#"{"always_ask":[{"workspace_id":null,"key":"Write"},{"workspace_id":"alpha","key":"Bash:cargo"}]}"#,
    );
    assert_eq!(settings.always_ask.len(), 2, "不能丢掉迁移后的结构化规则");
    let saved = settings_json(&settings).unwrap();
    let reloaded = parse_settings(&saved);
    assert_eq!(reloaded, settings, "序列化后规则身份保持");
    assert!(
        reloaded
            .always_ask
            .iter()
            .any(|rule| rule.matches(Some("alpha"), "Bash:cargo")),
        "alpha原规则生效"
    );
    assert!(
        !reloaded
            .always_ask
            .iter()
            .any(|rule| rule.matches(Some("beta"), "Bash:cargo")),
        "不能扩大到beta"
    );
    assert!(
        reloaded
            .always_ask
            .iter()
            .any(|rule| rule.matches(Some("future"), "Write")),
        "全局规则覆盖未来项目"
    );
    let legacy = parse_settings(r#"{"always_ask":["Write"]}"#);
    assert!(
        legacy.always_ask[0].matches(Some("alpha"), "Write"),
        "旧字符串仍为全局规则"
    );
}

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

/// nvm 版本目录按数值比较，v9 不能排在 v22 前面；非版本目录被忽略。
/// 预期结果：解析出数值元组，排序后 v22 大于 v9。
#[test]
fn node_versions_compare_numerically() {
    assert_eq!(super::node_version("v22.22.0"), Some((22, 22, 0)));
    assert_eq!(super::node_version("default"), None);
    assert!(super::node_version("v22.0.0") > super::node_version("v9.11.2"));
}

/// 含空格的 norma 路径改用状态目录下的无空格符号链接，普通路径原样返回。
/// 数据构造：临时状态目录和一个名为 `Norma OS.app` 的假可执行文件。
/// 预期结果：返回链接路径、不含空格、指向原文件；重复调用得到同一路径。
#[test]
fn spaced_norma_bin_gets_a_shell_safe_link() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("Norma OS.app");
    std::fs::write(&real, "").unwrap();
    let state = dir.path().join("state");
    let link = super::shell_safe_bin(&state, real.clone());
    assert!(!link.to_string_lossy().contains(' '));
    assert_eq!(std::fs::read_link(&link).unwrap(), real);
    assert_eq!(super::shell_safe_bin(&state, real), link);
    let plain = std::path::PathBuf::from("/bin/norma");
    assert_eq!(super::shell_safe_bin(&state, plain.clone()), plain);
}
