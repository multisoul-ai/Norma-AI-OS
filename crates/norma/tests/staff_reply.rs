use norma::protocol::{
    PlanItem, ReplyStatus, ReplyTrigger, StaffPart, StaffProposal, StaffQuestion, StepStatus,
    StepTitle, Task, TaskDisplayStatus, TaskMessage,
};

fn step(reply: &norma::protocol::StaffReply, id: &str) -> (StepTitle, StepStatus) {
    reply
        .parts
        .iter()
        .find_map(|part| match part {
            StaffPart::Step {
                id: found,
                title,
                status,
                ..
            } if found == id => Some((title.clone(), *status)),
            _ => None,
        })
        .expect("step must exist")
}

/// 场景：同一轮的流式文字合并成一段，被卡片隔开后才分段。
/// 数据构造：一条 Running 回复；依次追加「前」「半」、一张提议卡、「后」。
/// 执行过程：调用 push_text / push_card。
/// 预期结果：parts 为 [Text("前半"), Proposal(1), Text("后")]；不会出现相邻的两个 Text。
#[test]
fn streamed_text_merges_until_a_card_splits_it() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    reply.push_text("前");
    reply.push_text("半");
    reply.push_card(StaffPart::Proposal(1));
    reply.push_text("后");
    assert_eq!(
        reply.parts,
        vec![
            StaffPart::Text("前半".into()),
            StaffPart::Proposal(1),
            StaffPart::Text("后".into()),
        ],
        "同一段文字必须合并，卡片把文字切成两段"
    );
    assert_eq!(reply.status, ReplyStatus::Running, "新回复处于 Running");
}

/// 场景：同一张卡片重复挂载不产生重复引用。
/// 数据构造：对同一回复两次 push_card(Proposal(7))。
/// 预期结果：parts 里只有一个 Proposal(7)。
#[test]
fn the_same_card_is_attached_once() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    reply.push_card(StaffPart::Proposal(7));
    reply.push_card(StaffPart::Proposal(7));
    assert_eq!(reply.parts.len(), 1, "重复挂载同一卡片必须去重");
}

/// 场景：工具步骤按 id 更新，Denied 之后不会被后续事件改回。
/// 数据构造：步骤 a 先 Running，再 Completed 并带耗时；步骤 b 先 Denied，再收到 Completed。
/// 预期结果：a 为 Completed 且耗时保留；b 仍为 Denied。
#[test]
fn steps_update_by_id_and_denied_is_sticky() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    let title = StepTitle::Norma(vec!["brain".into(), "agents".into()]);
    reply.upsert_step("a", Some(title.clone()), Some(StepStatus::Running), None);
    reply.upsert_step("a", None, Some(StepStatus::Completed), Some(800));
    reply.upsert_step(
        "b",
        Some(StepTitle::Tool("Bash".into())),
        Some(StepStatus::Denied),
        None,
    );
    reply.upsert_step("b", None, Some(StepStatus::Completed), Some(5));
    assert_eq!(step(reply, "a"), (title, StepStatus::Completed), "a 应完成");
    assert_eq!(
        step(reply, "b").1,
        StepStatus::Denied,
        "被拒绝的步骤不能被后续完成事件覆盖"
    );
    assert_eq!(reply.step_count(), 2, "按 id 更新，不重复追加步骤");
}

/// 场景：停止或失败会把仍在运行的步骤标为中断，并固定耗时。
/// 数据构造：一个 Running 步骤与一个 Completed 步骤，以 Stopped 结束，耗时 1200ms。
/// 预期结果：Running 步骤变 Interrupted；Completed 步骤不变；回复状态 Stopped，耗时 1200。
#[test]
fn finishing_interrupts_running_steps() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    reply.upsert_step(
        "run",
        Some(StepTitle::Tool("Bash".into())),
        Some(StepStatus::Running),
        None,
    );
    reply.upsert_step(
        "ok",
        Some(StepTitle::Tool("Read".into())),
        Some(StepStatus::Completed),
        Some(3),
    );
    reply.finish(ReplyStatus::Stopped, 1200);
    assert_eq!(
        step(reply, "run").1,
        StepStatus::Interrupted,
        "运行中的步骤变中断"
    );
    assert_eq!(
        step(reply, "ok").1,
        StepStatus::Completed,
        "已完成的步骤不变"
    );
    assert_eq!(reply.status, ReplyStatus::Stopped, "状态记为已停止");
    assert_eq!(reply.elapsed_ms, 1200, "耗时固定");
}

/// 场景：Prompt 事件表示模型重写，要清掉本轮已流式的文字但保留步骤和卡片。
/// 数据构造：Text、Step、Proposal 各一个。
/// 预期结果：clear_text 后只剩 Step 与 Proposal。
#[test]
fn clear_text_keeps_steps_and_cards() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    reply.push_text("草稿");
    reply.upsert_step(
        "a",
        Some(StepTitle::Tool("Read".into())),
        Some(StepStatus::Running),
        None,
    );
    reply.push_card(StaffPart::Proposal(1));
    reply.clear_text();
    assert_eq!(reply.parts.len(), 2, "只剩步骤和卡片");
    assert!(reply.text().is_empty(), "文字被清空");
}

/// 场景：计划整体覆盖，不累积。
/// 数据构造：两次 set_plan，第二次条目更多。
/// 预期结果：parts 里只有一个 Plan，内容是第二次的。
#[test]
fn plan_is_replaced_not_accumulated() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    let item = |status: &str, text: &str| PlanItem {
        status: status.into(),
        text: text.into(),
    };
    reply.set_plan(vec![item("pending", "一")]);
    reply.set_plan(vec![item("completed", "一"), item("pending", "二")]);
    let plans: Vec<_> = reply
        .parts
        .iter()
        .filter(|part| matches!(part, StaffPart::Plan(_)))
        .collect();
    assert_eq!(plans.len(), 1, "只保留一个计划");
    assert!(
        matches!(plans[0], StaffPart::Plan(items) if items.len() == 2),
        "计划内容是最近一次"
    );
}

/// 场景：没有进行中的回复时挂卡片，另起一条已完成的事件回复，不丢卡片。
/// 数据构造：空任务直接 attach_card(Proposal(3))；随后开始一条 Running 回复再挂 Proposal(4)。
/// 预期结果：第一次产生 Done 且 trigger=Event 的回复；第二次挂进 Running 回复，不新增消息。
#[test]
fn attach_card_uses_the_running_reply_or_creates_one() {
    let mut task = Task::new("t", "w", "p", "x");
    task.attach_card(StaffPart::Proposal(3));
    assert_eq!(task.messages.len(), 1, "无进行中回复时新建一条");
    let TaskMessage::Staff(first) = &task.messages[0] else {
        panic!("staff reply")
    };
    assert_eq!(first.status, ReplyStatus::Done, "兜底回复直接完成");
    assert_eq!(first.trigger, ReplyTrigger::Event, "兜底回复不是用户触发");
    task.begin_reply(ReplyTrigger::User);
    task.attach_card(StaffPart::Proposal(4));
    assert_eq!(task.messages.len(), 2, "有进行中回复时不再新增消息");
}

/// 场景：崩溃恢复，加载时仍是 Running 的回复变 Failed，步骤变 Interrupted。
/// 数据构造：一条 Running 回复含 Running 步骤；一条 Done 回复。
/// 预期结果：recover_interrupted 返回 true；第一条 Failed；第二条不变；再次调用返回 false。
#[test]
fn recover_interrupted_fails_running_replies_once() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    reply.upsert_step(
        "a",
        Some(StepTitle::Tool("Bash".into())),
        Some(StepStatus::Running),
        None,
    );
    task.begin_reply(ReplyTrigger::User)
        .finish(ReplyStatus::Done, 10);
    assert!(task.recover_interrupted(), "有 Running 回复时应修复");
    let statuses: Vec<_> = task
        .messages
        .iter()
        .filter_map(|m| match m {
            TaskMessage::Staff(r) => Some(r.status),
            _ => None,
        })
        .collect();
    assert_eq!(
        statuses,
        vec![ReplyStatus::Failed, ReplyStatus::Done],
        "只改 Running 的"
    );
    assert!(!task.recover_interrupted(), "第二次没有可修复的");
}

/// 场景：问题在 Task.questions，待答才点亮“需要你”，回答后恢复。
/// 数据构造：Task.questions 放一个 Pending 问题；回合里放 Question(id) 引用。
/// 预期结果：Pending 为 NeedsYou；改成 Answered 后为 Waiting；过期函数把 Pending 变 TimedOut。
#[test]
fn pending_questions_live_in_task_questions() {
    use norma::protocol::StaffQuestionState;
    let mut task = Task::new("t", "w", "p", "x");
    task.begin_reply(ReplyTrigger::User)
        .push_card(StaffPart::Question("q".into()));
    task.questions.push(StaffQuestion {
        id: "q".into(),
        state: StaffQuestionState::Pending,
        ..StaffQuestion::default()
    });
    assert_eq!(task.pending_question_count(), 1, "一个待答问题");
    assert_eq!(
        task.display_status(),
        TaskDisplayStatus::NeedsYou,
        "待答需要你"
    );
    assert!(task.expire_pending_questions(), "过期应报告有变化");
    assert_eq!(
        task.questions[0].state,
        StaffQuestionState::TimedOut,
        "待答变超时"
    );
    assert_ne!(
        task.display_status(),
        TaskDisplayStatus::NeedsYou,
        "超时不再需要你"
    );
}

/// 场景：回答问题追加 Answered 且重新打开已完成任务，但不产生用户消息。
/// 数据构造：Done 阶段且已归档的任务，调用 answered_message("q")。
/// 预期结果：阶段回 Active、归档取消；消息依次为 Reopened、Answered；没有 User。
#[test]
fn answering_reopens_without_a_user_message() {
    let mut task = Task::new("t", "w", "p", "x");
    task.phase = norma::protocol::TaskPhase::Done;
    task.archived = true;
    task.answered_message("q");
    assert_eq!(
        task.phase,
        norma::protocol::TaskPhase::Active,
        "回答会重新打开任务"
    );
    assert!(!task.archived, "回答会取消归档");
    assert!(
        matches!(task.messages.as_slice(), [TaskMessage::Reopened, TaskMessage::Answered { question_id }] if question_id == "q"),
        "消息应为 Reopened + Answered"
    );
    assert!(
        !task
            .messages
            .iter()
            .any(|m| matches!(m, TaskMessage::User { .. })),
        "回答不能伪装成用户消息"
    );
}

/// 场景：新一轮完整新方案标记旧提议为已替换，保留回合内容，且不再要求用户采用。
/// 数据构造：第 1 轮提议 #1，第 2 轮提议 #2（均未采用）。
/// 执行过程：supersede_stale_proposals。
/// 预期结果：#1 superseded=true、#2 为 false；两张卡片引用仍在；#1 不再计入“需要你”。
#[test]
fn stale_proposals_are_marked_not_deleted() {
    let mut task = Task::new("t", "w", "p", "x");
    let proposal = |id, round| StaffProposal {
        id,
        agent: "Simon".into(),
        title: "t".into(),
        prompt: "p".into(),
        why: "w".into(),
        model: None,
        approved: false,
        ignored: false,
        dispatch_id: None,
        round,
        auto: false,
        downgrade: None,
        superseded: false,
    };
    task.round = 1;
    task.begin_reply(ReplyTrigger::User)
        .push_card(StaffPart::Proposal(1));
    task.proposals.push(proposal(1, 1));
    task.round = 2;
    task.begin_reply(ReplyTrigger::User)
        .push_card(StaffPart::Proposal(2));
    task.proposals.push(proposal(2, 2));
    task.supersede_stale_proposals();
    assert!(task.proposals[0].superseded, "旧提议被标记为已替换");
    assert!(!task.proposals[1].superseded, "本轮提议不受影响");
    assert_eq!(task.messages.len(), 2, "旧回合与卡片引用保留");
    task.proposals[1].ignored = true;
    assert_ne!(
        task.display_status(),
        TaskDisplayStatus::NeedsYou,
        "已替换的提议不再点亮需要你"
    );
}

/// 场景：步骤标题只保留 norma 子命令词，路径、参数和参数值不进快照。
/// 数据构造：带绝对路径和引号的 norma 命令、普通 shell 命令、只有工具名、什么都没有。
/// 预期结果：norma 命令得到 ["brain","agents"]；其余只留工具名；没有任何路径字符。
#[test]
fn step_titles_drop_paths_and_arguments() {
    use norma::protocol::step_title;
    let command =
        r#""/Users/openclawd/.norma/brain/bin/norma" brain agents --workspace w --route r"#;
    assert_eq!(
        step_title(Some(command), None),
        StepTitle::Norma(vec!["brain".into(), "agents".into()]),
        "只留命令词"
    );
    assert_eq!(
        step_title(
            Some("Bash"),
            Some("/x/norma brain task propose t1 --title a")
        ),
        StepTitle::Norma(vec!["brain".into(), "task".into(), "propose".into()]),
        "名称不是 norma 命令时看入参；最多三个命令词，任务 id 不进标题"
    );
    assert_eq!(
        step_title(Some("/bin/zsh -lc ls /Users/me"), None),
        StepTitle::Tool("zsh".into()),
        "普通命令只留可执行文件名"
    );
    assert_eq!(
        step_title(Some("skill"), None),
        StepTitle::Tool("skill".into()),
        "工具名原样保留"
    );
    assert_eq!(
        step_title(None, None),
        StepTitle::Tool("tool".into()),
        "什么都没有时用占位名"
    );
}

/// 场景：命令放在 rawInput 的 JSON 里（字符串或数组）时仍能识别 norma 子命令，且不泄露路径。
/// 数据构造：名称是占位的 Bash，入参分别是 {"command": "<带绝对路径和引号的 norma 命令>"} 与数组形式。
/// 执行过程：调用 step_title。
/// 预期结果：两种形式都得到 ["brain","agents"]；入参不是 JSON 时仍按原文本处理。
#[test]
fn step_titles_read_the_command_from_json_input() {
    use norma::protocol::step_title;
    let words = StepTitle::Norma(vec!["brain".into(), "agents".into()]);
    let string = r#"{"command":"\"/Users/me/.norma/brain/bin/norma\" brain agents --workspace w"}"#;
    assert_eq!(
        step_title(Some("Bash"), Some(string)),
        words,
        "字符串形式的命令"
    );
    let array = r#"{"command":["/Users/me/norma","brain","agents","--route","r"]}"#;
    assert_eq!(
        step_title(Some("Bash"), Some(array)),
        words,
        "数组形式的命令"
    );
    assert_eq!(
        step_title(Some("Bash"), Some(r#"{"path":"/etc/hosts"}"#)),
        StepTitle::Tool("Bash".into()),
        "没有命令字段时只留工具名"
    );
}
