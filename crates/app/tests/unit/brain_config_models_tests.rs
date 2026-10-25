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

/// 同一项目目录的不同画布拥有独立目标和提议，重启读取也不能串用。
/// 数据构造：两个稳定画布身份，分别保存 A/B 目标与提议文本。
/// 执行过程：通过会话目录接口定位文件，写入后重新打开目标存储。
/// 预期结果：目录不同，每个画布只恢复自己的目标和提议。
#[test]
fn canvas_state_survives_restart_without_crossing_ownership() {
    let dir = tempfile::tempdir().expect("应创建隔离状态目录");
    let a = session_dir(dir.path(), "desktop:A:area");
    let b = session_dir(dir.path(), "desktop:B:area");
    assert_ne!(a, b, "相同项目目录的不同画布不能共享状态目录");
    for (path, name) in [(&a, "A"), (&b, "B")] {
        let goals = brain::goal::GoalStore::open(path.clone());
        goals.record("Alan", &format!("只处理 {name} 项目的目标"));
        brain::persistence::atomic_write(&path.join("proposals.json"), name.as_bytes()).unwrap();
    }
    let reopened_a = brain::goal::GoalStore::open(a.clone());
    assert!(
        reopened_a.summary().unwrap().contains("A 项目"),
        "应恢复 A 目标"
    );
    assert!(
        !reopened_a.summary().unwrap().contains("B 项目"),
        "不能带入 B 目标"
    );
    assert_eq!(
        std::fs::read_to_string(a.join("proposals.json")).unwrap(),
        "A",
        "应保留 A 提议"
    );
    assert_eq!(
        std::fs::read_to_string(b.join("proposals.json")).unwrap(),
        "B",
        "不能覆盖 B 提议"
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
