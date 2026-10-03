use super::MemoryEntryId;

/// 主题标识的公开构造与反序列化入口都拒绝路径逃逸及保留目录名称。
///
/// 数据构造：
///   - 合法 ID 为 user-preferences；危险 ID 分别为 ../outside、/tmp/outside、memory。
///   - JSON 字符串“../outside”模拟通过协议绕过构造器。
///
/// 执行过程：
///   1. 调用 parse 解析合法 ID 与三个危险 ID。
///   2. 通过 serde_json 解析危险 ID，验证仍走相同校验。
///
/// 预期结果：
///   - 正断言：合法标识保留原值。
///   - 负断言：相对/绝对路径、目录保留名和危险 JSON 都不能形成 MemoryEntryId。
#[test]
fn entry_identity_rejects_paths_through_both_public_inputs() {
    assert_eq!(
        MemoryEntryId::parse("user-preferences")
            .expect("合法 ID 应可解析")
            .as_str(),
        "user-preferences",
        "合法主题 ID 应保持原值"
    );
    assert!(
        MemoryEntryId::parse("../outside").is_err(),
        "相对路径穿越不得成为主题 ID"
    );
    assert!(
        MemoryEntryId::parse("/tmp/outside").is_err(),
        "绝对路径不得成为主题 ID"
    );
    assert!(
        MemoryEntryId::parse("memory").is_err(),
        "生成目录的保留名称不得成为主题 ID"
    );
    assert!(
        serde_json::from_str::<MemoryEntryId>("\"../outside\"").is_err(),
        "反序列化不得绕过路径校验"
    );
}

/// ID 字节边界明确，大小写和平台路径分隔符不能引入文件别名。
///
/// 数据构造：
///   - 最大 ID = 128 个 ASCII 'a'；文件名长度 = 128 + '.md'(3) = 131 < 255。
///   - 过长 ID = 128 + 1 = 129 字节；大写和反斜线候选分别为 Preferences、a\\b。
///
/// 执行过程：
///   1. 分别解析边界内和边界外标识。
///   2. 验证大写和反斜线候选被拒绝。
///
/// 预期结果：
///   - 正断言：128 字节主题标识可用。
///   - 负断言：129 字节、大写别名和反斜线路径不可用。
#[test]
fn entry_identity_enforces_filename_budget_and_case_stability() {
    let maximum = "a".repeat(128); // 最大标识 128 字节，扩展名再占 3 字节。
    let oversized = "a".repeat(129); // 最大值 128 + 1 字节，应拒绝。
    assert!(
        MemoryEntryId::parse(&maximum).is_ok(),
        "边界内 128 字节 ID 应可用"
    );
    assert!(
        MemoryEntryId::parse(&oversized).is_err(),
        "超出 ID 字节预算应拒绝"
    );
    assert!(
        MemoryEntryId::parse("Preferences").is_err(),
        "大小写不敏感文件系统上不能形成别名"
    );
    assert!(
        MemoryEntryId::parse("a\\b").is_err(),
        "反斜线不能形成路径分隔或平台歧义"
    );
}
