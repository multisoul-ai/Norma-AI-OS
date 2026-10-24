use super::*;
use gpui::{AppContext as _, TestAppContext};
use norma::protocol::{AgentProfile, CardCommand, CardCommandResult};
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

/// 派发正文失败时提议必须保留，不能记录成用户已成功派发。
/// 数据构造：已就绪的 Alan 与拒绝所有写入的 PTY，一条带完整任务的提议。
/// 执行过程：经生产创建 Agent 入口建立卡片，用户派发提议，再读取结果与概览日志。
/// 预期结果：派发返回错误，原提议仍在，自主处理记录为空。
#[test]
fn failed_dispatch_keeps_the_proposal_and_does_not_log_success() {
    let mut test = TestAppContext::single();
    let dir = tempfile::tempdir().expect("应创建项目夹具");
    let attempts = Arc::new(AtomicUsize::new(0));
    let writes = attempts.clone();
    let factory: runtime::PtyFactory =
        Box::new(move |_, _, _, _| Ok(Box::new(RejectTaskPty(writes.clone()))));
    let (entity, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory));
    entity.update(cx, |app, cx| {
        app.enter_canvas_for_test(dir.path(), cx);
        app.brain.settings = Some(config::BrainSettings::default());
        app.brain.proposals_path = Some(dir.path().join("proposals.json"));
    });
    let created = entity.update_in(cx, |app, window, _| {
        crate::app::execute_agent_card_command(
            app,
            window,
            CardCommand::CreateAgent {
                name: Some("Alan".into()),
                profile: AgentProfile::Codex,
                activate: true,
            },
        )
        .expect("应创建测试卡片")
    });
    let CardCommandResult::AgentCreated {
        node_id: Some(node),
        ..
    } = created
    else {
        panic!("必须返回卡片编号：{created:?}");
    };
    entity.update(cx, |app, _| {
        app.pending_agent_submissions.mark_ready(node);
        let id = app
            .brain
            .proposals
            .add(
                proposals::Draft {
                    agent: "Alan".into(),
                    title: "测试提议".into(),
                    task: "完整测试任务".into(),
                    why: "验证失败".into(),
                },
                std::time::SystemTime::now(),
            )
            .expect("应创建提议");
        let before = attempts.load(Ordering::Relaxed);
        let result = app.dispatch_brain_proposal(id);
        assert!(result.is_err(), "失败回执不能被当成派发成功：{result:?}");
        assert!(
            attempts.load(Ordering::Relaxed) > before,
            "必须实际经过 PTY 失败分支，不能因其他拒绝而假通过"
        );
        assert!(
            result.unwrap_err().contains("未接收任务"),
            "应报告失败回执，而不是前置检查错误"
        );
        assert_eq!(
            app.brain.proposals.items().len(),
            1,
            "原提议应保留供用户重试"
        );
        assert!(app.brain.log.items().is_empty(), "不能生成成功派发记录");
    });
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
        let generation = app.brain.start_generation;
        app.brain.starting = Some(generation);
        let settings = config::BrainSettings {
            enabled: true,
            speak: false,
            ..Default::default()
        };
        app.apply_brain_settings(settings, false, cx);
        let (_, status) = std::sync::mpsc::channel();
        app.complete_brain_start(
            generation,
            "fixture",
            Ok(BrainHost {
                service: None,
                status,
                token: "fixture".into(),
            }),
            cx,
        );
        assert!(app.brain.host.is_some(), "语音开关变化不能丢弃有效启动结果");
        assert!(app.brain.starting.is_none(), "启动标记必须清除");
    });
}

/// 旧启动结果不能清除新一轮启动，也不能在用户关闭幕僚后复活服务。
/// 数据构造：第0代启动后应用一次需要重启的设置，再开始第1代。
/// 执行过程：交付旧结果，再关闭幕僚并交付新结果。
/// 预期结果：旧结果不改新 starting，关闭后两个结果都不能安装 host。
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
        app.brain.starting = Some(0);
        app.apply_brain_settings(config::BrainSettings::default(), true, cx);
        let generation = app.brain.start_generation;
        app.brain.starting = Some(generation);
        let host = || {
            let (_, status) = std::sync::mpsc::channel();
            Ok(BrainHost {
                service: None,
                status,
                token: "fixture".into(),
            })
        };
        app.complete_brain_start(0, "fixture", host(), cx);
        assert_eq!(app.brain.starting, Some(generation), "旧结果不能清除新启动");
        assert!(app.brain.host.is_none(), "旧结果不能安装 host");
        app.apply_brain_settings(config::BrainSettings::default(), true, cx);
        app.complete_brain_start(generation, "fixture", host(), cx);
        assert!(app.brain.host.is_none(), "关闭后不得安装迟到服务");
        assert!(app.brain.starting.is_none(), "关闭后不得遗留启动状态");
    });
}

/// 提议快照写失败必须让用户看见，并在路径恢复后保存最新状态。
/// 数据构造：两条提议；把保存目录位置建成普通文件，确定性造成写失败。
/// 执行过程：忽略第一条、等待真实队列执行，再推进生产轮询；修复目录并再次推进。
/// 预期结果：失败可见且不恢复已忽略条目，重试后文件只含第二条。
#[test]
fn failed_proposal_saves_are_visible_and_retried() {
    let mut test = TestAppContext::single();
    let dir = tempfile::tempdir().expect("应创建隔离目录");
    let blocker = dir.path().join("blocked");
    std::fs::write(&blocker, "fixture").expect("应制造不可写的父目录");
    let path = blocker.join("proposals.json");
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, _| Ok(Box::new(RejectTaskPty::default()))),
        )
    });
    entity.update(cx, |app, _| {
        app.brain.settings = Some(config::BrainSettings::default());
        app.brain.proposals_path = Some(path.clone());
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
        assert!(
            app.desktop_workspace.error.is_none(),
            "不能由既有错误造成假阳性"
        );
        assert!(app.decline_brain_proposal(1), "应忽略第一条并提交快照");
    });
    brain::persistence::shared_writer()
        .flush()
        .recv()
        .unwrap()
        .unwrap();
    entity.update(cx, |app, cx| {
        app.brain_tick(&[], &[], cx);
        assert!(
            app.desktop_workspace
                .error
                .as_ref()
                .is_some_and(|e| e.contains("保存幕僚提议失败")),
            "写入失败必须在界面可见"
        );
        assert_eq!(app.brain.proposals.items().len(), 1, "不能恢复已忽略提议");
    });
    std::fs::remove_file(&blocker).unwrap();
    entity.update(cx, |app, cx| {
        app.brain.proposals_retry_at = Instant::now();
        app.brain_tick(&[], &[], cx);
    });
    brain::persistence::shared_writer()
        .flush()
        .recv()
        .unwrap()
        .unwrap();
    let saved = proposals::Proposals::from_json(
        &std::fs::read_to_string(path).expect("恢复路径后必须重试保存"),
    )
    .unwrap();
    assert_eq!(saved.items().len(), 1, "只保存当前快照");
    assert_eq!(saved.items()[0].agent, "B", "已忽略的 A 不能复活");
}
