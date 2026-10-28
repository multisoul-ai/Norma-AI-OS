use canvas::{AgentProfile, CanvasRect};
use runtime::{AgentHookRuntimeConfig, Runtime};
use std::sync::{Arc, Mutex};

/// 场景：原生Claude启动前就必须拥有稳定运行身份，并安装专用问答等待Hook。
/// 构造：真实Runtime和捕获PtyCommand的后端；不启用手机服务。
/// 执行：普通桌面spawn_agent创建卡片，检查实际传给PTY的命令与Runtime身份。
/// 预期：同run注入环境，PreToolUse只匹配AskUserQuestion，PostToolUse观察消费；无审批代答。
#[test]
fn native_claude_launch_installs_question_hook_with_original_run() {
    let commands = Arc::new(Mutex::new(Vec::new()));
    let captured = commands.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp/question-launch-test");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "area".into(),
        norma_socket: "/tmp/question.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(move |command, _, _, tx| {
        captured.lock().unwrap().push(command.clone());
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let commands = commands.lock().unwrap();
    let command = commands.first().unwrap();
    let run = command
        .env
        .iter()
        .find(|(name, _)| name == "NORMA_CONTROL_RUN_ID")
        .map(|(_, value)| value.as_str());
    assert!(run.is_some(), "run必须在spawn前注入原进程环境");
    assert_eq!(
        run,
        runtime.mobile_agent_run_id(node),
        "原生启动不能在spawn后换成另一个run"
    );
    let launch = command.args.last().unwrap();
    assert!(
        launch.contains("--request-question"),
        "原生卡片必须安装可等待的专用问答Hook"
    );
    assert!(
        launch.contains("AskUserQuestion") && launch.contains("PostToolUse"),
        "必须限定题目来源并观察实际消费"
    );
    assert!(
        launch.contains("SessionStart") && launch.contains("SessionEnd"),
        "原生问题登记不可依赖全局Hook碰巧安装"
    );
    assert!(
        launch.contains("PostToolUseFailure"),
        "必须接收同一问答工具的失败与中断"
    );
    assert!(
        !launch.contains("--request-approval"),
        "问答适配不能把权限审批混入回答"
    );
}
