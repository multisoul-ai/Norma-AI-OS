use super::*;
use gpui::TestAppContext;
use runtime::{AgentTaskState, AgentTaskUpdate};

/// 正式 App 重开入口同时检查发送队列，不能只凭 Runtime 的 Done 中断排队消息。
///
/// 数据构造：一个旧 Claude echo PTY，session-a；Thinking 时排队一条回复，再把任务报告为 Done。
/// 执行过程：带排队回复重开失败 → 撤回回复 → 标记启动中再次失败 → 清理启动状态 → 错会话重开失败。
/// 预期结果：回复不会丢失，所有拒绝都保留原 PTY 和旧模式；队列清空后准确进入会话校验，没有真实 CLI 启动。
#[test]
fn mobile_reopen_preserves_queued_reply_and_pending_launch() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("隔离项目");
    let factory: runtime::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let (app, cx) = gpui.add_window_view(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    app.update(cx,|app,cx| {
        // 80×24 是标准 PTY 尺寸；definition=1 只用于此次排队回执身份。
        let id=app.runtime.spawn_agent(AgentProfile::Claude,canvas::CanvasRect::new(0.0,0.0,640.0,400.0),80,24).expect("旧卡");
        let report=|state|AgentTaskUpdate{node_id:id,profile:AgentProfile::Claude,provider_event:"Stop".into(),to:state,task_title:None,session_id:Some("session-a".into())};
        app.runtime.apply_agent_task_update(report(AgentTaskState::Thinking));
        let reply=app.enqueue_or_send_session_reply(1,"Claude".into(),id,"session-a".into(),"尚未发送的方向调整".into()).expect("排队");
        app.runtime.apply_agent_task_update(report(AgentTaskState::Done));
        let error=app.reopen_mobile_agent(id,"session-a",cx).expect_err("仍有排队消息");
        assert!(error.contains("排队消息"),"不能只查看 Done 就重开");
        assert_eq!(app.session_reply_queue.peek_reply(reply.reply_id).expect("回执保留").message,"尚未发送的方向调整","重开失败不得吞掉排队回复");
        assert!(app.session_reply_queue.cancel_reply_id(reply.reply_id).is_some(),"用户仍可撤回原消息");
        app.pending_agent_submissions.mark_starting(id);
        assert!(app.reopen_mobile_agent(id,"session-a",cx).is_err(),"启动/提交阶段不能重开");
        assert!(app.pending_agent_submissions.mark_ready(id),"失败重开不能清掉启动状态");
        let error=app.reopen_mobile_agent(id,"wrong-session",cx).expect_err("错误会话");
        assert!(error.contains("会话已变化"),"队列清空后应核对准确会话");
        assert!(app.runtime.node_view(id).is_some(),"所有拒绝都必须保留旧 PTY");
        assert!(matches!(&app.runtime.document.node(id).expect("原卡").kind,CanvasNodeKind::Agent(spec) if !spec.mobile_control),"失败前置检查不能改变控制模式");
        app.runtime.kill_all();
    });
}
