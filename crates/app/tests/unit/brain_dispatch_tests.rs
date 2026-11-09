use super::*;
use crate::app::brain_host::proposal_actions::classify_dispatch_block;
use gpui::{AppContext as _, TestAppContext};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use terminal::{PtyBackend, PtyError};

#[derive(Default)]
struct RejectTaskPty(Arc<AtomicUsize>);

impl PtyBackend for RejectTaskPty {
    fn write(&mut self, _: &[u8]) -> Result<(), PtyError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Err(PtyError::Io(std::io::Error::other(
            "fixture rejects task input",
        )))
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

/// 概览只能展示当前工作空间的提议，切换后不能留下其他空间的不可处理条目。
/// 数据构造：A/B 两个工作空间各有一个后台会话和同编号、不同标题的提议。
/// 执行过程：汇总 A，再切换 B 后重新汇总。
/// 预期结果：每次只出现当前空间的一条提议，不出现另一条。
#[test]
fn overview_proposals_follow_the_active_workspace() {
    let mut test = TestAppContext::single();
    let factory =
        || -> runtime::PtyFactory { Box::new(|_, _, _, _| Ok(Box::new(RejectTaskPty::default()))) };
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    let a = cx.new(|cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    let b = cx.new(|cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    for (session, title) in [(&a, "A 的提议"), (&b, "B 的提议")] {
        session.update(cx, |app, _| {
            app.brain
                .proposals
                .add(
                    proposals::Draft {
                        agent: "Alan".into(),
                        title: title.into(),
                        task: "完整任务".into(),
                        why: "夹具".into(),
                    },
                    std::time::SystemTime::UNIX_EPOCH,
                )
                .unwrap();
        });
    }
    host.update(cx, |app, cx| {
        app.desktop_workspace
            .sessions
            .insert(("A".into(), "area-a".into()), a);
        app.desktop_workspace
            .sessions
            .insert(("B".into(), "area-b".into()), b);
        for (workspace, title) in [("A", "A 的提议"), ("B", "B 的提议")] {
            app.desktop_workspace.active_id = Some(workspace.into());
            app.refresh_brain_overview(cx);
            assert_eq!(
                app.brain.overview.proposals.len(),
                1,
                "不能混入其他空间的条目"
            );
            assert_eq!(
                app.brain.overview.proposals[0].proposal.title, title,
                "提议应属于当前空间"
            );
        }
    });
}

/// 秒级持久化会使同一秒内的提议时间相同，概览仍须把最新编号排在前面。
/// 数据构造：同一空间的两条提议使用相同秒级时间，模拟重启后的快照。
/// 执行过程：按生产路径汇总概览。
/// 预期结果：新提议排第一，旧提议仍在且排第二，不能重启后倒序。
#[test]
fn overview_keeps_newest_proposal_first_when_timestamps_tie() {
    let mut test = TestAppContext::single();
    let factory =
        || -> runtime::PtyFactory { Box::new(|_, _, _, _| Ok(Box::new(RejectTaskPty::default()))) };
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    let session = cx.new(|cx| MultisoulApp::new_with_pty_factory(cx, factory()));
    session.update(cx, |app, _| {
        for name in ["A", "B"] {
            app.brain
                .proposals
                .add(
                    proposals::Draft {
                        agent: name.into(),
                        title: name.into(),
                        task: "完整任务".into(),
                        why: "夹具".into(),
                    },
                    std::time::SystemTime::UNIX_EPOCH,
                )
                .unwrap();
        }
    });
    host.update(cx, |app, cx| {
        app.desktop_workspace
            .sessions
            .insert(("A".into(), "area".into()), session);
        app.desktop_workspace.active_id = Some("A".into());
        app.refresh_brain_overview(cx);
        assert_eq!(
            app.brain
                .overview
                .proposals
                .iter()
                .map(|p| p.proposal.id)
                .collect::<Vec<_>>(),
            vec![2, 1],
            "时间相同时必须按递增编号的逆序展示"
        );
    });
}

/// 启动期间调整语音汇报不会取消或卡住已启动的服务。
/// 数据构造：一个待完成的启动，期间只修改 speak；返回不含子进程的测试 host。
/// 执行过程：应用无需重启的设置，再通过生产完成入口交付后台结果。
/// 预期结果：host 安装成功，starting 清空；不能永久停留在启动中。
#[test]
fn speaking_changes_do_not_strand_a_starting_session() {
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, _| Ok(Box::new(RejectTaskPty::default()))),
        )
    });
    entity.update(cx, |app, cx| {
        app.workspace_id = Some("fixture".into());
        app.workspace_route_id = Some("fixture-route".into());
        app.runtime.set_workspace_root("/fixture".into());
        app.ensure_brain_client(cx);
        app.apply_brain_settings(
            config::BrainSettings {
                enabled: true,
                ..Default::default()
            },
            true,
            cx,
        );
        let owner = owner::shared(cx);
        owner::support::prepare_owner(&owner);
        let generation = owner.begin_start(Instant::now()).expect("第一次启动");
        let settings = config::BrainSettings {
            enabled: true,
            speak: false,
            ..Default::default()
        };
        app.apply_brain_settings(settings, false, cx);
        let (_, status) = std::sync::mpsc::channel();
        owner.complete_start(
            generation,
            Ok(BrainHost {
                service: None,
                status,
                token: "fixture".into(),
            }),
        );
        assert!(app.brain.is_running(), "语音开关变化不能丢弃有效启动结果");
        assert!(app.brain.token_matches("fixture"), "启动结果必须可用");
        assert!(
            owner.begin_start(Instant::now()).is_none(),
            "不能重复启动第二宿主"
        );
    });
}

/// 旧启动结果不能清除新一轮启动，也不能在用户关闭幕僚后复活服务。
/// 数据构造：旧代启动中修改设置；旧任务收口后才允许新代启动。
/// 执行过程：交付旧结果，再关闭幕僚并交付新结果。
/// 预期结果：重复旧结果不改新启动；关闭后两个结果都不能安装 host。
#[test]
fn stale_start_results_never_replace_a_new_generation() {
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, _| Ok(Box::new(RejectTaskPty::default()))),
        )
    });
    entity.update(cx, |app, cx| {
        app.workspace_id = Some("fixture".into());
        app.workspace_route_id = Some("fixture-route".into());
        app.runtime.set_workspace_root("/fixture".into());
        app.ensure_brain_client(cx);
        app.apply_brain_settings(
            config::BrainSettings {
                enabled: true,
                ..Default::default()
            },
            true,
            cx,
        );
        let owner = owner::shared(cx);
        owner::support::prepare_owner(&owner);
        let old = owner.begin_start(Instant::now()).expect("旧代启动");
        app.apply_brain_settings(
            config::BrainSettings {
                enabled: true,
                speak: false,
                ..Default::default()
            },
            true,
            cx,
        );
        assert!(
            owner.begin_start(Instant::now()).is_none(),
            "旧启动未收口不得并发启动"
        );
        let host = || {
            Ok(BrainHost {
                service: None,
                status: std::sync::mpsc::channel().1,
                token: "fixture".into(),
            })
        };
        owner.complete_start(old, host());
        assert!(!app.brain.is_running(), "过期结果不能安装host");
        let generation = owner
            .begin_start(Instant::now())
            .expect("旧启动收口后启动新代");
        owner.complete_start(old, host());
        assert!(!app.brain.is_running(), "重复旧结果不能安装host");
        app.apply_brain_settings(config::BrainSettings::default(), true, cx);
        owner.complete_start(generation, host());
        assert!(!app.brain.is_running(), "关闭后不得安装迟到服务");
        assert!(
            owner.begin_start(Instant::now()).is_none(),
            "关闭后不能再次启动"
        );
    });
}

fn definition(id: i64, name: &str, enabled: bool) -> storage::AgentDefinition {
    storage::AgentDefinition {
        id,
        name: name.into(),
        speech_alias: None,
        runtime_profile: canvas::AgentProfile::Codex,
        workflow_description: String::new(),
        enabled,
        sort_order: 0,
        origin: storage::AgentDefinitionOrigin::Explicit,
    }
}

fn add_proposal(app: &mut MultisoulApp, agent: &str) -> u64 {
    app.brain
        .proposals
        .add(
            proposals::Draft {
                agent: agent.into(),
                title: format!("{agent} 的标题"),
                task: format!("{agent} 的完整任务"),
                why: "理由".into(),
            },
            std::time::SystemTime::UNIX_EPOCH,
        )
        .expect("夹具提议必须能写入")
}

/// 场景：派发阻断按「不在名册、停用、等确认、正忙」这条顺序决定，空闲则不阻断。
///
/// 数据：四个布尔组合。等确认和正忙同时为真时，等确认优先。
/// 执行：classify_dispatch_block。
/// 预期：找不到不会被写成停用；停用不会被写成正忙；空闲返回 None。
#[test]
fn classify_dispatch_block_prefers_missing_then_disabled_then_waiting() {
    assert_eq!(
        classify_dispatch_block(false, true, true, true),
        Some(proposals::ProposalBlock::Missing),
        "不在名册时，其他标志不能改成停用或正忙"
    );
    assert_eq!(
        classify_dispatch_block(true, false, true, true),
        Some(proposals::ProposalBlock::Disabled),
        "停用优先于等确认和正忙"
    );
    assert_eq!(
        classify_dispatch_block(true, true, true, true),
        Some(proposals::ProposalBlock::WaitingConfirmation),
        "等确认不能被正忙盖住"
    );
    assert_eq!(
        classify_dispatch_block(true, true, false, true),
        Some(proposals::ProposalBlock::Busy),
        "只正忙时才是忙碌"
    );
    assert_eq!(
        classify_dispatch_block(true, true, false, false),
        None,
        "在名册、启用、空闲时必须可以派发"
    );
}

/// 场景：岛上点派发时，找不到或已停用的提议留在列表里；忽略只删被点的那条。
///
/// 数据：名册里 Alan 启用、Dan 停用，没有 Nobody。三条提议各一个。
/// 执行：先读阻断，再派发 Nobody 和 Dan，忽略 Nobody，再忽略一次，再派发一个不存在的编号。
/// 预期：Nobody 是 Missing，Dan 是 Disabled，两条都还在；忽略后只剩 Dan；第二次忽略失败；不存在的编号不会删 Dan。
#[test]
fn dispatch_keeps_blocked_proposals_and_decline_removes_only_one() {
    let mut test = TestAppContext::single();
    let (session, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, _| Ok(Box::new(RejectTaskPty::default()))),
        )
    });
    session.update(cx, |app, _| {
        app.agent_definitions.replace_definitions_for_test(vec![
            definition(1, "Alan", true),
            definition(2, "Dan", false),
        ]);
        let nobody = add_proposal(app, "Nobody");
        let dan = add_proposal(app, "Dan");
        let alan = add_proposal(app, "Alan");
        assert_eq!(
            app.proposal_dispatch_block("Nobody"),
            Some(proposals::ProposalBlock::Missing),
            "名册没有 Nobody"
        );
        assert_eq!(
            app.proposal_dispatch_block("Dan"),
            Some(proposals::ProposalBlock::Disabled),
            "Dan 已停用"
        );
        assert_eq!(
            app.proposal_dispatch_block("Alan"),
            None,
            "启用且没有进行中的 Alan 可以派发"
        );
        let blocked = app.dispatch_brain_proposal(nobody);
        assert_eq!(
            blocked,
            Err(crate::island::ProposalDispatchError::Blocked {
                block: crate::island::IslandProposalBlock::Missing,
                agent: "Nobody".into(),
            }),
            "找不到的派发要带回 Agent 名"
        );
        assert!(
            app.brain
                .proposals
                .items()
                .iter()
                .any(|item| item.id == nobody),
            "阻断后 Nobody 的提议必须还在"
        );
        let disabled = app.dispatch_brain_proposal(dan);
        assert_eq!(
            disabled,
            Err(crate::island::ProposalDispatchError::Blocked {
                block: crate::island::IslandProposalBlock::Disabled,
                agent: "Dan".into(),
            }),
            "停用的派发不能执行"
        );
        assert!(
            app.brain
                .proposals
                .items()
                .iter()
                .any(|item| item.id == dan),
            "阻断后 Dan 的提议必须还在"
        );
        assert!(
            app.decline_brain_proposal(nobody),
            "第一次忽略 Nobody 必须成功"
        );
        assert!(
            app.brain
                .proposals
                .items()
                .iter()
                .all(|item| item.id != nobody),
            "忽略后 Nobody 的提议必须消失"
        );
        assert!(
            app.brain
                .proposals
                .items()
                .iter()
                .any(|item| item.id == dan),
            "忽略 Nobody 不能带走 Dan"
        );
        assert!(
            app.brain
                .proposals
                .items()
                .iter()
                .any(|item| item.id == alan),
            "忽略 Nobody 不能带走 Alan"
        );
        assert!(
            !app.decline_brain_proposal(nobody),
            "已经忽略的编号不能再成功一次"
        );
        assert_eq!(
            app.dispatch_brain_proposal(404),
            Err(crate::island::ProposalDispatchError::Missing),
            "没有这条提议时不能派发"
        );
        assert!(
            app.brain
                .proposals
                .items()
                .iter()
                .any(|item| item.id == dan),
            "不存在的编号不能删掉 Dan"
        );
    });
}
