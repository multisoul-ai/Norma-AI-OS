/// 场景：已命名持久卡尚未启动，后台launch准备不产生输入或真实PTY。
/// 数据：OpenCode普通卡片，通过Document预留后安装原身份。
/// 执行：准备registered launch，再次准备同节点应拒绝。
/// 预期：prepare只保留取消与运行指纹，未创建live terminal，也不能覆盖实例。
#[test]
fn prepares_registered_plain_card_without_live_terminal() {
    let mut rt = runtime::Runtime::new();
    let node = rt.document.reserve_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::OpenCode,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 400.0, 300.0),
    );
    let id = node.id;
    assert!(rt.document.install_reserved_node(node), "安装原已登记实例");
    let launch = rt.prepare_registered_agent_terminal(id, 80, 24).unwrap();
    assert!(!launch.run_id().is_empty(), "后台launch有独立运行身份");
    assert!(rt.node_view(id).is_none(), "准备阶段不创建PTY/live视图");
    assert!(
        rt.prepare_registered_agent_terminal(id, 80, 24).is_err(),
        "相同运行不可重复启动"
    );
}
