use canvas::AgentProfile;

/// Kimi、ZCode、Pi、Hermes Agent profile：画布节点必须保留稳定身份、标题和序列化值。
///
/// 数据构造（含关键数值的推导过程）：
///   新增 profile 4 个：Kimi / ZCode / Pi / Hermes
///   serde 值：枚举 variant 名 → 同名 JSON 字符串，例如 `AgentProfile::Kimi` → `"Kimi"`
///   用户可见标题：与官方产品短名一致（Kimi、ZCode、Pi、Hermes）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐个构造新增 profile
///   2. 调用 `display_title` 生成用户可见标题
///   3. 用 serde_json 序列化再反序列化，模拟画布持久化往返
///
/// 预期结果：
///   - 正断言：标题与序列化值与上表一致，往返后与原值相等
///   - 负断言：新增 profile 互不相等，且与既有 Grok 不相等
#[test]
fn new_cli_agent_profiles_have_stable_canvas_identity() {
    let expected = [
        (AgentProfile::Kimi, "Kimi"),
        (AgentProfile::ZCode, "ZCode"),
        (AgentProfile::Pi, "Pi"),
        (AgentProfile::Hermes, "Hermes"),
    ];

    for (profile, title) in expected {
        let serialized = serde_json::to_string(&profile)
            .unwrap_or_else(|error| panic!("{title} profile 应能序列化供画布持久化: {error}"));
        let restored: AgentProfile = serde_json::from_str(&serialized)
            .unwrap_or_else(|error| panic!("{title} profile 应能从持久化值恢复: {error}"));

        assert_eq!(
            profile.display_title(),
            title,
            "{title} profile 应显示官方产品短名"
        );
        assert_eq!(
            serialized,
            format!("\"{title}\""),
            "{title} profile 的持久化值必须稳定，旧库依赖该字符串"
        );
        assert_eq!(
            restored, profile,
            "{title} profile 持久化往返后必须保持不变"
        );
        assert_ne!(
            profile,
            AgentProfile::Grok,
            "{title} profile 不得与既有 Grok 混淆"
        );
    }

    assert_ne!(
        AgentProfile::Pi,
        AgentProfile::Kimi,
        "不同新增 profile 之间必须可区分"
    );
}
