/// Workspace open instrumentation: 复现卡顿时必须能看到每个打开阶段的耗时。
///
/// 数据构造：production_source = flow/windowing/canvas view 去掉 `mod tests` 后的源码；
/// labels = 12 个 `workspace_open_*` 阶段标签。
///
/// 执行过程：读取源码 → 搜索阶段标签 → 搜索首帧 render 触发点。
///
/// 预期结果：标签全部存在；旧单条 aggregate restore 日志不存在。
#[test]
fn test_workspace_open_flow_has_timing_stage_labels() {
    let flow = include_str!("../../src/workspace/flow.rs");
    let windowing = include_str!("../../src/workspace/windowing.rs");
    let canvas_view = include_str!("../../src/canvas/view.rs");
    let production_source = [
        flow.split("\n#[cfg(test)]")
            .next()
            .expect("flow.rs should contain production source before tests"),
        windowing
            .split("\n#[cfg(test)]\nmod tests")
            .next()
            .expect("windowing.rs should contain production source before tests"),
        canvas_view,
    ]
    .join("\n");

    for label in [
        "workspace_open_project_window",
        "workspace_open_record_recent",
        "workspace_open_project_store",
        "workspace_open_migrate",
        "workspace_open_flush_previous",
        "workspace_open_load_canvas_snapshot",
        "workspace_open_restore_layout",
        "workspace_open_load_checkpoints",
        "workspace_open_load_node_sessions",
        "workspace_open_restore_live_sessions",
        "workspace_open_restore_live_sessions_deferred_start",
        "workspace_open_restore_live_session_node",
        "workspace_open_restore_live_sessions_deferred_done",
        "workspace_open_total",
        "workspace_open_first_canvas_render",
    ] {
        assert!(
            production_source.contains(label),
            "workspace open instrumentation should include timing label {label}"
        );
    }
    assert!(
        production_source.contains("mark_canvas_open_render_pending"),
        "workspace open instrumentation should mark the next canvas render for first-frame timing"
    );
    assert!(
        !production_source.contains("restored live sessions for opened workspace\""),
        "workspace open instrumentation should replace the old single aggregate log with staged timing logs"
    );
}

/// Workspace open should not synchronously spawn every saved live session.
///
/// 数据构造：flow_source = workspace/flow.rs 生产源码；blocked call =
/// `restore_live_sessions_with_node_sessions(&checkpoints, &node_sessions)`，这是 5 个
/// PTY/Agent 一次性同步 spawn 的旧卡顿点。
///
/// 执行过程：读取源码 → 搜索 deferred restore 调度入口 → 搜索旧同步全量 restore 调用。
///
/// 预期结果：存在 `spawn_deferred_live_session_restore`；不存在旧同步全量 restore 调用。
#[test]
fn test_workspace_open_defers_live_session_restore() {
    let flow = include_str!("../../src/workspace/flow.rs");
    let production_source = flow
        .split("\n#[cfg(test)]")
        .next()
        .expect("flow.rs should contain production source before tests");

    assert!(
        production_source.contains("spawn_deferred_live_session_restore"),
        "workspace open should schedule live session restore after Canvas is visible"
    );
    assert!(
        !production_source.contains("restore_live_sessions_with_node_sessions(&checkpoints, &node_sessions)"),
        "enter_workspace_for_path must not synchronously restore every live session before showing Canvas"
    );
}

/// Workspace 打开恢复 Agent 时，缺失持久化 session id 的历史发现必须在后台执行。
///
/// 数据构造（含关键数值的推导过程）：
///   production_source = workspace/flow.rs 的生产源码
///   blocking discovery = 1 次 Provider 历史目录扫描，可能遍历多个 JSONL 文件
///   foreground restore = 1 次 Runtime/GPUI 状态更新，只消费已发现的 session id
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 定位 `spawn_deferred_live_session_restore` 的生产实现
///   2. 检查实现通过 `background_spawn` 调用 profile-specific session discovery
///   3. 检查前台恢复入口消费 `discovered_session_id`，生成对应 resume 命令
///
/// 预期结果：
///   - 正断言：Provider 历史发现由 background_spawn 承载，不阻塞 GPUI 前台
///   - 正断言：发现结果传入 Runtime 单节点恢复入口
///   - 负断言：不得退回只调用无 discovery 参数的旧恢复入口
#[test]
fn test_workspace_open_discovers_missing_agent_session_off_foreground() {
    let flow = include_str!("../../src/workspace/flow.rs");
    let production_source = flow
        .split("\n#[cfg(test)]")
        .next()
        .expect("flow.rs should contain production source before tests");

    assert!(
        production_source.contains("cx.background_spawn(async move"),
        "Agent session 历史发现必须通过 background_spawn 离开 GPUI 前台线程"
    );
    assert!(
        production_source.contains("find_agent_session_id_for_profile"),
        "Workspace 恢复缺失 session id 时必须调用 profile-specific 历史发现"
    );
    assert!(
        production_source.contains("discovered_session_id.as_deref()"),
        "后台发现到的 session id 必须传入 Runtime 单节点恢复入口"
    );
    assert!(
        !production_source.contains("&pending.node_sessions,\n                        );"),
        "Workspace 延迟恢复不得省略 discovery 参数而退回全新 Session"
    );
}
