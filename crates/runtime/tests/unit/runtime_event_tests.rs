use super::*;
use crate::live::{NodeInput, NodeResize, NodeScroll};
use crate::{AgentTaskState, NodeRunState, RuntimeEvent};
use canvas::{geometry::CanvasRect, AgentProfile, NodeType};
use std::time::{Duration, Instant};
use terminal::PtyCommand;

fn test_bounds() -> CanvasRect {
    CanvasRect::new(0.0, 0.0, 640.0, 400.0)
}

fn wait_until_state(
    runtime: &mut Runtime,
    node_id: NodeId,
    expected: impl Fn(&NodeRunState) -> bool,
) -> Option<NodeRunState> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        runtime.drain_all();
        if let Some(state) = runtime.node_run_state(node_id) {
            if expected(&state) {
                return Some(state);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    runtime.node_run_state(node_id)
}

/// spawn_terminal 成功后写入 Running 状态，并发出 NodeRegistered 事件。
///
/// 数据构造（含关键数值推导）：
///   bounds = 640×400，只用于创建画布节点
///   shell = "/bin/sh"，cols=80，rows=24
///   document.next_id 初始为 1，因此 node_id=1
///   事件队列初始为空，spawn 成功后应新增 1 条注册事件
///
/// 执行过程：
///   1. Runtime::new() → node_states/events 为空
///   2. spawn_terminal(...) → document.add_node(Term) + live.insert + state=Running
///   3. node_run_state(node_id) 读取当前状态
///   4. take_events() 消费注册事件
///
/// 预期结果：
///   - 正断言：node_run_state(node_id) == Running
///   - 正断言：事件为 NodeRegistered{node_type:Term,state:Running}
///   - 负断言：spawn 后不应产生 NodeRunStateChanged，pending_event_len 归零
#[test]
fn test_spawn_terminal_records_running_state_and_registered_event() {
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_terminal(test_bounds(), "/bin/sh", 80, 24)
        .expect("spawn_terminal 应成功以验证注册事件");

    assert_eq!(
        runtime.node_run_state(node_id),
        Some(NodeRunState::Running),
        "spawn_terminal 成功后节点运行状态应为 Running"
    );

    let events = runtime.take_events();
    assert_eq!(
        events.len(),
        1,
        "spawn_terminal 应只产生 1 条 NodeRegistered 事件"
    );
    assert_eq!(
        events[0],
        RuntimeEvent::NodeRegistered {
            node_id,
            node_type: NodeType::Term,
            state: NodeRunState::Running,
        },
        "注册事件应记录 node_id、Term 类型和 Running 初始状态"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::NodeRunStateChanged { .. })),
        "初始 Running 应通过 NodeRegistered 表达，不应伪造成状态变化"
    );
    assert_eq!(
        runtime.pending_event_len(),
        0,
        "take_events 后 pending_event_len 应归零，避免重复消费"
    );

    runtime.kill_all();
}

/// exit 0 经 drain_all 转为 Finished，且重复 drain 不重复发状态事件。
///
/// 数据构造（含关键数值推导）：
///   command = /bin/sh -lc "exit 0"
///   node_id 初始为 1
///   初始注册事件先 take_events 清空，因此后续只观察退出状态事件
///   最多等待 3 秒，每 20ms drain 一次，最多约 150 次
///
/// 执行过程：
///   1. spawn_agent_with_command(exit 0) → state=Running + NodeRegistered
///   2. take_events() 清空注册事件
///   3. 循环 drain_all() 直到 TerminalStatus::Exited{0} 映射为 Finished
///   4. take_events() 读取 Running→Finished
///   5. 再次 drain_all() 并 take_events()
///
/// 预期结果：
///   - 正断言：最终状态为 Finished{Some(0)}
///   - 正断言：产生 1 条 Running→Finished 和 1 条 Agent→Ended 状态变化事件
///   - 负断言：重复 drain 后不应再产生 NodeRunStateChanged
#[test]
fn test_exit_zero_transitions_to_finished_once() {
    let mut runtime = Runtime::new();
    let command = PtyCommand::new("/bin/sh").args(["-lc", "exit 0"]);
    let node_id = runtime
        .spawn_agent_with_command(AgentProfile::Claude, test_bounds(), command, 80, 24)
        .expect("spawn_agent_with_command 应成功以验证 exit 0 状态");
    let registered = runtime.take_events();
    assert_eq!(registered.len(), 1, "测试开始前应只清空 1 条注册事件");

    let final_state = wait_until_state(&mut runtime, node_id, |state| {
        matches!(state, NodeRunState::Finished { code: Some(0) })
    });

    assert_eq!(
        final_state,
        Some(NodeRunState::Finished { code: Some(0) }),
        "exit 0 应映射为 Finished{{code:Some(0)}}"
    );
    let events = runtime.take_events();
    assert_eq!(
        events.len(),
        2,
        "exit 0 应同步发布进程和 Agent 的两条状态变化事件"
    );
    assert_eq!(
        events[0],
        RuntimeEvent::NodeRunStateChanged {
            node_id,
            from: NodeRunState::Running,
            to: NodeRunState::Finished { code: Some(0) },
        },
        "状态变化应精确表达 Running→Finished(Some(0))"
    );
    assert!(
        matches!(&events[1], RuntimeEvent::AgentTaskStateChanged { node_id: id, to: AgentTaskState::Ended, .. } if *id == node_id),
        "正常退出的 Agent 应发布 Ended，而非继续就绪"
    );

    runtime.drain_all();
    let repeated = runtime.take_events();
    assert!(
        repeated.is_empty(),
        "重复 drain 不应再次产生状态变化事件，实际: {:?}",
        repeated
    );

    runtime.kill_all();
}

/// exit 2 经 drain_all 转为 Error，错误消息保留退出码。
///
/// 数据构造（含关键数值推导）：
///   command = /bin/sh -lc "exit 2"
///   退出码 2 非零，因此按计划映射为 Error 而非 Finished
///   注册事件先 take_events 清空，后续只观察 Running→Error
///
/// 执行过程：
///   1. spawn_agent_with_command(exit 2) → state=Running
///   2. 循环 drain_all() 直到 node_run_state 为 Error
///   3. take_events() 读取状态变化事件
///
/// 预期结果：
///   - 正断言：最终状态是 Error 且 message 包含 "2"
///   - 正断言：进程事件 from=Running，to=Error；Agent 同步发布 Failed
///   - 负断言：非零退出不应映射为 Finished
#[test]
fn test_non_zero_exit_transitions_to_error() {
    let mut runtime = Runtime::new();
    let command = PtyCommand::new("/bin/sh").args(["-lc", "exit 2"]);
    let node_id = runtime
        .spawn_agent_with_command(AgentProfile::Codex, test_bounds(), command, 80, 24)
        .expect("spawn_agent_with_command 应成功以验证 exit 2 状态");
    let registered = runtime.take_events();
    assert_eq!(registered.len(), 1, "测试开始前应只清空 1 条注册事件");

    let final_state = wait_until_state(&mut runtime, node_id, |state| {
        matches!(state, NodeRunState::Error { .. })
    });

    let Some(NodeRunState::Error { message }) = final_state else {
        panic!("exit 2 应映射为 Error 状态，实际: {:?}", final_state);
    };
    assert!(
        message.contains('2'),
        "Error message 应包含退出码 2，实际: {message}"
    );
    assert_ne!(
        runtime.node_run_state(node_id),
        Some(NodeRunState::Finished { code: Some(2) }),
        "非零退出码不应映射为 Finished"
    );

    let events = runtime.take_events();
    assert_eq!(
        events.len(),
        2,
        "exit 2 应同步发布进程和 Agent 的两条状态变化事件"
    );
    assert!(
        matches!(
            &events[0],
            RuntimeEvent::NodeRunStateChanged {
                node_id: event_node_id,
                from: NodeRunState::Running,
                to: NodeRunState::Error { message: event_message },
            } if *event_node_id == node_id && event_message.contains('2')
        ),
        "状态变化事件应精确表达 Running→Error(message contains 2)，实际: {:?}",
        events
    );
    assert!(
        matches!(&events[1], RuntimeEvent::AgentTaskStateChanged { node_id: id, to: AgentTaskState::Failed { message }, .. } if *id == node_id && message.contains('2')),
        "异常退出的 Agent 应保留退出码并发布 Failed"
    );

    runtime.kill_all();
}

/// remove_node 删除 live node 时发 NodeRemoved，并清空可查询运行状态。
///
/// 数据构造（含关键数值推导）：
///   spawn_terminal 创建 node_id=1，初始 state=Running
///   take_events 先清空 NodeRegistered
///   remove_node(node_id) 后事件队列应只新增 1 条 NodeRemoved
///
/// 执行过程：
///   1. spawn_terminal(...) → state=Running
///   2. take_events() 清空注册事件
///   3. remove_node(node_id) → live.remove + node_states.remove
///   4. 查询 node_run_state 与 take_events
///
/// 预期结果：
///   - 正断言：remove_node 返回 true，事件 previous_state=Running
///   - 正断言：node_run_state(node_id) 返回 None
///   - 负断言：删除后不应还能查询到 Running
#[test]
fn test_remove_node_emits_removed_event_and_clears_state() {
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_terminal(test_bounds(), "/bin/sh", 80, 24)
        .expect("spawn_terminal 应成功以验证 remove 事件");
    let registered = runtime.take_events();
    assert_eq!(registered.len(), 1, "测试开始前应只清空 1 条注册事件");

    let removed = runtime.remove_node(node_id);

    assert!(removed, "remove_node 应返回 true，表示节点确实被删除");
    assert_eq!(
        runtime.node_run_state(node_id),
        None,
        "remove_node 后 node_run_state 应返回 None"
    );
    assert_ne!(
        runtime.node_run_state(node_id),
        Some(NodeRunState::Running),
        "删除后不应还能查询到 Running 状态"
    );
    let events = runtime.take_events();
    assert_eq!(
        events,
        vec![RuntimeEvent::NodeRemoved {
            node_id,
            previous_state: NodeRunState::Running,
        }],
        "remove_node 应记录删除前的 Running 状态"
    );
}

/// 缺失节点操作失败时发 NodeOperationFailed，不改写已有 live node 状态。
///
/// 数据构造（含关键数值推导）：
///   valid node_id=1，missing node_id=999
///   resize 目标 100×30，只用于触发缺失节点分支
///   scroll 行数 1，只用于触发缺失节点分支
///
/// 执行过程：
///   1. spawn_terminal(...) → valid state=Running
///   2. 清空注册事件
///   3. send_node_input(999)、resize_node(999)、scroll_node(999) 分别返回 Err
///   4. 查询 valid node 的状态与失败事件
///
/// 预期结果：
///   - 正断言：产生 input/resize/scroll 三条失败事件
///   - 正断言：valid node 状态仍为 Running
///   - 负断言：失败操作不应产生 NodeRunStateChanged
#[test]
fn test_missing_node_operations_emit_failed_events_without_state_change() {
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_terminal(test_bounds(), "/bin/sh", 80, 24)
        .expect("spawn_terminal 应成功以验证失败事件");
    let registered = runtime.take_events();
    assert_eq!(registered.len(), 1, "测试开始前应只清空 1 条注册事件");

    let input_result = runtime.send_node_input(999, NodeInput::Text("noop".to_string()));
    let resize_result = runtime.resize_node(
        999,
        NodeResize::TermGrid {
            cols: 100,
            rows: 30,
        },
    );
    let scroll_result = runtime.scroll_node(999, NodeScroll::TermLines(1));

    assert!(
        input_result.is_err(),
        "send_node_input(999) 应返回错误以触发失败事件"
    );
    assert!(
        resize_result.is_err(),
        "resize_node(999) 应返回错误以触发失败事件"
    );
    assert!(
        scroll_result.is_err(),
        "scroll_node(999) 应返回错误以触发失败事件"
    );
    assert_eq!(
        runtime.node_run_state(node_id),
        Some(NodeRunState::Running),
        "失败的缺失节点操作不应改写真实 live node 的 Running 状态"
    );

    let events = runtime.take_events();
    assert_eq!(events.len(), 3, "三次失败操作应产生三条失败事件");
    assert!(
        matches!(&events[0], RuntimeEvent::NodeOperationFailed { node_id: Some(999), operation, message } if operation == "input" && message.contains("missing")),
        "第一条失败事件应记录 input missing node，实际: {:?}",
        events
    );
    assert!(
        matches!(&events[1], RuntimeEvent::NodeOperationFailed { node_id: Some(999), operation, message } if operation == "resize" && message.contains("missing")),
        "第二条失败事件应记录 resize missing node，实际: {:?}",
        events
    );
    assert!(
        matches!(&events[2], RuntimeEvent::NodeOperationFailed { node_id: Some(999), operation, message } if operation == "scroll" && message.contains("missing")),
        "第三条失败事件应记录 scroll missing node，实际: {:?}",
        events
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::NodeRunStateChanged { .. })),
        "失败操作不应伪造成运行状态变化，实际: {:?}",
        events
    );

    runtime.kill_all();
}
