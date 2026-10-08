use norma::mobile::{pending_items, WorkspaceActivityFacts};
use serde_json::json;
use std::collections::HashSet;

fn facts() -> WorkspaceActivityFacts {
    serde_json::from_value(json!({
        "workspace":{"id":"space-a","name":"桌面空间 A","revision":1,"projects":[{
            "area_id":"area-a","project_id":"project-a","name":"项目 A","root":"/local",
            "definitions":[],"agents":[{"definition_id":1,"name":"Agent","profile":"Codex",
                "card":{"node_id":1,"profile":"Codex","title":"Agent","mobile_control":true,
                    "state":"failed","run_id":"run","session_id":"session","turn_id":"turn",
                    "task_title":"修复构建","task_summary":"构建失败",
                    "task_timing":{"started_at_unix_ms":100500,"completed_elapsed_ms":20500}}}]
        }]},"observed":true,
        "approvals":[{"area_id":"area-a","approval":{
            "target":{"node_id":1,"profile":"Codex","run_id":"run","session_id":"session",
                "turn_id":"turn","request_id":"request"},"card_title":"Agent","tool":"Bash",
            "details":{},"decisions":["allow","deny"]}}],
        "reviews":[{"area_id":"area-a","node_id":1,"profile":"Codex","run_id":"old-run",
            "session_id":"old-session","turn_id":"old-turn","fingerprint":"a".repeat(64),
            "completed":true,"available":true,"conflicted":false,"accepted":false,"file_count":2,
            "saved_at_unix":150}]
    }))
    .expect("完整桌面事实")
}

/// 场景：同一卡片的三种真实待处理事项独立返回，并按创建时间混排。
/// 数据构造：审批回退到回合开始100秒；失败=(100500+20500)/1000=121秒；交付150秒。
/// 执行过程：投影一个空间的审批、失败、旧回合交付，再序列化结果。
/// 预期结果：review/failed/approval依次返回，保留原目标、项目、桌面空间、线程及说明。
#[test]
fn pending_items_preserve_all_kinds_and_real_target_metadata() {
    let items = pending_items("mac", vec![facts()], &HashSet::new());
    assert_eq!(items.len(), 3, "一个项目的三种事项不能互相覆盖");
    assert_eq!(
        items.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
        vec!["review", "failed", "approval"],
        "按创建时间倒序，不按类型优先级"
    );
    assert_eq!(
        items.iter().map(|i| i.created_at).collect::<Vec<_>>(),
        vec![150, 121, 100],
        "时间来自交付保存、失败结束和审批回合开始"
    );
    assert_eq!(
        items[0].thread_id, "old-session",
        "旧成果线程不能换成当前session"
    );
    assert_eq!(items[0].target.run_id, "old-run", "成果必须保留原始运行");
    assert_eq!(items[0].workspace_name, "桌面空间 A", "必须是Mac桌面空间名");
    assert_eq!(items[0].desktop_workspace_id, "space-a", "深链保留来源空间");
    assert_eq!(items[0].project_id, "project-a", "身份按项目，不按区域");
    assert_eq!(items[1].title, "修复构建", "失败使用真实任务标题");
    assert_eq!(items[1].reason, "构建失败", "保持Mac现有说明");
    assert_eq!(
        items[2].target.request_id.as_deref(),
        Some("request"),
        "审批精确定位请求"
    );
    assert_eq!(
        items[0].target.fingerprint.as_deref(),
        Some("a".repeat(64).as_str()),
        "验收精确定位指纹"
    );
    assert!(
        items[1].target.request_id.is_none(),
        "失败不能夹带审批request"
    );
    let value = serde_json::to_value(&items).expect("可序列化列表");
    assert_eq!(value[0]["machine_id"], "mac", "来源Mac必须可识别");
}

/// 场景：完整列表不受三项目摘要预算影响；重复绑定按项目事项去重。
/// 数据构造：5个独立项目各3事项=15；第二空间重复这5个项目但改area_id。
/// 执行过程：合并两空间、颠倒输入、忽略首空间的失败键，再换失败run/turn。
/// 预期结果：15项且顺序稳定；忽略后两空间共14项；新失败独立出现。
#[test]
fn pending_is_complete_stable_and_dismissed_across_workspace_bindings() {
    let mut a = facts();
    for n in 1..5 {
        let mut project = a.workspace.projects[0].clone();
        project.project_id = format!("project-{n}");
        project.area_id = format!("area-{n}");
        let mut approval = a.approvals[0].clone();
        approval.area_id = project.area_id.clone();
        let mut review = a.reviews[0].clone();
        review.area_id = project.area_id.clone();
        a.workspace.projects.push(project);
        a.approvals.push(approval);
        a.reviews.push(review);
    }
    let mut b = a.clone();
    b.workspace.id = "space-b".into();
    b.workspace.name = "桌面空间 B".into();
    for p in &mut b.workspace.projects {
        p.area_id.push_str("-b");
    }
    for approval in &mut b.approvals {
        approval.area_id.push_str("-b");
    }
    for review in &mut b.reviews {
        review.area_id.push_str("-b");
    }
    let items = pending_items("mac", vec![a.clone(), b.clone()], &HashSet::new());
    assert_eq!(
        items.len(),
        15,
        "全量15项，不能截断到3项目或只取每项目优先项"
    );
    assert_eq!(
        items,
        pending_items("mac", vec![b.clone(), a.clone()], &HashSet::new()),
        "相同时间和重复绑定不能随输入顺序跳动"
    );
    let failed = items
        .iter()
        .find(|i| i.kind == "failed" && i.project_id == "project-a")
        .expect("项目A失败");
    let dismissed = HashSet::from([failed.item_key.clone()]);
    let visible = pending_items("mac", vec![a.clone(), b.clone()], &dismissed);
    assert_eq!(visible.len(), 14, "项目级忽略恰好移除一个去重事项");
    assert!(
        !visible.iter().any(|i| i.item_key == failed.item_key),
        "不能换工作空间重新冒出"
    );
    assert_eq!(
        pending_items("mac", vec![b], &dismissed).len(),
        14,
        "单独读取第二空间也遵守忽略"
    );
    a.workspace.projects[0].agents[0].card.run_id = Some("new-run".into());
    a.workspace.projects[0].agents[0].card.turn_id = Some("new-turn".into());
    let new = pending_items("mac", vec![a], &dismissed);
    assert!(
        new.iter()
            .any(|i| i.kind == "failed" && i.target.run_id == "new-run"),
        "新run/turn不受旧忽略影响"
    );
    assert!(
        !new.iter().any(|i| i.item_key == failed.item_key),
        "旧失败不能残留"
    );
}

/// 场景：无真实待处理目标的失败/提问与不符合验收条件的交付不进入列表。
/// 数据构造：依次清空失败run/turn；approval decisions为空；四种不可验收事实。
/// 执行过程：移除有效审批和交付，修改卡片状态及身份，再逐种投影排除成果。
/// 预期结果：无run或turn的失败、提问、错Provider/冲突/不可用/已验收均不返回。
#[test]
fn pending_excludes_unactionable_failures_questions_and_nonpending_reviews() {
    let mut f = facts();
    f.approvals[0].approval.decisions.clear();
    let review = f.reviews.remove(0);
    for field in ["run", "turn"] {
        let mut missing = f.clone();
        let card = &mut missing.workspace.projects[0].agents[0].card;
        if field == "run" {
            card.run_id = Some(String::new());
        } else {
            card.turn_id = None;
        }
        assert!(
            pending_items("mac", vec![missing], &HashSet::new()).is_empty(),
            "缺{field}的失败不计入N"
        );
    }
    f.workspace.projects[0].agents[0].card.state = "waiting_approval".into();
    assert!(
        pending_items("mac", vec![f.clone()], &HashSet::new()).is_empty(),
        "无工具决定的等待不能成为agent提问项"
    );
    for mode in [
        "unavailable",
        "conflicted",
        "accepted",
        "other_provider",
        "incomplete",
    ] {
        let mut excluded = review.clone();
        match mode {
            "unavailable" => excluded.available = false,
            "conflicted" => excluded.conflicted = true,
            "accepted" => excluded.accepted = true,
            "other_provider" => excluded.profile = "Claude".into(),
            _ => excluded.completed = false,
        }
        f.reviews = vec![excluded];
        assert!(
            pending_items("mac", vec![f.clone()], &HashSet::new()).is_empty(),
            "{mode}交付不生成待验收"
        );
    }
    f.reviews = vec![review];
    assert_eq!(
        pending_items("mac", vec![f], &HashSet::new()).len(),
        1,
        "有效旧成果仍保留，不要求当前卡片done"
    );
}

/// 场景：缺少计时的历史数据不被刷新时间伪装成新事项。
/// 数据构造：失败、审批、交付均缺计时；另有同一卡片第二个合法审批request。
/// 执行过程：两次投影相同事实，读取时间和两个审批ID。
/// 预期结果：全部时间为0，刷新稳定且两个请求独立保留。
#[test]
fn pending_missing_timing_is_explicit_and_multiple_requests_survive() {
    let mut f = facts();
    f.workspace.projects[0].agents[0].card.task_timing = None;
    f.reviews[0].saved_at_unix = 0;
    let mut extra = f.approvals[0].clone();
    extra.approval.target.request_id = "second-request".into();
    f.approvals.push(extra);
    let items = pending_items("mac", vec![f.clone()], &HashSet::new());
    assert_eq!(items.len(), 4, "同一卡片的两个审批必须分别返回");
    assert_eq!(
        items.iter().filter(|i| i.kind == "approval").count(),
        2,
        "不能只保留最后一个审批"
    );
    assert!(
        items.iter().all(|i| i.created_at == 0),
        "未知时间明确为0，不生成当前时间"
    );
    assert_eq!(
        items,
        pending_items("mac", vec![f], &HashSet::new()),
        "相同输入多次刷新保持位置"
    );
}

/// 场景：被排除的同指纹事实不能覆盖真正待验收交付的创建时间。
/// 数据构造：不可用旧交付保存10秒，合法交付保存150秒，指纹和目标相同。
/// 执行过程：旧事实排在合法事实前，再投影统一列表。
/// 预期结果：仅合法交付生成一项，时间仍为150秒而非10秒。
#[test]
fn pending_review_timestamp_belongs_to_the_eligible_delivery() {
    let mut f = facts();
    let mut unavailable = f.reviews[0].clone();
    unavailable.available = false;
    unavailable.saved_at_unix = 10;
    f.reviews.insert(0, unavailable);
    let items = pending_items("mac", vec![f], &HashSet::new());
    let review = items.iter().find(|i| i.kind == "review").expect("合法交付");
    assert_eq!(review.created_at, 150, "不能从被排除事实读取时间");
    assert_ne!(review.created_at, 10, "不可用交付不应影响排序");
}
