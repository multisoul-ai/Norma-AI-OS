use norma::agent_hook::{
    agent_hooks_installed_for_source, hook_events_for_source, install_hook_config,
    HookInstallPaths, HookSource, HookTarget,
};

fn hook_paths(root: &std::path::Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: root.join("claude-settings.json"),
        codex_hooks: root.join("codex-hooks.json"),
        cursor_hooks: root.join("cursor-hooks.json"),
        grok_hooks: root.join("grok-hooks.json"),
        pi_extension: root.join("norma-agent-hook.ts"),
        opencode_plugin: root.join("multisoul-agent-hook.js"),
    }
}

/// JSON Hook 完整性：缺少任一必需事件时不得误报已安装，Repair 必须补回。
///
/// 数据构造（含关键数值的推导过程）：
///   source = Codex
///   required events = hook_events_for_source(Codex).len() = N
///   removed events  = 1
///   incomplete set  = N - 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用生产安装器写入 Codex 全量事件。
///   2. 从 JSON 精确删除最后一个必需事件。
///   3. 重新检测完整性，再执行一次 install 修复。
///
/// 预期结果：
///   - 正断言：全量安装与修复后均检测为已安装。
///   - 负断言：仅剩 N-1 个事件时不得因“至少一个命中”而误报已安装。
#[test]
fn codex_requires_every_hook_event_and_repair_restores_missing_event() {
    let temp = tempfile::tempdir().expect("Codex Hook 完整性测试必须创建临时目录");
    let paths = hook_paths(temp.path());
    let command = temp.path().join("multisoul-agent-hook");
    install_hook_config(HookTarget::Codex, &command, &paths).expect("Codex 全量 Hook 安装必须成功");
    let events = hook_events_for_source(HookSource::Codex);
    let removed = events.last().expect("Codex 必须至少声明一个 Hook 事件");
    let mut root: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&paths.codex_hooks).expect("Codex Hook JSON 必须可读取"),
    )
    .expect("安装器写出的 Codex Hook 必须是合法 JSON");

    assert!(
        agent_hooks_installed_for_source(HookSource::Codex, &paths),
        "包含全部 N 个事件时 Codex Hook 必须检测为已安装"
    );
    root.get_mut("hooks")
        .and_then(serde_json::Value::as_object_mut)
        .expect("Codex Hook JSON 必须包含 hooks object")
        .remove(*removed);
    std::fs::write(
        &paths.codex_hooks,
        serde_json::to_vec_pretty(&root).expect("删事件后的 JSON 必须可序列化"),
    )
    .expect("测试必须写入 N-1 事件配置");

    assert!(
        !agent_hooks_installed_for_source(HookSource::Codex, &paths),
        "缺少任一必需事件时不得把 N-1 配置误报为完整"
    );
    install_hook_config(HookTarget::Codex, &command, &paths).expect("重复安装必须补回缺失事件");
    assert!(
        agent_hooks_installed_for_source(HookSource::Codex, &paths),
        "Repair 补回缺失事件后必须恢复完整状态"
    );
}

/// Cursor Hook 完整性：扁平 handlers 配置也必须要求全部事件存在。
///
/// 数据构造（含关键数值的推导过程）：
///   source = Cursor
///   required events = N
///   removed events  = 1
///   remaining events = N - 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装 Cursor 全量扁平 Hook 配置。
///   2. 精确删除第一个必需事件。
///   3. 检查安装状态。
///
/// 预期结果：
///   - 正断言：完整 Cursor 配置报告已安装。
///   - 负断言：缺一个事件后必须报告未安装。
#[test]
fn cursor_requires_every_flat_hook_event() {
    let temp = tempfile::tempdir().expect("Cursor Hook 完整性测试必须创建临时目录");
    let paths = hook_paths(temp.path());
    let command = temp.path().join("multisoul-agent-hook");
    install_hook_config(HookTarget::Cursor, &command, &paths)
        .expect("Cursor 全量 Hook 安装必须成功");
    let removed = hook_events_for_source(HookSource::Cursor)
        .first()
        .expect("Cursor 必须至少声明一个 Hook 事件");
    let mut root: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&paths.cursor_hooks).expect("Cursor Hook JSON 必须可读取"),
    )
    .expect("安装器写出的 Cursor Hook 必须是合法 JSON");

    assert!(
        agent_hooks_installed_for_source(HookSource::Cursor, &paths),
        "Cursor 全部事件存在时必须检测为已安装"
    );
    root.get_mut("hooks")
        .and_then(serde_json::Value::as_object_mut)
        .expect("Cursor Hook JSON 必须包含 hooks object")
        .remove(*removed);
    std::fs::write(
        &paths.cursor_hooks,
        serde_json::to_vec_pretty(&root).expect("删事件后的 Cursor JSON 必须可序列化"),
    )
    .expect("测试必须写入缺一个事件的 Cursor 配置");

    assert!(
        !agent_hooks_installed_for_source(HookSource::Cursor, &paths),
        "Cursor 缺少任一事件时不得因其他事件命中而误报完整"
    );
}
