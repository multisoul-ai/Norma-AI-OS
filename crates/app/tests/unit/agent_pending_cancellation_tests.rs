use super::*;
use crate::app::agent_runtime::{
    execute_norma_adapter_command_with_source, execute_orchestration_agent_command,
};
use agent::{AdapterCommand, AgentDispatchSource, AgentInputSource};
use norma::protocol::{AgentProfile, AgentTaskReceiptStatus, CardCommand, CardCommandResult};

fn cancel_queued_realtime_task(adapter: bool) {
    let mut test = gpui::TestAppContext::single();
    let workspace = tempfile::tempdir().expect("独立项目");
    let factory: runtime::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let (app, cx) = test.add_window_view(|_, cx| MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let service = app.update(cx, |app, _| app.shared_context_service().unwrap());
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.snapshot())
        .expect("测试正式同步入口前准备共享状态");
    app.update(cx, |app, cx| {
        let created = execute_norma_adapter_command_with_source(
            app,
            None,
            cx,
            AdapterCommand::CreateAgent {
                profile: AgentProfile::Codex,
                task: Some("保留首任务".into()),
            },
            AgentInputSource::Voice,
        )
        .expect("创建唯一Agent并派发首任务");
        let first = app
            .agent_task_coordinator
            .as_ref()
            .expect("协调器")
            .tasks()
            .into_iter()
            .find(|t| Some(t.task_id) == created.task_id)
            .expect("首任务");
        let queued = execute_norma_adapter_command_with_source(
            app,
            None,
            cx,
            AdapterCommand::AssignAgentTask {
                agent: first.agent.clone(),
                task: "取消等待任务".into(),
            },
            AgentInputSource::Voice,
        )
        .expect("同Agent排队第二项任务");
        let second = app
            .agent_task_coordinator
            .as_ref()
            .expect("协调器")
            .tasks()
            .into_iter()
            .find(|t| Some(t.task_id) == queued.task_id)
            .expect("第二任务");
        assert_eq!(second.state, "queued", "第二任务必须仍在协调器FIFO中");
        assert!(
            app.pending_agent_submissions
                .task_stage(second.task_id)
                .is_none(),
            "第二任务尚未进入输入阶段，必须覆盖遗漏清理的分支"
        );
        for task in [&first, &second] {
            app.pending_agent_submissions.track_realtime_delivery(
                task,
                storage::NewMobileCommand {
                    device_id: "phone-cancel".into(),
                    operation_id: format!("op-{}", task.task_id),
                    workspace_id: "isolated-project".into(),
                    route_id: "isolated-route".into(),
                    owner_id: "test-owner".into(),
                    request_hash: format!("request-{}", task.task_id),
                },
            );
        }
        assert_eq!(
            app.pending_agent_submissions.realtime.len(),
            2,
            "取消前必须跟踪两个原操作，不能空表假通过"
        );
        if adapter {
            let receipt = execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                AdapterCommand::CancelAgentTask {
                    task_id: second.task_id,
                },
                AgentInputSource::Voice,
            )
            .expect("实时Adapter取消");
            assert_eq!(receipt.status, "succeeded", "排队取消必须是已执行回执");
        } else {
            let receipt = execute_orchestration_agent_command(
                app,
                None,
                CardCommand::AgentTaskCancel {
                    task_ref: second.task_ref.clone(),
                },
                AgentDispatchSource::norma(),
                AgentProfile::Codex,
            )
            .expect("编排命令取消");
            assert!(
                matches!(
                    receipt,
                    CardCommandResult::AgentTaskCancelled {
                        status: AgentTaskReceiptStatus::Cancelled,
                        ..
                    }
                ),
                "编排入口必须确认排队任务已取消"
            );
        }
        assert!(
            !app.pending_agent_submissions
                .realtime
                .contains_key(&second.task_id),
            "取消队列任务后必须回收原语音成果跟踪，不能无限残留"
        );
        assert!(
            app.pending_agent_submissions
                .realtime
                .contains_key(&first.task_id),
            "取消第二任务不能丢失首任务的原操作和成果基线"
        );
        assert!(
            app.pending_agent_submissions
                .task_stage(first.task_id)
                .is_some(),
            "取消第二任务不能清除首任务的待提交状态"
        );
        assert_eq!(
            app.agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .task_by_ref(&second.task_ref)
                .expect("取消回执")
                .state,
            "cancelled",
            "清理内存不能替代协调器的真实取消状态"
        );
    });
}

/// 实时Adapter取消FIFO任务时回收其成果跟踪，保留当前任务。
/// 数据构造：一个Agent、首任务pending加第二任务queued，共2条语音来源；取消后2−1=1。
/// 执行过程：正式创建/派工→登记各自来源→Adapter取消第二任务→检查协调器与来源表。
/// 预期结果：第二任务为cancelled且来源不存在；首任务来源和pending仍存在。
#[test]
fn realtime_adapter_cancellation_releases_queued_delivery() {
    cancel_queued_realtime_task(true);
}

/// 编排入口取消FIFO任务时执行同样的成果跟踪回收。
/// 数据构造：一个Agent、首任务pending加第二任务queued，共2条语音来源；取消后2−1=1。
/// 执行过程：正式创建/派工→登记各自来源→按task_ref取消第二任务→检查协调器与来源表。
/// 预期结果：第二任务为cancelled且来源不存在；首任务来源和pending仍存在。
#[test]
fn orchestration_cancellation_releases_queued_delivery() {
    cancel_queued_realtime_task(false);
}
