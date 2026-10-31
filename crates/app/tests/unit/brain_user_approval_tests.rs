use super::*;
use gpui::AppContext;
use norma::agent_hook::{parse_hook_event, HookSource};
use std::sync::{Arc, Mutex};

/// 场景：外层审批必须支持原生 Claude 请求，并在等待真实回执时拒绝重复预约。
/// 构造：隔离 App、100×40 的原始 PTY、单个 Bash 工具和匹配的两选项菜单。
/// 执行：观察同会话/同回合 Hook、注入真实终端输出；先证明旧接口不可用，再走桌面动作。
/// 预期：旧接口不能处理 native-*；新入口接受异步回执但不假装完成，第二次预约失败。
#[test]
fn user_approval_reserves_the_native_request_instead_of_the_legacy_hook() {
    let dir = tempfile::tempdir().unwrap();
    let output = Arc::new(Mutex::new(None));
    let producer = output.clone();
    let mut test = gpui::TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(move |_, _, _, tx| {
                *producer.lock().unwrap() = Some(tx.clone());
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
            }),
            storage::DatabasePaths::isolated_test(&dir.path().join("data")),
        )
    });
    let request = source.update(cx, |app, cx| {
        app.enter_canvas_for_test(dir.path(), cx);
        let node = app.runtime.spawn_agent(canvas::AgentProfile::Claude,
            canvas::CanvasRect::new(0., 0., 640., 400.), 100, 40).unwrap();
        let path = dir.path().join("session-a.jsonl");
        std::fs::write(&path, "").unwrap();
        let run = app.runtime.mobile_agent_run_id(node).map(str::to_owned);
        for name in ["SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest"] {
            let mut event = parse_hook_event(HookSource::Claude, node, serde_json::json!({
                "hook_event_name":name,"session_id":"session-a","prompt_id":"turn-a",
                "transcript_path":path,"tool_name":"Bash","tool_use_id":"tool-a",
                "tool_input":{"command":"printf approval","description":"Write marker"}
            })).unwrap();
            event.control_run_id = run.clone();
            assert!(app.runtime.observe_mobile_hook(&event), "同一原生会话的 {name} 应被接受");
        }
        app.runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
            node_id:node,profile:canvas::AgentProfile::Claude,provider_event:"PermissionRequest".into(),
            to:runtime::AgentTaskState::WaitingApproval { tool_name:Some("Bash".into()) },
            task_title:None,session_id:Some("session-a".into()),
        });
        output.lock().unwrap().as_ref().unwrap().send(terminal::PtyEvent::Output(
            b"\x1b[2J\x1b[HBash command\r\n printf approval\r\n Write marker\r\nDo you want to proceed?\r\n 1. Yes\r\n 2. No\r\nEsc to cancel \xc2\xb7 Tab to amend \xc2\xb7 ctrl+e to explain".to_vec()
        )).unwrap();
        app.runtime.drain_all();
        let request = app.runtime.mobile_approval_snapshots(node).unwrap().remove(0).target;
        assert!(request.request_id.starts_with("native-"), "必须构造真实原生审批身份");
        assert!(!app.runtime.decide_mobile_approval(node, &request.run_id, &request.request_id, ApprovalDecision::Allow),
            "旧 Hook 入口正是不能处理本请求的反例");
        request
    });
    let host = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            storage::DatabasePaths::isolated_test(&dir.path().join("host-data")),
        )
    });
    host.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some("w".into());
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), source.clone());
        app.refresh_brain_overview(cx);
        assert!(
            !app.brain_overview().running,
            "幕僚未运行也必须提供真实审批数据"
        );
        assert_eq!(
            app.brain_overview().approvals.len(),
            1,
            "概览必须保留具体请求，不能只剩待审核状态"
        );
        assert_eq!(
            app.brain_overview().approvals[0].1.target,
            request,
            "概览操作必须绑定同一个原生请求"
        );
        assert_eq!(
            app.overview_approval_details(
                "a",
                request.node_id,
                &request.run_id,
                &request.request_id,
                cx
            )
            .unwrap()
            .0,
            "printf approval",
            "无幕僚时展开也能看到完整命令"
        );
    });
    source.update(cx, |app, cx| {
        let receipt = app
            .decide_approval_by_user(
                request.node_id,
                &request.run_id,
                &request.request_id,
                true,
                cx,
            )
            .expect("桌面入口应预约原生请求并返回异步回执");
        assert!(
            receipt.try_recv().is_err(),
            "预约和按键不能冒充 Provider 完成回执"
        );
        assert!(
            app.decide_approval_by_user(
                request.node_id,
                &request.run_id,
                &request.request_id,
                true,
                cx
            )
            .is_err(),
            "等待回执期间不能重复预约同一原生请求"
        );
    });
}
