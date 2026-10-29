use super::*;
use crate::agent_hooks::AgentHookRuntimeConfig;
use std::time::{Duration, Instant};

/// 不经 `spawn_agent` 入口创建的 Agent（恢复会话 / 手机端直启的等价路径）不被武装。
///
/// 数据构造：
///   - Runtime 已配置 hook 通道，且 Codex 在「已装 hook」集合内（满足武装的其他全部条件）
///   - 用测试辅助 `spawn_agent_with_command` 创建 Codex（与恢复、手机路径一样直接
///     `register_agent_live_node`，不经 `spawn_agent_with_identity_at`）
///   - 采样 +16s（限值 15s）
///
/// 执行过程：
///   1. spawn_agent_with_command(Codex, /bin/cat) → 节点存在，但未武装
///   2. tick(+16s) → 无被监视节点，返回 false
///
/// 预期结果：
///   - tick 为 false：恢复的 Codex 在下一次 prompt 前本来就不会发 SessionStart，不得误报
///   - agent_startup_stalled 为 false
#[test]
fn agent_registered_outside_spawn_agent_is_not_armed() {
    let mut runtime = Runtime::new();
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
        token: "tok".to_string(),
        workspace_id: "w".to_string(),
        workspace_route_id: "r".to_string(),
        norma_socket: "/tmp/s.sock".to_string(),
    }));
    runtime.set_startup_watch_profiles([AgentProfile::Codex]);
    let node = runtime
        .spawn_agent_with_command(
            AgentProfile::Codex,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            PtyCommand::new("/bin/cat"),
            80,
            24,
        )
        .expect("命令式 Agent 应能 spawn");

    let crossed = runtime.tick_startup_stalls(Instant::now() + Duration::from_secs(16));

    assert!(!crossed, "未经新建入口武装的 Agent 越界也不得要求重绘");
    assert!(
        !runtime.agent_startup_stalled(node),
        "未经新建入口武装的 Agent 不得报告 stalled"
    );
    runtime.kill_all();
}
