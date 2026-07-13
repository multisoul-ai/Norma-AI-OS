use agent::{builtin_agent_names, AgentName, AgentNameRegistry};

/// AN-1：Agent 呼叫名应接受规格内的纯英文与纯中文名称，并产生稳定匹配键。
///
/// 数据构造（含关键数值的推导过程）：
///   english = "Alan"，字符数 = 4，满足英文 2..=16 个 ASCII 字母
///   chinese = "小安"，字符数 = 2，满足中文 2..=8 个汉字
///   英文匹配键应按大小写不敏感规则得到 "alan"；中文匹配键保持 "小安"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. AgentName::parse("Alan") → 校验纯英文并保存显示名与小写匹配键
///   2. AgentName::parse("小安") → 校验纯中文并保存原文匹配键
///
/// 预期结果：
///   - 正断言：英文显示名为 Alan，匹配键为 alan
///   - 正断言：中文显示名和匹配键均为小安
///   - 负断言：英文匹配键不应保留大写 Alan，否则大小写不同会形成两个目标
#[test]
fn agent_name_accepts_supported_english_and_chinese_names() {
    let english = AgentName::parse("Alan").expect("Alan 满足 4 个 ASCII 字母，应是合法英文名");
    let chinese = AgentName::parse("小安").expect("小安满足 2 个汉字，应是合法中文名");

    assert_eq!(
        english.display(),
        "Alan",
        "英文 Agent 名应保留用户输入的显示大小写"
    );
    assert_eq!(
        english.match_key(),
        "alan",
        "英文 Agent 名应生成大小写不敏感的匹配键"
    );
    assert_ne!(
        english.match_key(),
        "Alan",
        "英文匹配键不应保留显示名的大写形式"
    );
    assert_eq!(chinese.display(), "小安", "中文 Agent 名应保留原始显示文本");
    assert_eq!(
        chinese.match_key(),
        "小安",
        "中文 Agent 名应按原文生成精确匹配键"
    );
}

/// AN-2：内置名称目录应固定提供 30 组英文正式名与中文语音别名。
///
/// 数据构造（含关键数值的推导过程）：
///   规格名称对数量 = 30
///   首项 = Alan / 艾伦，末项 = Rowan / 罗文
///   可自动分配索引范围 = 0..29，共 30 个候选
///
/// 执行过程（逐步说明系统如何处理）：
///   1. builtin_agent_names() → 返回固定顺序的只读目录
///   2. 读取索引 0 和索引 29 → 验证目录顺序及双语映射
///
/// 预期结果：
///   - 正断言：目录恰好有 30 项，首项和末项符合规格
///   - 正断言：Alan 的中文语音别名为艾伦，Rowan 的中文语音别名为罗文
///   - 负断言：首项不应误用末项 Rowan，避免自动分配顺序漂移
#[test]
fn builtin_agent_name_catalog_has_fixed_bilingual_entries() {
    let catalog = builtin_agent_names();

    assert_eq!(
        catalog.len(),
        30,
        "内置 Agent 名称目录必须恰好提供规格约定的 30 个候选"
    );
    assert_eq!(
        catalog[0].name(),
        "Alan",
        "固定顺序的第一个自动名称必须是 Alan"
    );
    assert_eq!(
        catalog[0].speech_alias(),
        "艾伦",
        "Alan 的预设中文语音别名必须是艾伦"
    );
    assert_ne!(
        catalog[0].name(),
        "Rowan",
        "首项不能是末项 Rowan，否则自动分配顺序已被反转"
    );
    assert_eq!(
        catalog[29].name(),
        "Rowan",
        "固定顺序的第 30 个自动名称必须是 Rowan"
    );
    assert_eq!(
        catalog[29].speech_alias(),
        "罗文",
        "Rowan 的预设中文语音别名必须是罗文"
    );
}

/// AN-3：自动命名应同时避开正式名称与内置中文语音别名占用。
///
/// 数据构造（含关键数值的推导过程）：
///   fixed order 前 3 项 = Alan、Bella、Bruno
///   occupied = ["艾伦", "Bella"]，共占用 2 个候选的匹配空间
///   Alan 的键集合 = ["alan", "艾伦"]，因“艾伦”冲突必须跳过
///   Bella 的键集合 = ["bella", "贝拉"]，因“Bella”冲突必须跳过
///
/// 执行过程（逐步说明系统如何处理）：
///   1. AgentNameRegistry::from_names 构建当前项目的占用键集合
///   2. next_available_builtin() 按固定目录依次预检 Alan、Bella、Bruno
///   3. 前两项冲突，第三项无冲突 → 返回 Bruno
///
/// 预期结果：
///   - 正断言：下一个名称是 Bruno，语音别名是布鲁诺
///   - 负断言：不能返回 Alan，否则中文别名唯一性未生效
///   - 负断言：不能返回 Bella，否则英文大小写不敏感占用未生效
#[test]
fn builtin_allocator_skips_canonical_and_speech_alias_conflicts() {
    let registry = AgentNameRegistry::from_names(["艾伦", "Bella"])
        .expect("艾伦与 Bella 都符合 Agent 名称规则，应能建立占用表");
    let allocated = registry
        .next_available_builtin()
        .expect("仅占用 2 个候选时，名称池还应有 28 个可用名称");

    assert_eq!(
        allocated.name(),
        "Bruno",
        "Alan 与 Bella 被其正式名或别名占用后，应分配固定顺序中的 Bruno"
    );
    assert_eq!(
        allocated.speech_alias(),
        "布鲁诺",
        "Bruno 应携带固定中文语音别名布鲁诺"
    );
    assert_ne!(
        allocated.name(),
        "Alan",
        "艾伦已占用 Alan 的预设语音别名，不能再次分配 Alan"
    );
    assert_ne!(
        allocated.name(),
        "Bella",
        "Bella 正式名称已占用，不能再次分配 Bella"
    );
}
