use canvas::AgentProfile;

/// InfCode Agent profile：画布节点必须保留稳定身份、标题和序列化值。
///
/// 数据构造（含关键数值的推导过程）：
///   serde 值：`AgentProfile::InfCode` → `"InfCode"`
///   用户可见标题：InfCode（与官方产品名一致）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 InfCode profile
///   2. 调用 `display_title`
///   3. 用 serde_json 序列化再反序列化，模拟画布持久化往返
///
/// 预期结果：
///   - 正断言：标题与序列化值为 InfCode，往返后与原值相等
///   - 负断言：不得与 OpenCode 混淆（虽为 fork，身份必须独立）
#[test]
fn infcode_agent_profile_has_stable_canvas_identity() {
    let profile = AgentProfile::InfCode;
    let serialized =
        serde_json::to_string(&profile).expect("InfCode profile 应能序列化供画布持久化");
    let restored: AgentProfile =
        serde_json::from_str(&serialized).expect("InfCode profile 应能从持久化值恢复");

    assert_eq!(
        profile.display_title(),
        "InfCode",
        "InfCode profile 应显示官方产品名"
    );
    assert_eq!(
        serialized, "\"InfCode\"",
        "InfCode profile 的持久化值必须稳定，旧库依赖该字符串"
    );
    assert_eq!(
        restored, profile,
        "InfCode profile 持久化往返后必须保持不变"
    );
    assert_ne!(
        profile,
        AgentProfile::OpenCode,
        "InfCode 虽基于 OpenCode fork，画布身份必须与 OpenCode 可区分"
    );
}
