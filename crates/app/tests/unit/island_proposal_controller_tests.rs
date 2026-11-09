use super::*;
use crate::test_run_overview_rows as rows;
use gpui::{AppContext, TestAppContext};

fn proposal(id: u64, agent: &str) -> super::super::IslandProposal {
    super::super::IslandProposal {
        id,
        area_id: "a".into(),
        agent: agent.into(),
        title: format!("{agent} 的提议"),
        task: format!("任务 {id}"),
        why: "理由".into(),
        block: None,
        creates_card: false,
    }
}

fn workspace(proposals: Vec<super::super::IslandProposal>) -> IslandWorkspace {
    let mut overview = rows::sample(2_000_000);
    overview.running.clear();
    overview.idle.clear();
    overview.attention.clear();
    overview.unloaded_projects.clear();
    IslandWorkspace {
        database: "db.sqlite".into(),
        id: "w".into(),
        name: "Workspace".into(),
        projects: vec![super::super::IslandProject {
            source: 1,
            area_id: "a".into(),
            name: "Project".into(),
            root: "/project".into(),
            loaded: true,
            cached_overview: overview.clone(),
            overview,
            approvals: vec![],
            questions: vec![],
            proposals,
        }],
    }
}

/// 场景：同一条提议正在派发时，重复点击不能再发一次，也不能改成失效。
///
/// 数据：提议 7 的稳定键已在 pending 里。没有注册宿主。
/// 执行：再点派发，再点忽略。
/// 预期：消息仍为空，resolved 仍为空，pending 仍只有这一条。
#[test]
fn proposal_click_while_pending_does_not_expire_or_resolve() {
    let mut cx = TestAppContext::single();
    let model = cx.new(|_| {
        let mut model = IslandModel::default();
        model.workspaces = vec![workspace(vec![proposal(7, "Alan")])];
        model
    });
    let controller = cx.new(|cx| IslandController::new(model, cx));
    let key = proposal_key(1, "w", "a", 7);
    controller.update(&mut cx, |state, cx| {
        state.pending.insert(key.clone());
        let route = super::super::actions::IslandTarget {
            source: 1,
            workspace: "w".into(),
            area: "a".into(),
        };
        state.dispatch_proposal(route.clone(), 7, cx);
        state.dismiss_proposal(route, 7, cx);
        assert!(state.message.is_none(), "进行中的点击不能改成失效提示");
        assert!(
            state.resolved_proposals.is_empty(),
            "进行中的点击不能标成已处理"
        );
        assert!(state.pending.contains(&key), "进行中的键要留到这一轮结束");
        assert_eq!(state.pending.len(), 1, "不能再插入第二条进行中记录");
        assert_eq!(
            counts(&state.visible_workspaces(cx)),
            (0, 1),
            "没完成的提议仍算待办"
        );
    });
}

/// 场景：已派发或已忽略的提议从待办消失；模型里这条没了之后，处理记录也丢掉。
///
/// 数据：同一项目有提议 7 和 8。只把 7 标成已处理。
/// 执行：标成已处理后先裁剪；再从模型删掉 7 并通知观察者。
/// 预期：模型里还有 7 时裁剪留着它，可见列表只剩 8；删掉并刷新后处理记录消失，8 不受影响。
#[test]
fn resolved_proposal_hides_until_the_model_drops_it() {
    let mut cx = TestAppContext::single();
    let model = cx.new(|_| {
        let mut model = IslandModel::default();
        model.workspaces = vec![workspace(vec![proposal(7, "Alan"), proposal(8, "Nobody")])];
        model
    });
    let controller = cx.new(|cx| IslandController::new(model.clone(), cx));
    let alan = proposal_key(1, "w", "a", 7);
    let nobody = proposal_key(1, "w", "a", 8);
    controller.update(&mut cx, |state, cx| {
        state.resolved_proposals.insert(alan.clone());
        state.prune_resolved_proposals(cx);
        let visible = state.visible_workspaces(cx);
        let ids: Vec<u64> = visible[0].projects[0]
            .proposals
            .iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(ids, vec![8], "已处理的 Alan 不能留在待办");
        assert!(!ids.contains(&7), "提议 7 必须从可见列表消失");
        assert_eq!(counts(&visible), (0, 1), "徽标只数还看得见的 Nobody");
        assert!(
            state.resolved_proposals.contains(&alan),
            "模型里还有这条时，裁剪必须留下处理记录"
        );
        assert!(
            !state.resolved_proposals.contains(&nobody),
            "没处理的 Nobody 不能被标成已处理"
        );
    });
    model.update(&mut cx, |model, cx| {
        model.workspaces[0].projects[0]
            .proposals
            .retain(|item| item.id != 7);
        cx.notify();
    });
    controller.update(&mut cx, |state, cx| {
        assert!(
            !state.resolved_proposals.contains(&alan),
            "模型不再有这条后，处理记录要裁掉，避免以后同编号被藏住"
        );
        assert!(
            !state.resolved_proposals.contains(&nobody),
            "裁剪不能误删 Nobody"
        );
        let ids: Vec<u64> = state.visible_workspaces(cx)[0].projects[0]
            .proposals
            .iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(ids, vec![8], "Nobody 仍在待办");
    });
}
