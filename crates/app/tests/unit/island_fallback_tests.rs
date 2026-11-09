use super::*;
use crate::island::{IslandModel, IslandProject, IslandWorkspace};
use crate::test_run_overview_rows as rows;
use gpui::{AppContext, TestAppContext};
use norma::mobile::{ApprovalSnapshot, ApprovalTarget};
use runtime::island::IslandPreferences;
use std::{cell::RefCell, rc::Rc};

fn workspace_with_approval() -> IslandWorkspace {
    let mut overview = rows::sample(2_000_000);
    overview.running.clear();
    let approval = ApprovalSnapshot {
        target: ApprovalTarget {
            node_id: 7,
            profile: "Claude".into(),
            run_id: "r".into(),
            session_id: "s".into(),
            turn_id: "t".into(),
            request_id: "a1".into(),
        },
        card_title: "Agent".into(),
        tool: "Bash".into(),
        details: serde_json::json!({}),
        decisions: vec![norma::agent_hook::ApprovalDecision::Allow],
    };
    IslandWorkspace {
        database: "db.sqlite".into(),
        id: "w".into(),
        name: "Workspace".into(),
        projects: vec![IslandProject {
            source: 1,
            area_id: "a".into(),
            name: "Project".into(),
            root: "/project".into(),
            loaded: true,
            cached_overview: overview.clone(),
            overview,
            approvals: vec![approval],
            questions: vec![],
            proposals: vec![],
        }],
    }
}

/// 场景：岛隐藏后，新审批必须出现系统通知入口；可见时不发；取消隐藏不会重复弹出。
/// 数据：一个工作空间含一条审批；Controller 的投递口换成记录器。
/// 执行：先以可见偏好刷新，再设为隐藏刷新两次。
/// 预期：可见时 0 条；隐藏首次 1 条且标题为本地化审批文案、正文含工作空间名；重复刷新仍 1 条。
#[test]
fn hidden_island_mirrors_new_approval_to_system_notification_once() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| crate::island::preferences::install_for_test(IslandPreferences::default(), cx));
    let model = cx.new(|_| {
        let mut model = IslandModel::default();
        model.workspaces = vec![workspace_with_approval()];
        model
    });
    let controller = cx.new(|cx| IslandController::new(model, cx));
    let posted = Rc::new(RefCell::new(Vec::<(String, String, String)>::new()));
    controller.update(&mut cx, |controller, cx| {
        let sink = posted.clone();
        controller.post = Some(Box::new(move |id, title, body| {
            sink.borrow_mut()
                .push((id.into(), title.into(), body.into()))
        }));
        controller.refresh(cx);
    });
    assert!(
        posted.borrow().is_empty(),
        "岛可见时由岛自身提示，不发系统通知"
    );
    cx.update(|cx| {
        crate::island::preferences::select(crate::island::preferences::Setting::Hidden, true, cx)
    });
    controller.update(&mut cx, |controller, cx| {
        controller.refresh(cx);
        controller.refresh(cx);
    });
    let posted = posted.borrow();
    assert_eq!(posted.len(), 1, "隐藏后同一审批只通知一次: {posted:?}");
    assert_eq!(
        posted[0].1,
        ui::i18n::text("island.notify_approval"),
        "标题应为本地化的审批提示"
    );
    assert!(
        posted[0].2.contains("Workspace"),
        "正文应指出所属工作空间: {}",
        posted[0].2
    );
}

/// 场景：点击通知能解析出审批所属的精确卡片，已消失的请求则解析失败。
/// 数据：同上；一个真实存在的审批 ID 和一个陈旧 ID。
/// 执行：对两个 ID 调用 alert_site。
/// 预期：真实 ID 返回源 1、工作空间 w、区域 a、节点 7；陈旧 ID 为 None。
#[test]
fn notification_click_resolves_exact_card_or_nothing() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| crate::island::preferences::install_for_test(IslandPreferences::default(), cx));
    let workspace = workspace_with_approval();
    let approval = workspace.projects[0].approvals[0].target.clone();
    let model = cx.new(|_| {
        let mut model = IslandModel::default();
        model.workspaces = vec![workspace];
        model
    });
    let controller = cx.new(|cx| IslandController::new(model, cx));
    controller.update(&mut cx, |controller, cx| {
        let id = alert_key(1, "w", "a", &approval);
        let (target, node, name) = controller.alert_site(&id, cx).expect("真实审批应可定位");
        assert_eq!(
            (
                target.source,
                target.workspace.as_str(),
                target.area.as_str(),
                node
            ),
            (1, "w", "a", 7),
            "必须解析到发起请求的精确卡片"
        );
        assert_eq!(name, "Workspace", "应返回工作空间名用于正文");
        assert!(
            controller.alert_site("stale", cx).is_none(),
            "陈旧通知不得猜测目标"
        );
    });
}
