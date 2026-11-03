use agent::voice_command::{
    plan_voice_command, validate_voice_proposals, VoicePlan, VoiceProposal,
};
use agent::{AgentRosterEntry, AgentTaskState, AgentTrackingMode};
use norma::protocol::AgentProfile;
use serde_json::json;

fn bruno() -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: 1,
        name: "Bruno".into(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(11),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: AgentTaskState::Ready,
    }
}

/// 场景：普通点名问候不代表派工；续聊也不能在结束的会话上新建。
/// 构造：唯一 Bruno 以及它的 Ended 状态。
/// 执行：调用公开快速规划入口。
/// 预期：问候、称赞和 Ended 续聊都交语义路径，不生成写候选。
#[test]
fn fast_path_requires_an_explicit_action_and_valid_session() {
    for input in ["Bruno，你好", "Bruno，做得很好", "Bruno，辛苦了"] {
        assert_eq!(
            plan_voice_command(input, &[bruno()]),
            VoicePlan::NeedsModel,
            "普通对话不能点名即派发：{input}"
        );
    }
    let mut ended = bruno();
    ended.task_state = AgentTaskState::Ended;
    assert_eq!(
        plan_voice_command("Bruno，继续刚才的任务", &[ended]),
        VoicePlan::NeedsModel,
        "续聊缺少会话时不能转成新任务"
    );
}

/// 场景：同轮模型为同一任务产生两个不同改写，归一为原话后实际等价。
/// 构造：唯一 Bruno 与两个 assign 候选，正文分别为不同模型摘要。
/// 执行：调用公共整批校验接口。
/// 预期：规范候选只保留一份，避免不同 index 导致重复新建。
#[test]
fn normalized_proposals_submit_only_once() {
    let proposals = ["检查登录", "修复登录"].map(|task| VoiceProposal {
        tool: "assign_agent_task".into(),
        arguments: json!({"agent":"Bruno","task":task}),
    });
    let commits = validate_voice_proposals(
        "Bruno，新建任务，修复登录报错",
        &[bruno()],
        proposals.into(),
    )
    .expect("明确的新建请求应可验证");
    assert_eq!(commits.len(), 1, "替换为同一原话的候选必须去重，只提交一次");
}

/// 场景：画布外目标不能被在场成员替换，也不能无新建授权自动激活。
/// 构造：Bruno 在场、Alice 已知但未上画布；模型给出两个错误 assign。
/// 执行：整批公共校验，并对一条正常续聊在提交前变为 Ended 做重校验。
/// 预期：错目标、未授权激活、引用新任务均拒绝；失效续聊不能执行。
#[test]
fn target_scope_and_session_state_are_rechecked() {
    use agent::voice_command::{revalidate_voice_commit, VoiceFeedback};
    let mut alice = bruno();
    alice.definition_id = 2;
    alice.name = "Alice".into();
    alice.active_node_id = None;
    let roster = [bruno(), alice];
    for (input, target) in [
        ("Alice，新建任务，检查方案", "Bruno"),
        ("让 Alice 检查方案", "Alice"),
        ("Bruno，解释‘新任务’是什么意思", "Bruno"),
    ] {
        let result = validate_voice_proposals(
            input,
            &roster,
            vec![VoiceProposal {
                tool: "assign_agent_task".into(),
                arguments: json!({"agent":target,"task":input}),
            }],
        );
        assert!(
            result.is_err(),
            "不能越过目标、激活或引用语义限制：{input} -> {target}"
        );
    }
    let commit = validate_voice_proposals(
        "Bruno，继续刚才的任务",
        &[bruno()],
        vec![VoiceProposal {
            tool: "reply_agent".into(),
            arguments: json!({"agent":"Bruno","message":"继续"}),
        }],
    )
    .expect("有效续聊")
    .remove(0);
    let mut ended = bruno();
    ended.task_state = AgentTaskState::Ended;
    assert_eq!(
        revalidate_voice_commit(&commit, &[ended]),
        Err(VoiceFeedback::MissingSession),
        "同节点的会话已结束时也必须重新拒绝续聊"
    );
}

/// 场景：模型把画布上点名的任务转成后台大脑或其他工作区操作。
/// 构造：明确的 Bruno 请求和两个可解析、但越出目标作用域的候选。
/// 执行：公开整批校验。
/// 预期：不能通过未绑定目标的工具绕过点名校验。
#[test]
fn named_target_cannot_be_bypassed_with_another_write_tool() {
    for proposal in [
        VoiceProposal {
            tool: "handoff_to_brain".into(),
            arguments: json!({"request":"修复登录"}),
        },
        VoiceProposal {
            tool: "assign_workspace_task".into(),
            arguments: json!({"workspace_id":"other","area_id":"other","node_id":999,"task":"修复登录"}),
        },
    ] {
        assert!(
            validate_voice_proposals("Bruno，修复登录", &[bruno()], vec![proposal]).is_err(),
            "点名目标不能经其他写工具被替换或越界"
        );
    }
}

/// 场景：用户点名取消 Bruno 的任务，取消应绑定执行者而非退化为任意 task_id。
/// 构造：Bruno 唯一在场；对照没有取消授权的普通修复输入。
/// 执行：公开候选校验。
/// 预期：明确取消形成稳定目标信封；普通请求不能被模型改成取消。
#[test]
fn cancellation_keeps_the_named_target_and_requires_explicit_intent() {
    let proposal = VoiceProposal {
        tool: "cancel_agent_task".into(),
        arguments: json!({"task_id":7}),
    };
    let commits =
        validate_voice_proposals("Bruno，取消当前任务", &[bruno()], vec![proposal.clone()])
            .expect("点名取消应可形成待宿主核验的候选");
    assert_eq!(
        commits[0].binding.as_ref().map(|b| b.definition_id),
        Some(1),
        "取消也必须绑定明确执行者，宿主随后核验 task_id 归属"
    );
    assert!(
        validate_voice_proposals("Bruno，修复登录", &[bruno()], vec![proposal]).is_err(),
        "普通修复不能被模型改成取消"
    );
}

/// 场景：两项任务绑定不同节点，随后目录的首个同名节点发生变化。
/// 构造：成员定义不变，第一任务节点 11，排队任务节点 12，再同步回节点 11。
/// 执行：完成第一任务并取得下一项派发回执。
/// 预期：排队任务沿用创建时捕获的节点 12，不能被当前 roster 重定向。
#[test]
fn queued_task_keeps_its_captured_target_after_roster_changes() {
    let mut coordinator = agent::AgentTaskCoordinator::new(vec![bruno()]).expect("协调器");
    let first = coordinator
        .dispatch(vec![agent::AgentAssignment::new("Bruno", "第一项")], false)
        .expect("第一派发");
    let mut second = bruno();
    second.active_node_id = Some(12);
    coordinator.sync_roster(vec![second]).expect("第二节点");
    let queued = coordinator
        .dispatch(vec![agent::AgentAssignment::new("Bruno", "第二项")], false)
        .expect("排队派发");
    assert_eq!(first[0].node_id, 11, "第一项绑定节点 11");
    assert_eq!(queued[0].node_id, 12, "排队项在创建时必须绑定节点 12");
    coordinator
        .sync_roster(vec![bruno()])
        .expect("恢复默认目录");
    let next = coordinator.finish_current(1).expect("下一项");
    assert_eq!(next.node_id, 12, "排队任务不能被首个同名节点劫持");
    assert_ne!(next.node_id, 11, "不能回落到当前 roster 的首个节点");
}

/// 场景：节点 11 结束，而同定义节点 12 有已明确绑定的排队任务。
/// 构造：用公开协调器先派 11，再以 12 的目录快照排一项任务。
/// 执行：按节点结束 11。
/// 预期：仅 11 被中断；12 的任务保持原身份并继续派发，不能取消整份成员队列。
#[test]
fn ending_one_card_preserves_another_cards_queued_task() {
    let mut coordinator = agent::AgentTaskCoordinator::new(vec![bruno()]).expect("协调器");
    coordinator
        .dispatch(vec![agent::AgentAssignment::new("Bruno", "第一项")], false)
        .expect("第一项");
    let mut other = bruno();
    other.active_node_id = Some(12);
    coordinator.sync_roster(vec![other]).expect("第二卡片");
    coordinator
        .dispatch(vec![agent::AgentAssignment::new("Bruno", "第二项")], false)
        .expect("排队项");
    let (closed, next) = coordinator.finish_node_with_state(1, 11, "interrupted");
    assert_eq!(closed, 1, "只能中断已经结束的第一节点任务");
    assert_eq!(
        next.as_ref().map(|receipt| receipt.node_id),
        Some(12),
        "第二卡片的排队任务应继续，不得随第一卡片中断"
    );
    let tasks = coordinator.tasks();
    assert!(
        tasks
            .iter()
            .any(|task| task.target_node_id == 11 && task.state == "interrupted"),
        "第一节点有明确中断回执"
    );
    assert!(
        tasks
            .iter()
            .any(|task| task.target_node_id == 12 && task.state == "running"),
        "第二节点仍有自己的活跃任务"
    );
}
