use canvas::AgentProfile;

/// Grok Build Agent profile：画布节点必须保留稳定身份、标题和序列化值。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent profile 数量：现有 4（Claude/Codex/Cursor/OpenCode）+ Grok 1 = 5
///   Grok serde 值：枚举 variant `Grok` → JSON 字符串 `"Grok"`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 `AgentProfile::Grok` → 作为 Agent 节点的稳定运行时身份
///   2. 调用 `display_title` → 生成用户可见标题
///   3. 使用 serde_json 序列化 → 生成画布持久化值
///
/// 预期结果：
///   - 正断言：标题为 `Grok`
///   - 正断言：序列化值为 `"Grok"`
///   - 负断言：标题不得退化为 `Terminal`
#[test]
fn grok_agent_profile_has_stable_canvas_identity() {
    let profile = AgentProfile::Grok;
    let serialized =
        serde_json::to_string(&profile).expect("Grok Agent profile 应能序列化供画布持久化");

    assert_eq!(
        profile.display_title(),
        "Grok",
        "Grok Agent profile 应显示官方产品名称 Grok"
    );
    assert_eq!(
        serialized, "\"Grok\"",
        "Grok Agent profile 的持久化值必须稳定为枚举名 Grok"
    );
    assert_ne!(
        profile.display_title(),
        "Terminal",
        "Grok Agent 不得退化显示为普通 Terminal"
    );
}
