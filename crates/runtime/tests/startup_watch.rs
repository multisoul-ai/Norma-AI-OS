use canvas::{AgentProfile, CanvasRect};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime};
use std::time::{Duration, Instant};
use terminal::TestPtyBackend;

const ALL_PROFILES: [AgentProfile; 10] = [
    AgentProfile::Claude,
    AgentProfile::Codex,
    AgentProfile::Cursor,
    AgentProfile::Grok,
    AgentProfile::OpenCode,
    AgentProfile::InfCode,
    AgentProfile::Kimi,
    AgentProfile::ZCode,
    AgentProfile::Pi,
    AgentProfile::Hermes,
];

fn bounds() -> CanvasRect {
    CanvasRect::new(0.0, 0.0, 640.0, 400.0)
}

/// 假 PTY 后端 + 已配置 hook 通道 + 全部 profile 都「已装 hook」的 Runtime。
fn armed_runtime() -> Runtime {
    let mut runtime = bare_runtime();
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
        token: "tok".to_string(),
        workspace_id: "w".to_string(),
        workspace_route_id: "r".to_string(),
        norma_socket: "/tmp/s.sock".to_string(),
    }));
    runtime.set_startup_watch_profiles(ALL_PROFILES);
    runtime
}

fn bare_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    // echo 后端只在被写入时回显；全程不写入 → PTY 静默。注意：PTY 是否输出不再是判据。
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(TestPtyBackend::echo(tx)))
    }));
    runtime
}

fn session_start(runtime: &mut Runtime, node_id: u64, profile: AgentProfile) {
    let outcome = runtime.apply_agent_task_update_with_outcome(AgentTaskUpdate {
        node_id,
        profile,
        provider_event: "SessionStart".to_string(),
        to: AgentTaskState::Ready,
        task_title: None,
        session_id: None,
    });
    assert!(
        outcome.task_state_changed,
        "前置：SessionStart 应被 Runtime 接受并改变任务状态"
    );
}

/// 已装 hook 的新 Agent 在 15s 内收不到任何 hook，则 stalled，且 tick 只在越界那一次返回 true。
///
/// 数据构造：
///   armed_runtime：Codex 在「已装 hook」集合内且 hook 通道已配置；spawn 时刻 ≈ now0，限值 15s
///   采样：now0+14s（未满）、now0+16s（已满）、now0+20s（再次）
///   推导：采样用 Instant::now()+偏移，Instant::now() 晚于 spawn，真实经过时长 ≥ 偏移；14s < 15s ≤ 16s。
///
/// 执行过程：spawn_agent(Codex) → tick(+14s) → tick(+16s) → tick(+20s)。
///
/// 预期结果：
///   - 14s：tick=false，stalled=false
///   - 16s：tick=true，stalled=true
///   - 20s：tick=false（不重复触发重绘），stalled 仍 true
#[test]
fn agent_without_hook_stalls_after_limit_and_ticks_once() {
    let mut runtime = armed_runtime();
    let node = runtime
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("工厂注入的 Codex 应能 spawn");
    let now0 = Instant::now();

    assert!(
        !runtime.tick_startup_stalls(now0 + Duration::from_secs(14)),
        "14s 小于 15s，不应触发"
    );
    assert!(
        !runtime.agent_startup_stalled(node),
        "14s 时不应显示 stalled"
    );
    assert!(
        runtime.tick_startup_stalls(now0 + Duration::from_secs(16)),
        "16s 越过 15s 仍无 hook，应返回 true 触发重绘"
    );
    assert!(
        runtime.agent_startup_stalled(node),
        "16s 时 Agent 应处于 stalled"
    );
    assert!(
        !runtime.tick_startup_stalls(now0 + Duration::from_secs(20)),
        "已 stalled 后不得重复返回 true"
    );
    assert!(
        runtime.agent_startup_stalled(node),
        "没有 hook 时 stalled 应保持"
    );
    runtime.kill_all();
}

/// 限值前收到 hook 则永不 stalled；stalled 后 hook 迟到则立即恢复。
///
/// 数据构造：
///   healthy：spawn 后立刻收到 SessionStart（模拟 CLI 正常启动），再 tick(+60s)
///   late：静默到 +16s 已 stalled，之后才收到 SessionStart
///
/// 预期结果：
///   - healthy：tick=false，stalled=false（负断言：健康 CLI 不出徽标）
///   - late：收到 hook 后 stalled=false（徽标撤销）
#[test]
fn hook_prevents_or_clears_stalled() {
    let mut runtime = armed_runtime();
    let healthy = runtime
        .spawn_agent(AgentProfile::Claude, bounds(), 80, 24)
        .expect("Claude 应能 spawn");
    session_start(&mut runtime, healthy, AgentProfile::Claude);
    let now0 = Instant::now();
    assert!(
        !runtime.tick_startup_stalls(now0 + Duration::from_secs(60)),
        "已收到 SessionStart 的 Agent 即使过 60s 也不应触发"
    );
    assert!(
        !runtime.agent_startup_stalled(healthy),
        "已收到 SessionStart 的 Agent 不得 stalled"
    );
    runtime.kill_all();

    let mut runtime = armed_runtime();
    let late = runtime
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("Codex 应能 spawn");
    let now1 = Instant::now();
    assert!(
        runtime.tick_startup_stalls(now1 + Duration::from_secs(16)),
        "前置：16s 无 hook 应已 stalled"
    );
    session_start(&mut runtime, late, AgentProfile::Codex);
    assert!(
        !runtime.agent_startup_stalled(late),
        "hook 到来后 stalled 必须立即撤销，不必等下一次 tick"
    );
    assert!(
        !runtime.tick_startup_stalls(now1 + Duration::from_secs(40)),
        "撤销后不得再次触发"
    );
    assert!(
        !runtime.agent_startup_stalled(late),
        "撤销后 stalled 必须保持 false"
    );
    runtime.kill_all();
}

/// PTY 有无输出不影响判据：这正是旧判据对真实 CLI 失效的原因。
///
/// 数据构造：Codex 卡片 spawn 后向 PTY 写入一次触发 echo 回显（模拟 TUI 已绘制「loading」首屏），
/// 但始终没有 hook。
///
/// 预期结果：+16s 仍 stalled（真实 Codex 联网失败时会画首屏但永不发 SessionStart）。
#[test]
fn pty_output_alone_does_not_settle_the_watch() {
    let mut runtime = armed_runtime();
    let node = runtime
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("Codex 应能 spawn");
    runtime
        .send_node_input(node, runtime::NodeInput::Text("loading".into()))
        .expect("应能向节点写入触发回显");
    runtime.drain_all();
    let now0 = Instant::now();

    assert!(
        runtime.tick_startup_stalls(now0 + Duration::from_secs(16)),
        "只有 PTY 输出、没有 hook：16s 后仍须判定 stalled"
    );
    assert!(
        runtime.agent_startup_stalled(node),
        "PTY 输出不得让看门狗 settled"
    );
    runtime.kill_all();
}

/// 只有「已装 hook 且 hook 通道已配置」的 Agent 才武装，否则恒不报警（避免对根本不会发 hook 的卡片误报）。
///
/// 数据构造（4 种情形，每种一张卡片，采样 +16s）：
///   - installed：Codex 在已装集合内且有 hook 通道 → 应武装
///   - not_installed：Claude 不在已装集合 → 不武装
///   - no_channel：另一个 Runtime，集合内含 Codex 但没有 hook 通道 → 不武装
///   - 空集合：新 Runtime 默认没有任何已装 profile → 不武装
///
/// 预期结果：只有 installed 为 stalled；其余三种 tick=false 且 stalled=false。
#[test]
fn only_hook_installed_agents_with_a_channel_are_armed() {
    let mut runtime = armed_runtime();
    runtime.set_startup_watch_profiles([AgentProfile::Codex]);
    let installed = runtime
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("Codex 应能 spawn");
    let not_installed = runtime
        .spawn_agent(AgentProfile::Claude, bounds(), 80, 24)
        .expect("Claude 应能 spawn");
    let now0 = Instant::now();
    assert!(
        runtime.tick_startup_stalls(now0 + Duration::from_secs(16)),
        "已装 hook 的 Codex 应触发"
    );
    assert!(
        runtime.agent_startup_stalled(installed),
        "已装 hook 的 Codex 无 hook 应 stalled"
    );
    assert!(
        !runtime.agent_startup_stalled(not_installed),
        "未装 hook 的 Claude 不得 stalled"
    );
    runtime.kill_all();

    let mut no_channel = bare_runtime();
    no_channel.set_startup_watch_profiles([AgentProfile::Codex]);
    let silent = no_channel
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("Codex 应能 spawn");
    assert!(
        !no_channel.tick_startup_stalls(Instant::now() + Duration::from_secs(16)),
        "没有 hook 通道时 hook 永远不会到，不得武装"
    );
    assert!(
        !no_channel.agent_startup_stalled(silent),
        "没有 hook 通道的 Agent 不得 stalled"
    );
    no_channel.kill_all();

    let mut empty = bare_runtime();
    empty.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
        token: "tok".to_string(),
        workspace_id: "w".to_string(),
        workspace_route_id: "r".to_string(),
        norma_socket: "/tmp/s.sock".to_string(),
    }));
    let unset = empty
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("Codex 应能 spawn");
    assert!(
        !empty.tick_startup_stalls(Instant::now() + Duration::from_secs(16)),
        "默认没有任何已装 profile，不得武装"
    );
    assert!(
        !empty.agent_startup_stalled(unset),
        "默认状态下 Agent 不得 stalled"
    );
    empty.kill_all();
}

/// 全部 10 个 Agent profile 经 spawn_agent 路径都能被武装并在无 hook 时 stalled。
///
/// 数据构造：10 个 profile 各一张卡片，全部在已装集合内，采样 +16s。
/// 预期结果：每个 profile 的节点都 stalled（逐一断言带 profile 名，防遗漏）。
#[test]
fn every_agent_profile_can_be_armed() {
    let mut runtime = armed_runtime();
    let nodes: Vec<_> = ALL_PROFILES
        .iter()
        .map(|profile| {
            let node = runtime
                .spawn_agent(*profile, bounds(), 80, 24)
                .unwrap_or_else(|e| panic!("{profile:?} 应能 spawn: {e}"));
            (*profile, node)
        })
        .collect();
    let now0 = Instant::now();

    assert!(
        runtime.tick_startup_stalls(now0 + Duration::from_secs(16)),
        "10 张无 hook 卡片越界时 tick 应返回 true"
    );
    for (profile, node) in nodes {
        assert!(
            runtime.agent_startup_stalled(node),
            "{profile:?} 卡片无 hook 16s 后应 stalled"
        );
    }
    runtime.kill_all();
}

/// 普通 Terminal 与不存在的节点都不报 stalled，且不触发重绘。
///
/// 数据构造：
///   - Terminal 一张（采样 +16s，已越过 15s 限值）
///   - node_id=9999（不存在）
///
/// 预期结果：tick=false；两者 stalled 均为 false。
#[test]
fn terminal_and_unknown_nodes_are_not_stalled() {
    let mut runtime = armed_runtime();
    let term = runtime
        .spawn_terminal(bounds(), runtime::DEFAULT_TERMINAL_SHELL, 80, 24)
        .expect("Terminal 应能 spawn");
    let now0 = Instant::now();

    assert!(
        !runtime.tick_startup_stalls(now0 + Duration::from_secs(16)),
        "Terminal 不参与看门狗，越界也不得要求重绘"
    );
    assert!(
        !runtime.agent_startup_stalled(term),
        "Terminal 节点不得报告 stalled"
    );
    assert!(
        !runtime.agent_startup_stalled(9999),
        "不存在的节点不得报告 stalled"
    );
    runtime.kill_all();
}

/// 多节点时 tick 返回值是各节点结果的「或」，而不是最后一个节点的结果。
///
/// 数据构造：1 张无 hook 的 Codex（会越界）+ 8 张已收到 SessionStart 的 Claude（已 settled）。
/// HashMap/Vec 遍历顺序不定；若实现误写成赋值，沉默节点不在最后时结果会被覆盖为 false。
///
/// 预期结果：tick=true；Codex stalled；8 张 Claude 均不 stalled。
#[test]
fn tick_reports_crossing_even_when_other_nodes_are_settled() {
    let mut runtime = armed_runtime();
    let silent = runtime
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("Codex 应能 spawn");
    let healthy: Vec<_> = (0..8)
        .map(|_| {
            let node = runtime
                .spawn_agent(AgentProfile::Claude, bounds(), 80, 24)
                .expect("Claude 应能 spawn");
            session_start(&mut runtime, node, AgentProfile::Claude);
            node
        })
        .collect();
    let now0 = Instant::now();

    assert!(
        runtime.tick_startup_stalls(now0 + Duration::from_secs(16)),
        "沉默节点刚越界，即使其余 8 个已 settled 也必须返回 true"
    );
    assert!(
        runtime.agent_startup_stalled(silent),
        "无 hook 的 Codex 应 stalled"
    );
    for (i, node) in healthy.into_iter().enumerate() {
        assert!(
            !runtime.agent_startup_stalled(node),
            "第 {i} 张已收到 SessionStart 的 Claude 不得 stalled"
        );
    }
    runtime.kill_all();
}

/// 进程已退出的 Agent 不是「网络慢」，不得显示启动信号徽标，也不再计时。
///
/// 数据构造：
///   工厂把 PTY 事件通道 tx 存出来；spawn Codex（已装 hook、未收到任何 hook）
///   随后向 tx 投递 Exited{code: Some(1)}，模拟命令缺失/认证失败后秒退
///   采样点 now0+16s（> 15s 限值）
///
/// 执行过程：spawn → 投递退出 → drain_all（会话状态变为 Exited）→ tick(+16s) → 查询 stalled。
///
/// 预期结果：
///   - tick=false：已退出节点的看门狗被丢弃，不触发重绘
///   - stalled=false：否则用户会在已崩溃的卡片上看到「可能是网络问题」
#[test]
fn exited_agent_never_reports_startup_stall() {
    // 机制：进程退出后 Runtime 会写入终态任务态（退出码 1 → Failed），看门狗视其为「已有信号」而 settled。
    // 此测试固化这一依赖；退出码 0 同样覆盖。
    for code in [Some(1), Some(0)] {
        assert_exit_does_not_stall(code);
    }
}

fn assert_exit_does_not_stall(code: Option<i32>) {
    use crossbeam_channel::Sender;
    use std::sync::{Arc, Mutex};
    let slot: Arc<Mutex<Option<Sender<terminal::PtyEvent>>>> = Arc::new(Mutex::new(None));
    let mut runtime = Runtime::new();
    let factory_slot = Arc::clone(&slot);
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        *factory_slot.lock().unwrap() = Some(tx.clone());
        Ok(Box::new(TestPtyBackend::echo(tx)))
    }));
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
        token: "tok".to_string(),
        workspace_id: "w".to_string(),
        workspace_route_id: "r".to_string(),
        norma_socket: "/tmp/s.sock".to_string(),
    }));
    runtime.set_startup_watch_profiles([AgentProfile::Codex]);
    let node = runtime
        .spawn_agent(AgentProfile::Codex, bounds(), 80, 24)
        .expect("工厂注入的 Codex 应能 spawn");
    let now0 = Instant::now();
    slot.lock()
        .unwrap()
        .as_ref()
        .expect("工厂应已捕获 tx")
        .send(terminal::PtyEvent::Exited { code })
        .expect("投递退出事件");
    runtime.drain_all();

    let crossed = runtime.tick_startup_stalls(now0 + Duration::from_secs(16));

    assert!(!crossed, "退出码 {code:?}：已退出节点越界不应触发重绘");
    assert!(
        !runtime.agent_startup_stalled(node),
        "退出码 {code:?}：已退出的 Agent 不得显示启动信号徽标"
    );
    runtime.kill_all();
}
