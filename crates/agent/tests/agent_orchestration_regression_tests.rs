use agent::{
    AgentAssignment, AgentCoordinationError, AgentDispatchSource, AgentRosterEntry,
    AgentTaskCoordinator, AgentTrackingMode,
};
use norma::protocol::AgentProfile;

fn roster(
    definition_id: i64,
    name: &str,
    profile: AgentProfile,
    tracking: AgentTrackingMode,
) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile,
        enabled: true,
        active_node_id: Some(definition_id as u64 + 100),
        tracking,
        busy: false,
        task_state: agent::AgentTaskState::Ready,
    }
}

/// 用户直接任务根：Hook 确认的用户任务必须成为首次 CLI 子委派的可信父任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent = Alan(id=1,node=101) + Bella(id=2,node=102)
///   direct roots = 1；delegated children = 1
///   task refs = root(1) + child(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 UserPromptSubmit 的可信信息为 Alan 注册直接根任务。
///   2. 以 Alan/node=101 为服务端来源向 Bella 派发一项任务。
///   3. 读取子任务的 parent/root 与目标定义。
///
/// 预期结果：
///   - 正断言：首次子委派成功，parent/root 都指向用户直接根任务。
///   - 负断言：不得因来源任务不是 Norma 内部派发而返回 source_agent_unavailable。
#[test]
fn direct_user_task_becomes_parent_for_first_cli_delegation() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "runtime-direct-root",
        vec![
            roster(1, "Alan", AgentProfile::Codex, AgentTrackingMode::Managed),
            roster(2, "Bella", AgentProfile::Claude, AgentTrackingMode::Managed),
        ],
    )
    .expect("合法双 Agent roster 应建立协调器");
    let (root, created) = coordinator
        .register_external_root(1, 101, "用户直接交给 Alan 的任务")
        .expect("可信 UserPromptSubmit 应建立 Alan 根任务");

    let child = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "检查子任务")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect("直接用户任务中的首次 CLI 委派必须成功");

    assert_eq!(child.len(), 1, "单目标 CLI 委派必须只返回一个子任务");
    assert!(created, "首次 UserPromptSubmit 必须新建直接根任务");
    assert_eq!(
        child[0].parent_task_ref.as_deref(),
        Some(root.task_ref.as_str()),
        "子任务 parent 必须由服务端绑定到 Alan 的直接根任务"
    );
    assert_eq!(
        child[0].root_task_ref, root.task_ref,
        "首次子任务 root 必须继承直接用户任务引用"
    );
    assert_eq!(
        child[0].definition_id, 2,
        "首次子委派必须精确路由到 Bella，而不是回到来源 Alan"
    );
    assert_ne!(
        child[0].definition_id, 1,
        "服务端不得把直接根任务误当作目标任务"
    );
}

/// 已完成中间父任务的祖先链：仍有后代运行时不得丢失通往活跃根任务的 lineage。
///
/// 数据构造（含关键数值的推导过程）：
///   chain = Alan(A,id=1) → Bella(B,id=2) → Cindy(C,id=3)
///   完成中间任务 = B(1)；仍活跃祖先/后代 = A(1) + C(1)
///   回边尝试 = C → A(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注册 A 的直接根任务，A 委派 B，B 再委派 C。
///   2. 将 B 标记为 succeeded，但保持 A 与 C 活跃。
///   3. 从 C 尝试再次委派 A，并沿保留的 B lineage 回溯。
///
/// 预期结果：
///   - 正断言：C→A 返回 DelegationCycle。
///   - 负断言：B 完成不得截断 A→B→C 祖先链并放行死锁回边。
#[test]
fn completed_middle_parent_keeps_lineage_until_descendant_finishes() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "runtime-lineage",
        vec![
            roster(1, "Alan", AgentProfile::Codex, AgentTrackingMode::Managed),
            roster(2, "Bella", AgentProfile::Claude, AgentTrackingMode::Managed),
            roster(3, "Cindy", AgentProfile::Cursor, AgentTrackingMode::Managed),
        ],
    )
    .expect("三 Agent roster 应建立协调器");
    let _ = coordinator
        .register_external_root(1, 101, "A root")
        .expect("A 应建立直接根任务");
    coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "B task")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect("A→B 应成功");
    coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Cindy", "C task")],
            false,
            AgentDispatchSource::agent(2, 102),
        )
        .expect("B→C 应成功");
    coordinator.finish_current(2);

    let cycle = coordinator.dispatch_from(
        vec![AgentAssignment::new("Alan", "cycle back")],
        false,
        AgentDispatchSource::agent(3, 103),
    );

    assert!(
        matches!(
            cycle,
            Err(AgentCoordinationError::DelegationCycle {
                target_definition_id: 1
            })
        ),
        "B 完成但 C 仍活跃时，C→A 必须沿保留 lineage 检出循环，实际：{cycle:?}"
    );
    assert!(
        coordinator
            .tasks()
            .iter()
            .any(|task| task.agent == "Alan" && task.state == "running"),
        "循环被拒绝后活跃根任务 Alan 必须仍然存在"
    );
    assert!(
        !coordinator
            .tasks()
            .iter()
            .any(|task| task.task == "cycle back"),
        "被拒绝的循环回边不得进入任何 Agent 队列"
    );
}

/// 提交路径失败历史：返回 Failed 的稳定 task_ref 必须继续可查询。
///
/// 数据构造（含关键数值的推导过程）：
///   managed tasks = active(1) + queued(1) = 2
///   rejected ids = 2；terminal state = failed
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 Alan 连续派发两项任务。
///   2. 模拟提交路径确认失败，按两个精确 ID 拒绝派发。
///   3. 分别用原 task_ref 查询协调器历史。
///
/// 预期结果：
///   - 正断言：两项任务均保留为 failed。
///   - 负断言：不得把 Failed 回执对应任务删除为 task_not_found。
#[test]
fn rejected_dispatch_retains_each_task_as_failed_history() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "runtime-failed-history",
        vec![roster(
            1,
            "Alan",
            AgentProfile::Codex,
            AgentTrackingMode::Managed,
        )],
    )
    .expect("单 Agent roster 应建立协调器");
    let receipts = coordinator
        .dispatch(
            vec![
                AgentAssignment::new("Alan", "active failure"),
                AgentAssignment::new("Alan", "queued failure"),
            ],
            false,
        )
        .expect("两项任务应形成 active + queued");
    let task_ids = receipts
        .iter()
        .map(|receipt| receipt.task_id)
        .collect::<Vec<_>>();

    assert!(
        coordinator.reject_dispatch(1, &task_ids),
        "精确提交失败应命中 Alan 的 active/queued 任务"
    );
    let active = coordinator
        .task_by_ref(&receipts[0].task_ref)
        .expect("active Failed 回执必须仍可查询");
    let queued = coordinator
        .task_by_ref(&receipts[1].task_ref)
        .expect("queued Failed 回执必须仍可查询");

    assert_eq!(active.state, "failed", "active 提交失败必须进入 failed");
    assert_eq!(queued.state, "failed", "queued 提交失败必须进入 failed");
    assert!(
        !coordinator.is_busy(1),
        "两项提交均确认失败后 Alan 不得继续占用协调器运行槽"
    );
    assert_ne!(
        active.state, "running",
        "Failed 回执不得在后续查询时回退为 running"
    );
    assert_ne!(
        queued.state, "queued",
        "Failed 回执不得在后续查询时仍显示 queued"
    );
}

/// 强制 Untracked 替换：旧任务必须保留明确终态，新任务不得覆盖其 task_ref。
///
/// 数据构造（含关键数值的推导过程）：
///   untracked capacity = 1 active
///   dispatches = first(1) + forced second(1) = 2
///   expected history = interrupted old(1) + running new(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向无 Hook 的 Alan 派发 first。
///   2. 以明确 force 再派发 second。
///   3. 分别按两个稳定 task_ref 查询状态。
///
/// 预期结果：
///   - 正断言：旧任务保留为 interrupted，新任务成为唯一 active。
///   - 负断言：旧 task_ref 不得被覆盖成 NotFound，也不得继续占用运行态。
#[test]
fn forced_untracked_replacement_preserves_superseded_task_history() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "runtime-untracked-force",
        vec![roster(
            1,
            "Alan",
            AgentProfile::Codex,
            AgentTrackingMode::Untracked,
        )],
    )
    .expect("单 Untracked Agent roster 应建立协调器");
    let first = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "first")], false)
        .expect("首项 Untracked 任务应被接收");
    let second = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "second")], true)
        .expect("用户明确 force 后第二项应被接收");

    let old = coordinator
        .task_by_ref(&first[0].task_ref)
        .expect("被替换的旧 task_ref 必须保留历史");
    let current = coordinator
        .task_by_ref(&second[0].task_ref)
        .expect("新 task_ref 必须成为当前任务");

    assert_eq!(
        old.state, "interrupted",
        "被明确覆盖的旧 Untracked 任务必须保留为 interrupted"
    );
    assert_eq!(
        current.state, "running",
        "force 后新 Untracked 任务必须成为唯一运行任务"
    );
    assert_ne!(
        old.task_ref, current.task_ref,
        "强制替换不得重用或覆盖旧任务引用"
    );
    assert_ne!(
        old.state, "running",
        "旧任务在新任务激活后不得继续显示 running"
    );
}

/// 取消终态映射：运行任务收到取消请求后，后续 Done Hook 也必须收口为 interrupted。
///
/// 数据构造（含关键数值的推导过程）：
///   active tasks       = 1
///   cancellation calls = 1
///   terminal hooks     = Done × 1
///   Total transitions  = running → cancel_requested → interrupted，共 2 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 managed Alan 派发一个运行任务。
///   2. 对该任务请求取消，协调器记录 cancel_requested。
///   3. 模拟 Provider 随后上报 Done，调用方先查询取消标记再决定终态。
///
/// 预期结果：
///   - 正断言：取消请求后协调器可查询到 cancel_requested。
///   - 正断言：最终任务状态为 interrupted。
///   - 负断言：取消后的 Done 不得被误记为 succeeded。
#[test]
fn cancel_requested_task_maps_late_done_hook_to_interrupted() {
    let mut coordinator = AgentTaskCoordinator::new(vec![roster(
        1,
        "Alan",
        AgentProfile::Codex,
        AgentTrackingMode::Managed,
    )])
    .expect("测试 roster 必须合法");
    let receipt = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "停止前的任务")], false)
        .expect("managed Agent 派发必须成功")
        .remove(0);

    assert_eq!(
        coordinator.cancel_task(receipt.task_id),
        agent::AgentTaskCancellation::CancelRequested,
        "运行任务取消必须先进入 cancel_requested"
    );
    assert!(
        coordinator.is_cancel_requested(1),
        "definition=1 的活动任务必须保留取消请求标记"
    );
    let terminal_state = if coordinator.is_cancel_requested(1) {
        "interrupted"
    } else {
        "succeeded"
    };
    coordinator.finish_current_with_state(1, terminal_state);
    let completed = coordinator
        .task_by_ref(&receipt.task_ref)
        .expect("终态任务必须仍可按 task_ref 查询");

    assert_eq!(
        completed.state, "interrupted",
        "取消请求后的 Done Hook 必须收口为 interrupted"
    );
    assert_ne!(
        completed.state, "succeeded",
        "取消请求后的 Done Hook 不得误报成功"
    );
}

/// 新的用户直接回合必须接替仍挂着的手动根任务，否则来源 Agent 没有可委派的 active。
///
/// 数据构造：
///   Alan definition_id=1，node=101。
///   第一条手动根 = "旧回合"；第二条 = "新回合"。
///   手动根的 source 是 Manual，且没有 parent。
///
/// 执行过程：
///   1. 连续两次 register_external_root。
///   2. 用第二次的根向 Bella 派发。
///
/// 预期结果：
///   - 正断言：第二次仍 created=true；旧回合状态为 succeeded；新回合为 running。
///   - 正断言：派发成功，parent 指向新回合。
///   - 负断言：旧回合不得仍是 running。
#[test]
fn new_user_prompt_replaces_manual_external_root() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "runtime-replace-manual-root",
        vec![
            roster(1, "Alan", AgentProfile::Claude, AgentTrackingMode::Managed),
            roster(2, "Bella", AgentProfile::Codex, AgentTrackingMode::Managed),
        ],
    )
    .expect("双 Agent roster 应建立协调器");
    let (first, first_created) = coordinator
        .register_external_root(1, 101, "旧回合")
        .expect("第一条用户回合应登记为根");
    let (second, second_created) = coordinator
        .register_external_root(1, 101, "新回合")
        .expect("下一条用户回合仍应能登记");
    let child = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "接着做")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect("新用户根必须能作为派发父任务");

    assert!(first_created, "第一条用户回合必须新建根任务");
    assert!(second_created, "已有手动根时，新的用户回合仍必须新建根任务");
    assert_eq!(
        task_state(&coordinator, &first.task_ref),
        "succeeded",
        "被接替的手动根必须以 succeeded 结束"
    );
    assert_eq!(
        task_state(&coordinator, &second.task_ref),
        "running",
        "新用户回合必须成为当前 active"
    );
    assert_ne!(
        task_state(&coordinator, &first.task_ref),
        "running",
        "旧手动根不得继续占据 active"
    );
    assert_eq!(
        child[0].parent_task_ref.as_deref(),
        Some(second.task_ref.as_str()),
        "派发父任务必须是新回合，而不是已经结束的旧根"
    );
}

/// Norma 派发或已有父任务的 active 不能被下一条用户 prompt 覆盖。
///
/// 数据构造：
///   Alan(id=1,node=101) 的 active 来自 Norma，parent 为空，source 不是 Manual。
///   Bella(id=2,node=102) 的 active 是 Alan 委派的子任务，带 parent。
///
/// 执行过程：
///   1. Norma 向 Alan 派发一项任务。
///   2. 对 Alan 再调用 register_external_root。
///   3. 给 Alan 登记手动根后委派 Bella，再对 Bella 调用 register_external_root。
///
/// 预期结果：
///   - 正断言：两次 register 都返回 created=false。
///   - 正断言：Norma 任务和 Bella 的子任务都保持 running。
///   - 负断言：这两项都不得变成 succeeded。
#[test]
fn user_prompt_does_not_replace_dispatched_active_task() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "runtime-keep-dispatched",
        vec![
            roster(1, "Alan", AgentProfile::Claude, AgentTrackingMode::Managed),
            roster(2, "Bella", AgentProfile::Codex, AgentTrackingMode::Managed),
        ],
    )
    .expect("双 Agent roster 应建立协调器");
    let norma = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Alan", "Norma 派发")],
            false,
            AgentDispatchSource::norma(),
        )
        .expect("Norma 直派应成功")
        .remove(0);
    let (replaced_norma, created_norma) = coordinator
        .register_external_root(1, 101, "用户回合")
        .expect("查询已有 Norma 任务不应失败");

    assert!(!created_norma, "Norma 派发的 active 不得被用户回合覆盖");
    assert_eq!(
        replaced_norma.task_ref, norma.task_ref,
        "拒绝覆盖时必须返回仍在运行的 Norma 任务"
    );
    assert_eq!(
        task_state(&coordinator, &norma.task_ref),
        "running",
        "Norma 任务必须继续 running"
    );

    coordinator.finish_current(1);
    let _ = coordinator
        .register_external_root(1, 101, "Alan 手动根")
        .expect("清掉 Norma 任务后应能登记手动根");
    let delegated = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "子任务")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect("手动根上的委派应成功")
        .remove(0);
    let (kept, created_child) = coordinator
        .register_external_root(2, 102, "Bella 的自动回合")
        .expect("查询 Bella 的委派任务不应失败");

    assert!(!created_child, "带 parent 的委派任务不得被用户回合覆盖");
    assert_eq!(
        kept.task_ref, delegated.task_ref,
        "拒绝覆盖时必须返回 Bella 正在执行的子任务"
    );
    assert_eq!(
        task_state(&coordinator, &delegated.task_ref),
        "running",
        "委派子任务必须继续 running"
    );
    assert_ne!(
        task_state(&coordinator, &delegated.task_ref),
        "succeeded",
        "委派子任务不得被标成 succeeded"
    );
    assert_ne!(
        task_state(&coordinator, &norma.task_ref),
        "running",
        "为腾出 Alan 而结束的 Norma 任务不得仍是 running"
    );
}

fn task_state(coordinator: &AgentTaskCoordinator, task_ref: &str) -> String {
    coordinator
        .task_by_ref(task_ref)
        .map(|task| task.state.to_string())
        .unwrap_or_else(|_| "missing".to_string())
}
