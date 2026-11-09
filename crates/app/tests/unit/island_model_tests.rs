use super::*;

/// 场景：两个宿主各保留不同工作空间，同一空间也可出现在另一宿主的未加载缓存里。
/// 数据：A/B 都使用 area-1；宿主 1 加载 A，宿主 2 加载 B 并保存 A 的旧缓存。
/// 执行：依次发布两份投影，再原样重发，最后释放宿主 1。
/// 预期：A/B 不串空间，实时 A 胜过旧缓存；重复发布不通知；释放后仅移除对应实时贡献。
#[test]
fn island_model_merges_retained_sources_without_duplicate_projects() {
    let mut model = IslandModel::default();
    let a = workspace("a", "Alpha", true);
    let b = workspace("b", "Beta", true);
    assert!(
        publish(
            &mut model,
            1,
            vec![a.clone(), workspace("b", "Beta", false)]
        ),
        "首个宿主应改变缓存"
    );
    assert!(
        publish(
            &mut model,
            2,
            vec![workspace("a", "Alpha", false), b.clone()]
        ),
        "另一个空间应加入全局视图"
    );
    assert_eq!(model.workspaces.len(), 2, "相同区域 ID 不能导致空间合并");
    assert_eq!(
        model.workspaces[0].projects.len(),
        1,
        "同一项目不能显示两遍"
    );
    assert!(
        model.workspaces[0].projects[0].loaded,
        "实时数据必须覆盖未加载缓存"
    );
    assert_eq!(
        model.workspaces[0].projects[0].source, 1,
        "操作路由必须保留真正宿主"
    );
    assert_eq!(
        model.workspaces[1].projects[0].source, 2,
        "后台 B 必须可精确路由"
    );
    assert!(
        !publish(&mut model, 1, vec![a, workspace("b", "Beta", false)]),
        "可见数据不变不能要求重绘"
    );
    assert!(model.remove_source(1), "真正释放宿主必须刷新贡献");
    assert!(
        !model.workspaces[0].projects[0].loaded,
        "释放 A 后只能显示持久化缓存"
    );
    assert!(model.workspaces[1].projects[0].loaded, "释放 A 不得影响 B");
}

fn workspace(id: &str, name: &str, loaded: bool) -> IslandWorkspace {
    IslandWorkspace {
        database: "/test/app.db".into(),
        id: id.into(),
        name: name.into(),
        projects: vec![IslandProject {
            source: 0,
            area_id: "area-1".into(),
            name: "Project".into(),
            root: "/test/project".into(),
            loaded,
            overview: norma::mobile::RunOverview {
                attention: Vec::new(),
                running: Vec::new(),
                idle: Vec::new(),
                unloaded_projects: Vec::new(),
            },
            cached_overview: norma::mobile::RunOverview {
                attention: Vec::new(),
                running: Vec::new(),
                idle: Vec::new(),
                unloaded_projects: Vec::new(),
            },
            approvals: Vec::new(),
            questions: Vec::new(),
            proposals: Vec::new(),
        }],
    }
}

/// 场景：同一项目在两扇窗口中有独立 Runtime，第二扇窗口后来重命名/删除了空间。
/// 数据：两个 loaded 来源使用同一个 workspace/area；第二来源名称从 Alpha 改为 Renamed。
/// 执行：发布两来源，再从第二来源发布删除后的空目录。
/// 预期：两份实时贡献都保留，名称采用最新目录；旧来源不能复活已删除空间。
#[test]
fn island_model_keeps_both_live_owners_and_uses_latest_catalog() {
    let mut model = IslandModel::default();
    let mut first = workspace("a", "Alpha", true);
    first.projects[0].approvals.push(approval("pwd"));
    let mut second = workspace("a", "Renamed", true);
    second.projects[0].approvals.push(approval("ls"));
    publish(&mut model, 1, vec![first]);
    publish(&mut model, 2, vec![second]);
    assert_eq!(
        model.workspaces[0].projects.len(),
        2,
        "两份独立 Runtime 的待办不能互相覆盖"
    );
    assert_eq!(
        model.workspaces[0].name, "Renamed",
        "旧宿主不能压住已重命名的目录"
    );
    assert_eq!(
        model.workspaces[0].projects[0].approvals[0].details["command"], "pwd",
        "首个宿主待办仍在"
    );
    assert_eq!(
        model.workspaces[0].projects[1].approvals[0].details["command"], "ls",
        "同 request ID 的另一个宿主待办不能丢"
    );
    publish(&mut model, 2, Vec::new());
    assert!(
        model.workspaces.is_empty(),
        "旧来源不能复活最新目录已删除的空间"
    );
}

/// 场景：旧宿主投影晚于目录删除；唯一已发布来源在旧投影回来前被释放。
/// 数据：ticket 1 捕获 Alpha；ticket 2 先发布空目录后释放，另一个宿主才返回 ticket 1。
/// 执行：按上述反序交付，再检查已停止来源的迟到交付。
/// 预期：空目录的版本不能随释放回退；已释放来源也不能复活自己的旧数据。
#[test]
fn island_model_rejects_late_catalog_and_released_source() {
    let mut model = IslandModel::default();
    let old = model.begin_refresh();
    let new = model.begin_refresh();
    model.replace_source(2, new, "/test/app.db".into(), Vec::new());
    model.remove_source(2);
    model.replace_source(
        1,
        old,
        "/test/app.db".into(),
        vec![workspace("a", "Alpha", true)],
    );
    assert!(
        model.workspaces.is_empty(),
        "最后已发布宿主释放不能回退目录版本"
    );

    let mut model = IslandModel::default();
    let version = model.begin_refresh();
    let rows = vec![workspace("a", "Alpha", true)];
    model.replace_source(1, version, "/test/app.db".into(), rows.clone());
    model.remove_source(1);
    assert!(
        !model.replace_source(1, version, "/test/app.db".into(), rows),
        "释放后的回调必须被拒绝"
    );
    assert!(model.workspaces.is_empty(), "已停止来源不能重新显示");
}

fn approval(command: &str) -> norma::mobile::ApprovalSnapshot {
    norma::mobile::ApprovalSnapshot {
        target: norma::mobile::ApprovalTarget {
            node_id: 7,
            profile: "Claude".into(),
            run_id: "r".into(),
            session_id: "s".into(),
            turn_id: "t".into(),
            request_id: "request-1".into(),
        },
        card_title: "Ada".into(),
        tool: "Bash".into(),
        details: serde_json::json!({"command":command}),
        decisions: vec![norma::agent_hook::ApprovalDecision::Allow],
    }
}

/// 场景：较慢宿主首次返回待审批时，另一个宿主已发布更新的目录与成果已读状态。
/// 数据：ticket 1 有实时审批和旧 File；ticket 2 的同项目已重命名、成果列表为空。
/// 执行：先交付 ticket 2，再交付 ticket 1；第二来源加载同项目，发布一个成果再标记已读。
/// 预期：保留晚到的实时审批，但不能恢复旧名称或已读成果；双实时来源的同一成果只显示一次。
#[test]
fn island_model_accepts_late_live_requests_without_restoring_read_files() {
    let mut model = IslandModel::default();
    let old = model.begin_refresh();
    let new = model.begin_refresh();
    let mut live = workspace("a", "Alpha", true);
    live.projects[0].approvals.push(approval("pwd"));
    live.projects[0]
        .overview
        .attention
        .push(norma::mobile::RunOverviewAttention {
            kind: RunOverviewAttentionKind::File,
            area_id: "area-1".into(),
            node_id: 7,
            project: "Old Project".into(),
            agent: "Ada".into(),
            title: "report.md".into(),
            run_id: String::new(),
            turn_id: "t".into(),
            request_id: None,
            fingerprint: Some("v1".into()),
            paths: vec!["report.md".into()],
            loaded: true,
        });
    let file = live.projects[0].overview.attention[0].clone();
    model.replace_source(
        2,
        new,
        "/test/app.db".into(),
        vec![workspace("a", "Renamed", false)],
    );
    model.replace_source(1, old, "/test/app.db".into(), vec![live]);
    assert_eq!(
        model.workspaces[0].name, "Renamed",
        "迟到任务不能覆盖目录名称"
    );
    let project = &model.workspaces[0].projects[0];
    assert_eq!(project.source, 1, "较早但首次返回的实时来源不能整批丢弃");
    assert_eq!(project.approvals.len(), 1, "晚到的真实待办必须显示");
    assert!(
        project.overview.attention.is_empty(),
        "旧实时快照里的已读 File 不得复活"
    );
    let mut second = workspace("a", "Renamed", true);
    second.projects[0].overview.attention.push(file.clone());
    second.projects[0].cached_overview.attention.push(file);
    publish(&mut model, 2, vec![second.clone()]);
    assert_eq!(
        model.workspaces[0].projects.len(),
        2,
        "两个实时 Runtime 均存在"
    );
    let file_count = |model: &IslandModel| {
        model.workspaces[0]
            .projects
            .iter()
            .flat_map(|project| &project.overview.attention)
            .filter(|row| row.kind == RunOverviewAttentionKind::File)
            .count()
    };
    assert_eq!(file_count(&model), 1, "共享成果不能按实时来源重复展示");
    second.projects[0].cached_overview.attention.clear();
    publish(&mut model, 2, vec![second]);
    assert_eq!(
        file_count(&model),
        0,
        "最新已读状态必须移除所有实时来源里的旧成果"
    );
}

/// 场景：隔离数据库恰好具有相同的工作空间和区域 ID。
/// 数据：两个数据库各有 A/area-1；第一个数据库随后发布空目录。
/// 执行：发布、删除第一库，再读取第二库的缓存。
/// 预期：两库最初分别存在；第一库删除不能抹掉第二库。
#[test]
fn island_model_separates_database_scopes() {
    let mut model = IslandModel::default();
    publish(&mut model, 1, vec![workspace("a", "Alpha", true)]);
    let mut other = workspace("a", "Isolated", true);
    other.database = "/other/app.db".into();
    let version = model.begin_refresh();
    model.replace_source(2, version, other.database.clone(), vec![other]);
    assert_eq!(
        model.workspaces.len(),
        2,
        "不同数据库不能按相同 workspace ID 合并"
    );
    publish(&mut model, 1, Vec::new());
    assert_eq!(model.workspaces.len(), 1, "只删除第一库的目录");
    assert_eq!(model.workspaces[0].name, "Isolated", "第二库必须保留");
}

/// 场景：已加载项目带着本空间的提议；未加载目录、别的空间和后来拿掉提议的投影都不能留下它。
///
/// 数据构造：
///   提议 7：agent Ada，title 整理日志，挂在已加载的 Alpha/area-1
///   提议 9：只写在最新目录里 Alpha 的未加载副本上。实时宿主还在时不能盖掉 7；宿主释放后也不能留下
///   Beta 全程没有提议
///
/// 执行过程：
///   1. 宿主 1 发布已加载 Alpha（提议 7）和未加载 Beta
///   2. 原样再发布一次
///   3. 宿主 1 再发布一份没有提议的已加载 Alpha
///   4. 重新放回提议 7，然后宿主 2 用更新的目录发布未加载 Alpha（7 和 9）与已加载 Beta
///   5. 释放宿主 1。Alpha 不再有实时宿主，只能走这份未加载目录
///
/// 预期结果：
///   - 第 1 步 Alpha 只有提议 7，Beta 没有提议
///   - 第 2 步不通知
///   - 第 3 步提议 7 消失，提议 9 也不存在
///   - 第 4 步已加载 Alpha 仍只有 7，Beta 没有提议
///   - 第 5 步未加载 Alpha 的提议被清空，不能留下 7 或 9
#[test]
fn island_model_keeps_loaded_proposals_and_drops_cached_or_removed_ones() {
    let mut model = IslandModel::default();
    let mut loaded = workspace("a", "Alpha", true);
    loaded.projects[0].proposals.push(proposal(7));
    let unloaded_beta = workspace("b", "Beta", false);
    assert!(
        publish(&mut model, 1, vec![loaded.clone(), unloaded_beta.clone()]),
        "首次带提议的投影必须通知"
    );
    let shown: Vec<u64> = model.workspaces[0].projects[0]
        .proposals
        .iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(shown, vec![7], "已加载 Alpha 必须带着提议 7");
    assert!(
        model.workspaces[1].projects[0].proposals.is_empty(),
        "未加载 Beta 不能凭空出现提议"
    );
    assert!(
        !publish(&mut model, 1, vec![loaded, unloaded_beta]),
        "相同提议不能重复通知"
    );

    let cleared = workspace("a", "Alpha", true);
    assert!(
        publish(&mut model, 1, vec![cleared, workspace("b", "Beta", false)]),
        "拿掉提议必须通知"
    );
    assert!(
        model.workspaces[0].projects[0].proposals.is_empty(),
        "新的已加载投影没有提议时，旧的 7 必须消失"
    );
    assert!(
        model.workspaces.iter().all(|workspace| {
            workspace
                .projects
                .iter()
                .all(|project| project.proposals.iter().all(|item| item.id != 9))
        }),
        "提议 9 这时还没有发布"
    );

    let mut loaded = workspace("a", "Alpha", true);
    loaded.projects[0].proposals.push(proposal(7));
    publish(&mut model, 1, vec![loaded, workspace("b", "Beta", false)]);
    let mut stale = workspace("a", "Alpha", false);
    stale.projects[0].proposals.push(proposal(7));
    stale.projects[0].proposals.push(proposal(9));
    assert!(
        publish(&mut model, 2, vec![stale, workspace("b", "Beta", true)]),
        "第二个宿主必须加入，并让未加载目录带上提议 9"
    );
    let kept: Vec<u64> = model.workspaces[0].projects[0]
        .proposals
        .iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(
        kept,
        vec![7],
        "已加载宿主的提议 7 必须留下，不能被目录里的 9 替换"
    );
    assert!(
        model.workspaces[0].projects[0].loaded,
        "Alpha 仍由宿主 1 加载"
    );
    assert_eq!(
        model.workspaces[0].projects[0].source, 1,
        "Alpha 必须路由到宿主 1"
    );
    assert!(
        model.workspaces[1].projects[0].proposals.is_empty(),
        "Beta 没有提议"
    );
    assert_eq!(
        model.workspaces[1].projects[0].source, 2,
        "Beta 必须路由到宿主 2"
    );

    assert!(model.remove_source(1), "释放宿主 1 必须刷新");
    let cached = &model.workspaces[0].projects[0];
    assert!(!cached.loaded, "释放后 Alpha 只剩未加载目录");
    assert!(
        cached.proposals.is_empty(),
        "未加载目录必须清掉提议，不能留下 7 或 9"
    );
    assert!(
        model.workspaces[1].projects[0].loaded,
        "释放 Alpha 的宿主不得卸下 Beta"
    );
    assert!(
        model.workspaces[1].projects[0].proposals.is_empty(),
        "Beta 仍然没有提议"
    );
}

fn proposal(id: u64) -> IslandProposal {
    IslandProposal {
        id,
        area_id: "area-1".into(),
        agent: "Ada".into(),
        title: "整理日志".into(),
        task: "把今天的日志收成三条".into(),
        why: "日志已经散在三处".into(),
        block: None,
        creates_card: false,
    }
}

fn publish(model: &mut IslandModel, source: u64, workspaces: Vec<IslandWorkspace>) -> bool {
    let version = model.begin_refresh();
    model.replace_source(source, version, "/test/app.db".into(), workspaces)
}
