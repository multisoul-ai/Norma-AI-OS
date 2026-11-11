use super::*;
use norma::mobile::{
    ApprovalSnapshot, ApprovalTarget, RunOverview, RunOverviewAgent, RunOverviewAttention,
    RunOverviewAttentionKind, RunOverviewPresence,
};
use runtime::island::roster::{AgentActivity, SlotKind};

fn agent(name: &str, node: u64, presence: RunOverviewPresence, raw: &str) -> RunOverviewAgent {
    RunOverviewAgent {
        area_id: "area".into(),
        project: "Norma".into(),
        current: true,
        node_id: node,
        name: name.into(),
        profile: "Claude".into(),
        raw_state: raw.into(),
        presence,
        approval_count: 0,
        task: Some(format!("{name} task")),
        latest_prompt: None,
        action: Some("step".into()),
        last_tool: None,
        progress: None,
        started_at_unix_ms: None,
    }
}

fn approval(node: u64, title: &str) -> ApprovalSnapshot {
    ApprovalSnapshot {
        target: ApprovalTarget {
            node_id: node,
            profile: "Claude".into(),
            run_id: "run".into(),
            session_id: "session".into(),
            turn_id: "turn".into(),
            request_id: format!("req-{node}"),
        },
        card_title: title.into(),
        tool: "bash".into(),
        details: serde_json::json!({}),
        decisions: vec![],
    }
}

fn review(node: u64, agent: &str, turn: &str) -> RunOverviewAttention {
    RunOverviewAttention {
        kind: RunOverviewAttentionKind::Review,
        area_id: "area".into(),
        node_id: node,
        project: "Norma".into(),
        agent: agent.into(),
        title: "result".into(),
        run_id: "run".into(),
        turn_id: turn.into(),
        request_id: None,
        fingerprint: Some("fp".into()),
        paths: vec!["a.rs".into(), "b.rs".into()],
        loaded: true,
    }
}

fn workspace(
    running: Vec<RunOverviewAgent>,
    idle: Vec<RunOverviewAgent>,
    attention: Vec<RunOverviewAttention>,
    approvals: Vec<ApprovalSnapshot>,
) -> IslandWorkspace {
    IslandWorkspace {
        database: "db".into(),
        id: "ws".into(),
        name: "Norma-AI-OS".into(),
        projects: vec![super::super::IslandProject {
            source: 7,
            area_id: "area".into(),
            name: "App".into(),
            root: "/app".into(),
            loaded: true,
            overview: RunOverview {
                attention,
                running,
                idle,
                unloaded_projects: vec![],
            },
            cached_overview: RunOverview {
                attention: vec![],
                running: vec![],
                idle: vec![],
                unloaded_projects: vec![],
            },
            approvals,
            questions: vec![],
            proposals: vec![],
        }],
    }
}

/// 场景：一个人不能同时占左右槽。完成的成果留在右侧，排队的人不占槽。
/// 数据：Grace 在跑，Iris 在跑但有审批，Leo 已完成且有成果，Daisy 闲着。
/// 执行：collect 再按槽位归类。
/// 预期：Grace 在左；Iris 只在右且表情是等你；Leo 在右且表情是完成；Daisy 隐藏并排队。
#[test]
fn pending_takes_the_person_off_the_running_side() {
    let faces = collect(&[workspace(
        vec![
            agent("Grace", 1, RunOverviewPresence::Running, "thinking"),
            agent("Iris", 2, RunOverviewPresence::Running, "thinking"),
        ],
        vec![
            agent("Leo", 3, RunOverviewPresence::Idle, "done"),
            agent("Daisy", 4, RunOverviewPresence::Idle, "idle"),
        ],
        vec![review(3, "Leo", "turn-leo")],
        vec![approval(2, "Iris")],
    )]);
    let face = |name: &str| faces.iter().find(|face| face.name == name).expect(name);
    assert_eq!(face("Grace").slot, SlotKind::Running, "运行的人在左槽");
    assert_eq!(
        face("Grace").activity,
        AgentActivity::Running,
        "运行用工作表情"
    );
    assert_eq!(face("Iris").slot, SlotKind::Pending, "有审批的人不进左槽");
    assert_eq!(face("Iris").activity, AgentActivity::Waiting, "审批是等你");
    assert_ne!(
        face("Iris").slot,
        SlotKind::Running,
        "同一个人不能出现在两侧"
    );
    assert_eq!(face("Leo").slot, SlotKind::Pending, "待查看成果在右槽");
    assert_eq!(
        face("Leo").activity,
        AgentActivity::Done,
        "完成的成果保持完成表情"
    );
    assert_eq!(face("Leo").files, 2, "成果文件数来自路径");
    assert_eq!(face("Daisy").slot, SlotKind::Hidden, "排队的人不占收起槽");
    assert_eq!(
        face("Daisy").activity,
        AgentActivity::Queued,
        "排队单独成一种活动"
    );
    assert_eq!(faces.len(), 4, "四个人各一张脸，成果不额外造人");
}

fn file_row(node: u64, agent: &str, turn: &str, path: &str) -> RunOverviewAttention {
    let mut row = review(node, agent, turn);
    row.kind = RunOverviewAttentionKind::File;
    row.title = path.into();
    row.paths = vec![path.into()];
    row
}

/// 场景：同一回合的成果和它列出的文件是一张待处理卡。
/// 数据：Iris 一条审批。Leo 的 turn-leo 有一条带 3 条路径的成果，以及这 3 个文件各一行。
/// Noah 的另一回合只有一个文件，没有成果行。
/// 执行：queue_items，并用来历数页签件数。
/// 预期：Iris、Leo、Noah 三张卡。Leo 只出现一次。页签件数与队列相同，是 3 不是 6。
#[test]
fn one_turn_of_files_is_one_pending_card() {
    let paths = ["relay.mjs", "retry.mjs", "relay.test.mjs"];
    let mut attention = vec![review(4, "Leo", "turn-leo")];
    attention[0].paths = paths.iter().map(|path| (*path).to_string()).collect();
    for path in paths {
        attention.push(file_row(4, "Leo", "turn-leo", path));
    }
    attention.push(file_row(9, "Noah", "turn-other", "notes.md"));
    let space = workspace(vec![], vec![], attention, vec![approval(2, "Iris")]);
    let items = queue_items(std::slice::from_ref(&space));
    let names: Vec<_> = items.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Iris", "Leo", "Noah"],
        "同一回合的三个文件不能再各算一件：{names:?}"
    );
    assert!(
        items.iter().any(|item| item.name == "Leo" && item.review),
        "Leo 保留成果卡"
    );
    assert_eq!(
        items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        items.len(),
        "队列里不能有重复 id"
    );
    assert_eq!(
        super::super::controller::counts(&[space]).1,
        3,
        "页签件数必须和队列一样，是 3 不是 6"
    );
}

/// 场景：完成的人还留在概览的运行列表里，但成果已经把他标成完成。
/// 数据：Grace 仍在跑。Leo 的 raw_state 是 done，并且有一条成果。
/// 执行：用生产计数算运行页签。
/// 预期：页签是 1，不是运行列表的长度 2。筛选「运行中」与页签同一口径。
#[test]
fn running_tab_counts_faces_still_running_not_the_overview_list() {
    let grace = agent("Grace", 1, RunOverviewPresence::Running, "running");
    let leo = agent("Leo", 4, RunOverviewPresence::Running, "done");
    let space = workspace(
        vec![grace, leo],
        vec![],
        vec![review(4, "Leo", "turn-leo")],
        vec![],
    );
    let faces = collect(std::slice::from_ref(&space));
    let running_faces = faces
        .iter()
        .filter(|face| face.activity == AgentActivity::Running)
        .count();
    assert_eq!(running_faces, 1, "Leo 已完成，脸上不再是运行中");
    assert_eq!(
        super::super::controller::counts(&[space]),
        (1, 1),
        "运行页签跟脸的状态走，待处理仍是 Leo 这一张成果"
    );
}

/// 场景：审批处理完，队列里下一个是成果，而不是跳到更后面的提问。
/// 数据：顺序 a、成果 r、提问 q。a 被拿走。
/// 执行：queue_step。
/// 预期：按住成果 r；a 还在时停在 a；全部消失则没有下一张。
#[test]
fn the_queue_advances_into_a_review_before_the_later_alert() {
    let review = QueueItem {
        id: "r".into(),
        name: "Leo".into(),
        provider: "OpenCode".into(),
        workspace: "norma-cloud".into(),
        review: true,
        proposal: false,
    };
    let alert = |id: &str| QueueItem {
        id: id.into(),
        name: id.into(),
        provider: "Claude".into(),
        workspace: "Norma".into(),
        review: false,
        proposal: false,
    };
    let previous = ["a".to_owned(), "r".to_owned(), "q".to_owned()];
    let remaining = vec![review, alert("q")];
    let step = queue_step(&previous, &remaining, Some("a"));
    assert_eq!(step.focused_review.as_deref(), Some("r"), "下一张是成果");
    assert!(step.hold_review, "成果要按住提醒形态");
    assert_eq!(step.select_alert, None, "成果不是审批 id");
    let stay = queue_step(&previous, &remaining, Some("r"));
    assert_eq!(
        stay.focused_review.as_deref(),
        Some("r"),
        "还在的成果保持不动"
    );
    let gone = queue_step(&previous, &[], Some("a"));
    assert_eq!(gone.focused_review, None, "空队列没有下一张");
    assert!(!gone.hold_review, "空队列松开按住");
}

/// 场景：两个项目各有一张节点号同为 42 的卡片，只有其中一个刚完成。
/// 数据：同一宿主来源 7。area-a 的 Ada 仍在运行，area-b 的 Bea 也在运行。完成身份是 7:area-b:42。
/// 执行：收集脸，再用完成身份匹配。
/// 预期：命中 Bea；只用来源和节点号的 7:42 匹配不到任何人，也不能落到 Ada。
#[test]
fn completion_face_keeps_the_project_when_node_ids_collide() {
    let project = |area: &str, name: &str| {
        let mut person = agent(name, 42, RunOverviewPresence::Running, "thinking");
        person.area_id = area.into();
        super::super::IslandProject {
            source: 7,
            area_id: area.into(),
            name: area.into(),
            root: format!("/{area}").into(),
            loaded: true,
            overview: RunOverview {
                attention: vec![],
                running: vec![person],
                idle: vec![],
                unloaded_projects: vec![],
            },
            cached_overview: RunOverview {
                attention: vec![],
                running: vec![],
                idle: vec![],
                unloaded_projects: vec![],
            },
            approvals: vec![],
            questions: vec![],
            proposals: vec![],
        }
    };
    let space = IslandWorkspace {
        database: "db".into(),
        id: "ws".into(),
        name: "Norma".into(),
        projects: vec![project("area-a", "Ada"), project("area-b", "Bea")],
    };
    let faces = collect(std::slice::from_ref(&space));
    let matched =
        matching_completion_face(&faces, &agent_id(7, "area-b", 42)).expect("完成身份必须命中 Bea");
    assert_eq!(matched.name, "Bea", "同号节点必须留在自己的项目");
    assert_eq!(matched.area_id, "area-b", "命中的项目是 area-b");
    assert!(
        matching_completion_face(&faces, "7:42").is_none(),
        "丢掉项目后不能绑到 Ada"
    );
}

fn proposal(id: u64, agent: &str) -> super::super::IslandProposal {
    super::super::IslandProposal {
        id,
        area_id: "area".into(),
        agent: agent.into(),
        title: "下一步".into(),
        task: "去做".into(),
        why: "因为空着".into(),
        block: None,
        creates_card: false,
    }
}

/// 场景：提议要改头像槽，但没有卡片的 Agent 不能占用画布节点号。
/// 数据：Iris 已在跑，节点 2。提议 3 指向 Iris，提议 9 指向还没有脸的 Nova。
/// 执行：收集脸和队列，再用队列步进从审批跳过提议去成果。
/// 预期：Iris 离开运行槽并等待，节点仍是 2；Nova 的节点是高位合成号；队列步进选中成果而不是提议。
#[test]
fn proposal_moves_a_known_agent_and_synthesizes_an_unknown_one() {
    let mut space = workspace(
        vec![
            agent("Grace", 1, RunOverviewPresence::Running, "thinking"),
            agent("Iris", 2, RunOverviewPresence::Running, "thinking"),
        ],
        vec![],
        vec![review(4, "Leo", "turn-leo")],
        vec![],
    );
    space.projects[0].proposals = vec![proposal(3, "Iris"), proposal(9, "Nova")];
    let faces = collect(std::slice::from_ref(&space));
    let iris = faces
        .iter()
        .find(|face| face.name == "Iris")
        .expect("Iris 的脸必须在");
    assert_eq!(iris.node_id, 2, "已有卡片的人继续用画布节点 2");
    assert_eq!(iris.activity, AgentActivity::Waiting, "Iris 在等你处理提议");
    assert_eq!(iris.slot, SlotKind::Pending, "Iris 要从运行侧挪到待处理");
    // 高 16 位留给合成节点。9 只占低 48 位：(1 << 48) | 9。
    let synthetic = (1u64 << 48) | 9;
    let nova = faces
        .iter()
        .find(|face| face.name == "Nova")
        .expect("没有卡片时要合成 Nova");
    assert_eq!(nova.node_id, synthetic, "未知 Agent 不能占用画布节点号");
    assert_eq!(nova.activity, AgentActivity::Waiting, "合成的脸也是在等你");
    assert_ne!(nova.node_id, 2, "Nova 不能撞上 Iris 的节点");
    let grace = faces
        .iter()
        .find(|face| face.name == "Grace")
        .expect("Grace 仍在");
    assert_eq!(grace.activity, AgentActivity::Running, "没有提议的人继续跑");
    let items = queue_items(std::slice::from_ref(&space));
    assert!(
        items
            .iter()
            .any(|item| item.proposal && item.name == "Iris"),
        "队列要看得见 Iris 的提议"
    );
    assert!(
        items.iter().any(|item| item.review && !item.proposal),
        "成果仍是成果，不是提议"
    );
    let proposal_id = super::super::controller::proposal_key(7, "ws", "area", 3);
    let review_id = items
        .iter()
        .find(|item| item.review)
        .expect("队列里要有成果")
        .id
        .clone();
    let step = queue_step(
        &["approval".into(), proposal_id, review_id.clone()],
        &items,
        Some("approval"),
    );
    assert_eq!(
        step.focused_review.as_deref(),
        Some(review_id.as_str()),
        "下一步是成果"
    );
    assert!(step.select_alert.is_none(), "提议编号不能拿去选提醒");
    assert!(step.hold_review, "成果要按住提醒形态");
}

/// 场景：运行列表一行要同时给出会话名、用户最近一次输入和最近工具。
/// 数据：Grace 有会话名、最近输入与最近工具；Daisy 只有会话名，没有输入也没有工具。
/// 执行：collect。
/// 预期：三个字段各归其位；没有最近输入时 `last_input` 为空，不得退回会话名；没有工具时 `tool` 为空。
#[test]
fn face_carries_session_last_input_and_last_tool() {
    let mut grace = agent("Grace", 1, RunOverviewPresence::Running, "running_tool");
    grace.task = Some("支持打开项目外部文件".into());
    grace.latest_prompt = Some("/implement 开始实施吧".into());
    grace.last_tool = Some(norma::mobile::RecentTool {
        name: "Edit".into(),
        arg: Some("mod.rs".into()),
    });
    let daisy = agent("Daisy", 2, RunOverviewPresence::Idle, "idle");
    let faces = collect(&[workspace(vec![grace], vec![daisy], vec![], vec![])]);
    let face = |name: &str| faces.iter().find(|face| face.name == name).expect(name);
    assert_eq!(
        face("Grace").session,
        "支持打开项目外部文件",
        "标题是会话名"
    );
    assert_eq!(
        face("Grace").last_input,
        "/implement 开始实施吧",
        "第二行是最近输入"
    );
    assert_eq!(
        face("Grace")
            .tool
            .as_ref()
            .map(|t| (t.name.as_str(), t.arg.as_deref())),
        Some(("Edit", Some("mod.rs"))),
        "第三行是最近工具"
    );
    assert_eq!(face("Daisy").session, "Daisy task");
    assert_eq!(face("Daisy").last_input, "", "没有最近输入时不得退回会话名");
    assert!(face("Daisy").tool.is_none(), "没有工具记录时不画第三行");
}
