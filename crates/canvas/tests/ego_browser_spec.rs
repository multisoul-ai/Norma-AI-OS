use canvas::{BrowserSpec, BrowserSurfaceKind, EgoCardPhase, EgoLiteSpec, NodeKind, NodeType};

/// Ego Browser 持久化兼容：旧 Browser JSON 必须默认恢复为 Native，新 Ego 数据必须完整往返。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy fields = url(1) + title(1) = 2，旧快照没有 surface 字段
///   ego task_space_id = "42"，绑定 Agent node_id = 7
///   持久化状态 = AwaitingUser，交接消息 = "请完成登录"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 反序列化不含 surface 的旧 BrowserSpec。
///   2. 构造含 EgoLiteSpec 的 BrowserSpec 并 JSON 往返。
///   3. 读取 node_type、display_title 与 Ego 持久化字段。
///
/// 预期结果：
///   - 正断言：旧 BrowserSpec 恢复为 Native，旧 url/title 不变。
///   - 正断言：Ego Task Space、Agent、阶段与消息完整保留。
///   - 正断言：Ego 节点仍通过 NodeType::Browser 分发，标题显示 Task Space 名称。
///   - 负断言：旧 Browser 不得被误恢复为 Ego，Ego 标题不得退化为普通 Browser。
#[test]
fn browser_surface_serde_preserves_legacy_native_and_ego_state() {
    let legacy_json = r#"{"url":"https://example.com","title":"Example"}"#;
    let legacy: BrowserSpec =
        serde_json::from_str(legacy_json).expect("旧 BrowserSpec JSON 应保持可反序列化");

    assert_eq!(
        legacy.surface,
        BrowserSurfaceKind::Native,
        "缺少 surface 的旧 BrowserSpec 必须默认恢复为 Native"
    );
    assert_eq!(
        legacy.url, "https://example.com",
        "旧 BrowserSpec 的 url 必须保持不变"
    );
    assert_eq!(
        legacy.title.as_deref(),
        Some("Example"),
        "旧 BrowserSpec 的 title 必须保持不变"
    );
    assert!(
        !matches!(legacy.surface, BrowserSurfaceKind::EgoLite(_)),
        "旧 BrowserSpec 不得被误恢复为 EgoLite"
    );

    let ego = NodeKind::Browser(BrowserSpec {
        url: String::new(),
        title: None,
        surface: BrowserSurfaceKind::EgoLite(EgoLiteSpec {
            task_space_id: Some("42".to_string()),
            task_space_name: Some("检查发布结果".to_string()),
            bound_agent_node_id: Some(7),
            last_phase: EgoCardPhase::AwaitingUser,
            last_message: Some("请完成登录".to_string()),
        }),
    });
    let json = serde_json::to_string(&ego).expect("Ego Browser 节点应可序列化");
    let restored: NodeKind = serde_json::from_str(&json).expect("Ego Browser 节点应可反序列化");

    assert_eq!(
        restored.node_type(),
        NodeType::Browser,
        "Ego 节点必须继续通过 NodeType::Browser 分发"
    );
    assert_eq!(
        restored.display_title(),
        "检查发布结果",
        "Ego 节点标题应优先显示 Task Space 名称"
    );
    assert_ne!(
        restored.display_title(),
        "Browser",
        "有 Task Space 名称的 Ego 节点标题不得退化为 Browser"
    );
    let NodeKind::Browser(BrowserSpec {
        surface: BrowserSurfaceKind::EgoLite(spec),
        ..
    }) = restored
    else {
        panic!("Ego JSON 往返后必须仍是 BrowserSurfaceKind::EgoLite");
    };
    assert_eq!(
        spec.task_space_id.as_deref(),
        Some("42"),
        "Ego Task Space ID 必须完整保留"
    );
    assert_eq!(
        spec.bound_agent_node_id,
        Some(7),
        "Ego 绑定 Agent node_id 必须完整保留"
    );
    assert_eq!(
        spec.last_phase,
        EgoCardPhase::AwaitingUser,
        "Ego 最后阶段必须完整保留"
    );
    assert_eq!(
        spec.last_message.as_deref(),
        Some("请完成登录"),
        "Ego 交接消息必须完整保留"
    );
}
