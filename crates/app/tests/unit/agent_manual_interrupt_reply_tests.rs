use super::*;
use crate::app::agent_runtime::{
    execute_agent_card_command, reply_adapter::execute_reply_adapter_command,
};
use agent::AdapterCommand;
use gpui::TestAppContext;
use norma::protocol::{CardCommand, CardCommandResult};
use std::sync::{Arc, Mutex};
use terminal::{PtyBackend, PtyError};

struct RecordingPty(Arc<Mutex<Vec<Vec<u8>>>>);

impl PtyBackend for RecordingPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.0.lock().expect("PTY 写入记录").push(bytes.to_vec());
        Ok(())
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

/// 场景：手动终止任务后，通过语音所用的 reply_agent 继续给同一卡片发指令。
/// 构造：Alan/Claude 的运行中 PTY；先 UserPromptSubmit，再走概览的真实 Ctrl+C 入口。
/// 执行：用生产 reply Adapter 发送“继续修复审批按钮”，检查回执与 PTY 原始字节。
/// 预期：同一卡片收到 ETX、完整正文、CR；回执为 sent，不能一直 queued 或只播放动画。
#[test]
fn manual_interrupt_reply_delivers_text_and_enter_to_same_card() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded = writes.clone();
    let factory: runtime::PtyFactory =
        Box::new(move |_, _, _, _| Ok(Box::new(RecordingPty(recorded.clone()))));
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("隔离项目");
    let (app, cx) = gpui.add_window_view(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let created = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, _| {
                execute_agent_card_command(
                    app,
                    window,
                    CardCommand::CreateAgent {
                        name: Some("Alan".into()),
                        profile: norma::protocol::AgentProfile::Claude,
                        activate: true,
                    },
                )
            })
        })
        .expect("创建 Agent");
    let CardCommandResult::AgentCreated {
        node_id: Some(node),
        ..
    } = created
    else {
        panic!("必须创建活跃卡片");
    };
    app.update(cx, |app, cx| {
        app.pending_agent_submissions.mark_ready(node);
        app.runtime
            .apply_agent_task_update(runtime::AgentTaskUpdate {
                node_id: node,
                profile: canvas::AgentProfile::Claude,
                provider_event: "UserPromptSubmit".into(),
                to: AgentTaskState::Thinking,
                task_title: None,
                session_id: Some("session-a".into()),
            });
        app.interrupt_agent_from_desktop_overview(node, cx)
            .expect("手动中断");
        assert_eq!(
            app.runtime.agent_task_state(node),
            Some(AgentTaskState::Ended),
            "中断只结束任务"
        );
        assert_eq!(
            app.runtime.node_run_state(node),
            Some(runtime::NodeRunState::Running),
            "CLI 必须仍运行"
        );
        let result = execute_reply_adapter_command(
            app,
            AdapterCommand::ReplyAgent {
                agent: "Alan".into(),
                message: "继续修复审批按钮".into(),
            },
        )
        .expect("续发指令");
        assert_eq!(
            result.reply_status.as_deref(),
            Some("sent"),
            "中断后新指令应真正发送，不得永久排队"
        );
        assert!(
            !app.session_reply_queue.has_pending_for_node(node),
            "不能残留不可推进的回复"
        );
        assert_eq!(
            app.runtime.document.nodes().len(),
            1,
            "续发必须复用同一卡片"
        );
        app.runtime.kill_all();
    });
    assert_eq!(
        *writes.lock().expect("PTY 记录"),
        vec![vec![3], "继续修复审批按钮".as_bytes().to_vec(), vec![13]],
        "ETX=Ctrl+C，CR=Enter；任务正文必须真实送达"
    );
}

/// 场景：用户中断当前任务时，中断前因审批等待而积压的回复也必须取消。
/// 构造：运行中的 Claude PTY + WaitingApproval，暂存一条“旧方向，不再执行”。
/// 执行：走生产概览 Ctrl+C 入口，随后推进回复队列并检查 PTY 字节和队列回执。
/// 预期：旧回复不再占位，也不得在新一轮自动发送；只有 Ctrl+C(ETX=3) 写入。
#[test]
fn manual_interrupt_reply_discards_pre_interrupt_messages() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded = writes.clone();
    let factory: runtime::PtyFactory =
        Box::new(move |_, _, _, _| Ok(Box::new(RecordingPty(recorded.clone()))));
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("隔离项目");
    let (app, cx) = gpui.add_window_view(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    app.update(cx, |app, cx| {
        let node = app
            .runtime
            .spawn_agent(
                canvas::AgentProfile::Claude,
                canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
                80,
                24,
            )
            .expect("运行中的 CLI");
        app.runtime
            .apply_agent_task_update(runtime::AgentTaskUpdate {
                node_id: node,
                profile: canvas::AgentProfile::Claude,
                provider_event: "PermissionRequest".into(),
                to: AgentTaskState::WaitingApproval {
                    tool_name: Some("Bash".into()),
                },
                task_title: None,
                session_id: Some("session-a".into()),
            });
        let receipt = app
            .enqueue_or_send_session_reply(
                1,
                "Alan".into(),
                node,
                "session-a".into(),
                "旧方向，不再执行".into(),
            )
            .expect("暂存回复");
        assert_eq!(
            receipt.status,
            AgentReplyStatus::Queued,
            "审批期间必须暂存回复"
        );
        app.interrupt_agent_from_desktop_overview(node, cx)
            .expect("手动中断");
        assert!(
            !app.session_reply_queue.has_pending_for_node(node),
            "手动终止应清理此前积压的回复"
        );
        assert!(
            app.session_reply_queue
                .peek_reply(receipt.reply_id)
                .is_none(),
            "旧回复不能占据下一轮的队首"
        );
        app.advance_session_reply_queue();
        app.runtime.kill_all();
    });
    assert_eq!(
        *writes.lock().expect("PTY 记录"),
        vec![vec![3]],
        "中断后不能继续写入旧方向"
    );
}
