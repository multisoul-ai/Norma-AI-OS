use super::*;
use norma::orchestration::{
    load_provider_authorization, AggregateStatus, AuthorizationRegistry, CliHomeOverrides,
    InstallPaths, OrchestrationInstaller, OrchestrationProvider,
};
use norma::protocol::WireRequest;
use storage::{OrchestrationAuthorizationCache, OrchestrationAuthorizationStore};

struct Registry(OrchestrationAuthorizationStore);

impl AuthorizationRegistry for Registry {
    type Error = storage::StorageError;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        version: &str,
    ) -> Result<(), Self::Error> {
        self.0.authorize(provider.slug(), token, version)
    }

    fn revoke(&mut self, provider: OrchestrationProvider) -> Result<(), Self::Error> {
        self.0.revoke(provider.slug())
    }

    fn is_authorized(
        &self,
        provider: OrchestrationProvider,
        token: &str,
    ) -> Result<bool, Self::Error> {
        self.0.is_authorized(provider.slug(), token)
    }
}

fn list_request(authorization: ProviderAuthorization, source_node_id: NodeId) -> WireRequest {
    WireRequest {
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        request_id: 1,
        operation_id: None,
        authorization: Some(authorization),
        source_workspace_id: None,
        source_workspace_route_id: None,
        target_workspace_id: None,
        source_node_id: Some(source_node_id),
        command: CardCommand::ListAgents,
    }
}

/// 场景：已安装 Grok 的有效凭据可经过生产服务端鉴权，错误、冒用及撤销的凭据不可用。
/// 数据构造：隔离 home/bundle/SQLite；安装 Grok，读取真实生成的 token；创建一个 Grok 来源节点。
/// 执行过程：加载安装授权快照，经 execute_card_command 查询 agent list；再依次用错误 token、
/// 已授权 Claude token 冒用 Grok 节点、卸载后原 Grok token 重试（共四次请求）。
/// 预期：有效请求只列出 Grok 来源；后三次分别为 Unauthorized、SourceAgentUnavailable、Unauthorized。
#[test]
fn installed_grok_orchestration_authorization_validates_token_source_and_revocation() {
    let temp = tempfile::tempdir().expect("应创建隔离安装目录");
    let bundle = temp.path().join("bundle");
    std::fs::create_dir_all(&bundle).expect("应创建 bundle");
    for name in ["norma", "multisoul-agent-hook"] {
        std::fs::write(bundle.join(name), name).expect("应写入安装器二进制夹具");
    }
    let paths = InstallPaths::for_home_bundle_bin_and_env(
        temp.path(),
        &bundle,
        &CliHomeOverrides::default(),
    );
    let mut registry = Registry(OrchestrationAuthorizationStore::new(
        temp.path().join("authorization.sqlite"),
    ));
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let installed = installer
        .install_or_repair(OrchestrationProvider::Grok, &mut registry)
        .expect("应完成 Grok 安装与授权");
    assert_eq!(
        installed.aggregate_status,
        AggregateStatus::Ready,
        "安装必须就绪"
    );
    let authorization = load_provider_authorization(&paths, OrchestrationProvider::Grok)
        .expect("应读取安装器生成的 Grok 凭据")
        .expect("已安装 Grok 应存在受管凭据");
    registry
        .0
        .authorize("claude", "claude-token", "1.0.0")
        .expect("应登记冒用测试凭据");
    let mut gpui = gpui::TestAppContext::single();
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_cmd, _cols, _rows, tx| {
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
            }),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(temp.path(), cx);
        app.orchestration_authorizations = OrchestrationAuthorizationCache::new(
            registry.0.list().expect("应加载已安装 Provider 的授权快照"),
        );
    });
    let source = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            let created = crate::app::execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("GrokSource".into()),
                    profile: AgentProfile::Grok,
                    activate: true,
                },
            )
            .expect("应激活 Grok 来源节点");
            let CardCommandResult::AgentCreated {
                node_id: Some(node_id),
                ..
            } = created
            else {
                panic!("激活 Grok 应返回来源节点");
            };
            node_id
        })
    });
    let mut query = |authorization| {
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.execute_card_command(list_request(authorization, source), Some(window), cx)
            })
        })
    };
    let valid = query(authorization.clone());
    assert!(
        matches!(valid.result, Ok(CardCommandResult::AgentsListed { ref agents })
            if agents.len() == 1 && agents[0].name == "GrokSource"),
        "有效 Grok 凭据必须通过生产服务端并列出来源 Agent：{valid:?}"
    );
    for (auth, expected) in [
        (
            ProviderAuthorization {
                provider: AgentProfile::Grok,
                token: "wrong-token".into(),
            },
            AgentErrorCode::Unauthorized,
        ),
        (
            ProviderAuthorization {
                provider: AgentProfile::Claude,
                token: "claude-token".into(),
            },
            AgentErrorCode::SourceAgentUnavailable,
        ),
    ] {
        let response = query(auth);
        assert!(
            matches!(&response.result, Err(CardCommandError::Agent(failure)) if failure.code == expected),
            "错误凭据或冒用来源必须被拒绝为 {expected:?}：{response:?}"
        );
        assert!(response.result.is_err(), "非法请求不得返回成功列表");
    }
    installer
        .uninstall(OrchestrationProvider::Grok, &mut registry)
        .expect("应卸载并撤销 Grok");
    app.update(cx, |app, _cx| {
        app.orchestration_authorizations = OrchestrationAuthorizationCache::new(
            registry.0.list().expect("应刷新撤销后的授权快照"),
        );
    });
    let revoked = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.execute_card_command(list_request(authorization, source), Some(window), cx)
        })
    });
    assert!(
        matches!(revoked.result, Err(CardCommandError::Agent(ref failure))
            if failure.code == AgentErrorCode::Unauthorized),
        "已卸载 Grok 的旧凭据不得继续通过服务端：{revoked:?}"
    );
}
