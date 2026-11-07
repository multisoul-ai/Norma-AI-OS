use norma::client::{decode_request_line, encode_response_line};
use norma::protocol::{BrainOp, CardCommand, CardCommandResult, WireRequest, WireResponse};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 全局幕僚漏写身份时不能退回唯一窗口，半套旧环境也不能成为有效来源。
/// 数据：无身份、仅workspace、仅route、空白workspace四种环境，socket指向隔离不存在路径。
/// 执行：运行真实 norma brain agents，不带显式作用域。
/// 预期：连接之前返回 workspace_required，不能返回 app_unavailable（已尝试连接）。
#[test]
fn missing_or_partial_brain_identity_fails_before_socket_routing() {
    let dir = tempfile::tempdir().expect("隔离不存在的socket路径");
    for (workspace, route) in [
        (None, None),
        (Some("w"), None),
        (None, Some("r")),
        (Some("  "), Some("r")),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_norma"));
        command
            .args(["brain", "--token", "test", "agents"])
            .env("NORMA_SOCKET", dir.path().join("missing.sock"))
            .env_remove("NORMA_WORKSPACE_ID")
            .env_remove("NORMA_WORKSPACE_ROUTE_ID");
        if let Some(value) = workspace {
            command.env("NORMA_WORKSPACE_ID", value);
        }
        if let Some(value) = route {
            command.env("NORMA_WORKSPACE_ROUTE_ID", value);
        }
        let output = command.output().expect("运行真实CLI");
        assert!(!output.status.success(), "缺身份必须拒绝");
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("JSON结构化错误");
        assert_eq!(
            value["error"]["code"], "workspace_required",
            "在本地拒绝身份不全，不能猜测目标或先连接：{value}"
        );
    }
}

/// 同一对全局身份参数可以分布在子命令两侧，不能因解析层级误判缺失。
/// 数据：workspace 在 agents 前/route 在后，以及相反的顺序，共两个合法请求。
/// 执行：通过真实 CLI 发请求，隔离 socket 读取最终身份。
/// 预期：两种顺序都发送 w/r，不能退出参数错误或继承 old/old-route。
#[test]
fn brain_scope_pair_can_straddle_the_subcommand() {
    for arguments in [
        ["--workspace", "w", "agents", "--route", "r"],
        ["--route", "r", "agents", "--workspace", "w"],
    ] {
        let request = capture_brain_request(&arguments);
        assert_eq!(
            request.source_workspace_id.as_deref(),
            Some("w"),
            "跨层 workspace 参数必须保留"
        );
        assert_eq!(
            request.source_workspace_route_id.as_deref(),
            Some("r"),
            "跨层 route 参数必须保留"
        );
    }
}

/// 身份参数成对出现且不可为空；已下发的身份值不被 trim 或按名称替换。
/// 数据：workspace-only、route-only、两个空白身份，以及放在子命令后的合法身份。
/// 执行：通过公开 CLI parser 分别解析上述输入。
/// 预期：四种不完整输入全部失败；合法身份保留空格，不能变成另一个项目/route。
#[test]
fn brain_scope_requires_complete_nonblank_identity_and_preserves_bytes() {
    use norma::cli::parse_invocation_from;
    for arguments in [
        vec!["norma", "brain", "--workspace", "w", "agents"],
        vec!["norma", "brain", "--route", "r", "agents"],
        vec![
            "norma",
            "brain",
            "--workspace",
            "  ",
            "--route",
            "r",
            "agents",
        ],
        vec![
            "norma",
            "brain",
            "--workspace",
            "w",
            "--route",
            "\t",
            "agents",
        ],
    ] {
        assert!(
            parse_invocation_from(arguments.clone()).is_err(),
            "不完整身份不能继承另一半环境：{arguments:?}"
        );
    }
    let parsed = parse_invocation_from([
        "norma",
        "brain",
        "agents",
        "--workspace",
        " w ",
        "--route",
        " r ",
    ])
    .expect("子命令后也支持精确作用域");
    let scope = parsed.brain_scope.expect("显式身份不得丢失");
    assert_eq!(scope.workspace_id, " w ", "workspace 身份按字节保留");
    assert_eq!(scope.route_id, " r ", "route 身份按字节保留");
}

/// 旧按项目幕僚仍通过环境传递完整身份，升级 CLI 不能中断其路由。
/// 数据：不带 CLI scope，环境为 old/old-route。
/// 执行：真实二进制向隔离 socket 请求 agents。
/// 预期：完整环境身份原样上送；不会丢弃 route 并回退到唯一窗口。
#[test]
fn legacy_brain_keeps_complete_environment_scope() {
    let request = capture_brain_request(&["agents"]);
    assert_eq!(
        request.source_workspace_id.as_deref(),
        Some("old"),
        "保留旧项目身份"
    );
    assert_eq!(
        request.source_workspace_route_id.as_deref(),
        Some("old-route"),
        "保留旧 route，服务端仍检查过期"
    );
}

/// 全局幕僚按事件显式指定作用域，不能继承启动时的另一个项目身份。
/// 数据：环境为 old/old-route，CLI 为「项目 beta」/route-beta，含空格的身份原样保留。
/// 执行：运行真实 norma 二进制，在隔离 Unix socket 捕获请求并返回成功响应。
/// 预期：请求携带 beta 的完整身份和 Agents 操作；没有旧身份、Provider 或跨空间目标。
#[test]
fn explicit_brain_scope_reaches_wire_without_inheriting_old_project() {
    let request = capture_brain_request(&[
        "--workspace",
        "项目 beta",
        "--route",
        "route-beta",
        "agents",
    ]);
    assert_eq!(
        request.source_workspace_id.as_deref(),
        Some("项目 beta"),
        "工作空间身份原样进入请求"
    );
    assert_eq!(
        request.source_workspace_route_id.as_deref(),
        Some("route-beta"),
        "route 与工作空间一起覆盖旧环境"
    );
    assert_eq!(
        request.command,
        CardCommand::Brain {
            token: "test-token".into(),
            op: Box::new(BrainOp::Agents)
        },
        "scope 不改变原操作或 token"
    );
    assert!(
        request.authorization.is_none() && request.source_node_id.is_none(),
        "幕僚不冒用 Agent 身份"
    );
    assert!(
        request.target_workspace_id.is_none(),
        "精确来源 route 不降级为按工作空间猜测目标"
    );
}

fn capture_brain_request(args: &[&str]) -> WireRequest {
    let temp = tempfile::tempdir_in("/tmp").expect("创建隔离 socket 目录");
    let socket = temp.path().join("brain.sock");
    let listener = UnixListener::bind(&socket).expect("绑定隔离 socket");
    listener
        .set_nonblocking(true)
        .expect("accept 不阻塞失败测试");
    let mut child = Command::new(env!("CARGO_BIN_EXE_norma"))
        .args(["brain", "--token", "test-token"])
        .args(args)
        .env("NORMA_SOCKET", &socket)
        .env("NORMA_WORKSPACE_ID", "old")
        .env("NORMA_WORKSPACE_ROUTE_ID", "old-route")
        .env("MULTISOUL_AGENT_PROFILE", "codex")
        .env("MULTISOUL_NODE_ID", "42")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动真实 CLI");
    // 本地 CLI 无网络；5 秒仅作故障上界，10ms 轮询不影响产品行为。
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("接收 CLI 请求失败：{error}"),
        }
        if child.try_wait().expect("查询 CLI 状态").is_some() {
            let output = child.wait_with_output().expect("收集 CLI 退出信息");
            panic!(
                "CLI 未发送请求：{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if Instant::now() >= deadline {
            child.kill().expect("终止超时测试子进程");
            child.wait().expect("回收测试子进程");
            panic!("CLI 未在 5 秒内发送请求");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("限制读取时间");
    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .expect("读取请求");
    let request = decode_request_line(&line).expect("解码 wire 请求");
    let response = WireResponse::new(
        request.request_id,
        Ok(CardCommandResult::Brain {
            output: "[]".into(),
        }),
    );
    stream
        .write_all(
            encode_response_line(&response)
                .expect("编码响应")
                .as_bytes(),
        )
        .expect("回复 CLI");
    let output = child.wait_with_output().expect("回收 CLI");
    assert!(
        output.status.success(),
        "CLI 应成功退出：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    request
}
