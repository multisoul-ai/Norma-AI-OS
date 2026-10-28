use super::*;

/// 场景：Claude /clear或/resume的SessionEnd只结束会话，不能杀掉拥有的原生PTY。
/// 构造：真实prepare→spawn→complete路径，命令换成仅本测试拥有的/bin/cat。
/// 执行：SessionStart→SessionEnd(clear/resume)，读取进程取消标记，最后显式kill_all回收。
/// 预期：会话结束不会取消进程；项目关闭仍会取消并回收拥有的进程。
#[test]
fn native_session_switch_keeps_owned_process_until_project_close() {
    for reason in ["clear", "resume"] {
        let mut runtime = Runtime::new_for_workspace("/tmp");
        let node = runtime
            .create_mobile_agent_card(
                AgentProfile::Claude,
                None,
                None,
                CanvasRect::new(0., 0., 640., 400.),
            )
            .unwrap();
        let mut launch = runtime.prepare_native_agent_terminal(node, 80, 24).unwrap();
        launch.command = PtyCommand::new("/bin/cat");
        let run = launch.run_id.clone();
        let control = launch.control.clone();
        let prepared = launch.spawn().unwrap();
        runtime
            .complete_native_agent_terminal(node, &run, prepared.into_live())
            .unwrap();
        let event = |name: &str| {
            let mut event = norma::agent_hook::parse_hook_event(norma::agent_hook::HookSource::Claude, node,
                serde_json::json!({"hook_event_name":name, "session_id":"owned-session", "reason":reason})).unwrap();
            event.control_run_id = Some(run.clone());
            event
        };
        assert!(
            runtime.observe_mobile_hook(&event("SessionStart")),
            "原生启动应绑定会话"
        );
        assert!(
            runtime.observe_mobile_hook(&event("SessionEnd")),
            "会话结束Hook应被处理"
        );
        let cancelled_by_session = control.is_cancelled();
        runtime.kill_all();
        assert!(
            !cancelled_by_session,
            "SessionEnd({reason})不能杀掉同一原生进程"
        );
        assert!(control.is_cancelled(), "项目结束仍须回收测试拥有的进程");
    }
}
