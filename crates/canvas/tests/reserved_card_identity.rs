/// 场景：两个并发后台创建预留身份时，不显示节点、不启动PTY。
/// 数据：空Document，预留两张Agent节点。
/// 执行：先预留再逆序安装，重复安装被拒绝。
/// 预期：预留不增加节点数，身份唯一；安装保留原身份、node编号不会冲突。
#[test]
fn reserves_without_publishing_and_installs_original_identity() {
    let mut doc = canvas::Document::new();
    let kind = canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
        profile: canvas::AgentProfile::Codex,
        alias: None,
        definition_id: None,
        mobile_control: false,
    });
    let bounds = canvas::CanvasRect::new(0.0, 0.0, 400.0, 300.0);
    let a = doc.reserve_node(kind.clone(), bounds);
    let b = doc.reserve_node(kind, bounds);
    assert!(doc.nodes().is_empty(), "预留不能发布具名可投递卡片");
    assert_ne!(a.id, b.id, "并发创建预留不同node编号");
    assert_ne!(a.card_instance_id, b.card_instance_id, "角色身份不同");
    assert!(
        doc.install_reserved_node(b.clone()),
        "可以先安装第二个后台完成者"
    );
    assert!(doc.install_reserved_node(a.clone()), "可以后安装原预留节点");
    assert!(!doc.install_reserved_node(a.clone()), "不得覆盖已存在node");
    assert_eq!(
        doc.node(a.id).unwrap().card_instance_id,
        a.card_instance_id,
        "实例身份保持"
    );
}
