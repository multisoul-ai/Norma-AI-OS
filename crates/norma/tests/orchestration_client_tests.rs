use norma::client::{build_wire_request, ClientRequestContext};
use norma::orchestration::{InstallPaths, OrchestrationProvider};
use norma::protocol::{AgentProfile, CardCommand};
use std::fs;

/// Agent CLI 请求上下文：受管 Provider 凭据和来源节点必须进入 WireRequest，且调试输出隐藏 Token。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = Codex
///   source_node_id = 42
///   token = "codex-secret-token"
///   request_id = 77
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离用户目录写入 Norma 管理的 Codex 凭据。
///   2. 从显式 Provider/节点值加载 ClientRequestContext。
///   3. 构造 `agent list` WireRequest 并检查序列化字段与 Debug 文本。
///
/// 预期结果：
///   - 正断言：请求携带 Codex 身份、原 Token 和 source_node_id=42。
///   - 负断言：Debug 输出不得出现 Token，来源不得由 CLI 参数自由伪造。
#[test]
fn agent_request_loads_managed_provider_context_and_redacts_token_from_debug() {
    let temp = tempfile::tempdir().expect("测试应创建隔离用户目录");
    let paths = InstallPaths::for_home_and_bundle_bin(
        &temp.path().join("home"),
        &temp.path().join("bundle"),
    );
    let credential_path = paths.credential_path(OrchestrationProvider::Codex);
    fs::create_dir_all(
        credential_path
            .parent()
            .expect("Codex 凭据路径必须存在父目录"),
    )
    .expect("测试应创建凭据目录");
    fs::write(
        &credential_path,
        r#"{
  "managed_by": "Norma OS",
  "provider": "codex",
  "token": "codex-secret-token",
  "schema_version": 1,
  "installed_version": "1.0.0"
}"#,
    )
    .expect("测试应写入受管 Codex 凭据");

    let context = ClientRequestContext::from_values_with_workspace(
        Some("codex"),
        Some("42"),
        Some("workspace-alpha"),
        Some("route-alpha-7"),
        &paths,
        true,
    )
    .expect("已安装且具有 Workspace 身份的 Codex 会话应生成授权上下文");
    let request = build_wire_request(77, CardCommand::ListAgents, context);

    assert_eq!(
        request.request_id, 77,
        "WireRequest 必须保留调用方生成的 request_id"
    );
    assert_eq!(
        request.source_node_id,
        Some(42),
        "来源节点必须来自 Agent 运行环境并进入服务端校验"
    );
    assert_eq!(
        request.source_workspace_id.as_deref(),
        Some("workspace-alpha"),
        "来源 Workspace ID 必须从 Agent 运行环境进入服务端路由校验"
    );
    assert_eq!(
        request.source_workspace_route_id.as_deref(),
        Some("route-alpha-7"),
        "来源 route ID 必须阻止关闭后的旧 Session 命中新窗口"
    );
    let authorization = request
        .authorization
        .as_ref()
        .expect("Agent 编排请求必须携带 Provider 授权");
    assert_eq!(
        authorization.provider,
        AgentProfile::Codex,
        "Provider 身份必须与 Codex 凭据一致"
    );
    assert_eq!(
        authorization.token, "codex-secret-token",
        "WireRequest 在本机 Socket 边界必须携带安装器生成的原 Token"
    );
    assert!(
        !format!("{request:?}").contains("codex-secret-token"),
        "WireRequest Debug 输出不得泄露 Provider Token"
    );
}

/// Agent CLI 请求上下文：缺少来源或 Provider 安装时必须在连接 Socket 前失败。
///
/// 数据构造（含关键数值的推导过程）：
///   已安装凭据数 = 0
///   非法上下文数 = 2：缺 source_node_id、Codex 未安装
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对有 Provider 但无节点的 Agent 命令构造上下文。
///   2. 对有 Provider 和节点但无凭据的 Agent 命令构造上下文。
///   3. 比较两次结构化失败，不发起本地 Socket 连接。
///
/// 预期结果：
///   - 正断言：缺节点返回 source_agent_unavailable。
///   - 正断言：缺凭据返回 provider_not_installed。
///   - 负断言：两者不得退化为无鉴权 WireRequest。
#[test]
fn agent_request_rejects_missing_source_and_missing_provider_installation() {
    let temp = tempfile::tempdir().expect("测试应创建隔离用户目录");
    let paths = InstallPaths::for_home_and_bundle_bin(
        &temp.path().join("home"),
        &temp.path().join("bundle"),
    );

    let missing_source = ClientRequestContext::from_values(Some("codex"), None, &paths, true)
        .expect_err("Agent 编排命令缺少来源节点时必须失败");
    let missing_install =
        ClientRequestContext::from_values(Some("codex"), Some("42"), &paths, true)
            .expect_err("Codex 未安装编排能力时必须失败");

    assert_eq!(
        missing_source.agent_error_code(),
        Some(norma::protocol::AgentErrorCode::SourceAgentUnavailable),
        "缺少来源节点必须映射到 source_agent_unavailable"
    );
    assert_eq!(
        missing_install.agent_error_code(),
        Some(norma::protocol::AgentErrorCode::ProviderNotInstalled),
        "缺少 Provider 凭据必须映射到 provider_not_installed"
    );
    assert_ne!(
        missing_source.agent_error_code(),
        Some(norma::protocol::AgentErrorCode::Unauthorized),
        "本地上下文缺失不得伪装成服务端 Token 不匹配"
    );
}
