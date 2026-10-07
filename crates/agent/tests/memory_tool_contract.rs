use agent::{memory_text_chunk, norma_tool_definitions};

/// 记忆工具使用受控范围且不接受路径/来源；中文分块保持 UTF-8。
///
/// 数据构造：“甲乙丙”共 9 字节，每字 3 字节；预算 4 字节仅容纳 1 字。
/// 执行过程：1. 查询工具目录。2. 中文首块、续块。
/// 预期结果：memory_read 存在；path 不存在；续读不乱码。
#[test]
fn controlled_tools_and_utf8_budget() {
    let definitions = norma_tool_definitions();
    let read = definitions
        .iter()
        .find(|tool| tool.name == "memory_read")
        .expect("应注册读取工具");
    assert!(
        read.parameters["properties"]["scope"].is_object(),
        "读取必须有受控范围"
    );
    assert!(
        read.parameters["properties"]["path"].is_null(),
        "模型不得指定文件路径"
    );
    // 4 bytes - 3 bytes per Chinese character = 1 unused byte.
    let (first, next) = memory_text_chunk("甲乙丙", 0, 4).expect("UTF8 首块");
    assert_eq!(first, "甲", "不能截断半个汉字");
    assert_eq!(next, Some(3), "续读位置必须位于字边界");
}

/// 记忆写入可指定同范围的重复主题，旧写入参数保持兼容，且模型不能传入删除路径。
///
/// 数据构造：旧调用仅含 scope/id/markdown，新调用另含 supersedes=[old-preferences]。
/// 执行过程：1. 查询生产 memory_write schema。2. 解析旧参数。3. 解析新增合并参数。
/// 预期结果：supersedes 是可选去重数组；旧调用解析为空列表；未知路径字段仍拒绝。
#[test]
fn write_schema_exposes_optional_atomic_merge_without_paths() {
    use agent::MemoryCommand;
    use serde_json::json;
    let write = norma_tool_definitions()
        .into_iter()
        .find(|tool| tool.name == "memory_write")
        .expect("写入工具");
    assert_eq!(
        write.parameters["properties"]["supersedes"]["type"], "array",
        "合并应接收主题 ID 列表"
    );
    assert_eq!(
        write.parameters["properties"]["supersedes"]["uniqueItems"], true,
        "删除目标不能重复"
    );
    assert_eq!(
        write.parameters["required"],
        json!(["scope", "id", "markdown"]),
        "旧调用不能被迫指定删除目标"
    );
    let old: MemoryCommand = serde_json::from_value(
        json!({"action":"write", "scope":"personal", "id":"preferences", "markdown":"中文"}),
    )
    .expect("旧格式兼容");
    assert!(
        matches!(old, MemoryCommand::Write { supersedes, .. } if supersedes.is_empty()),
        "省略 supersedes 时只能正常写入"
    );
    let new: MemoryCommand = serde_json::from_value(json!({"action":"write", "scope":"personal", "id":"preferences", "markdown":"中文", "supersedes":["old-preferences"]})).expect("新格式可解析");
    assert!(
        matches!(new, MemoryCommand::Write { supersedes, .. } if supersedes == ["old-preferences"]),
        "重复主题 ID 必须传递给宿主"
    );
    assert!(serde_json::from_value::<MemoryCommand>(json!({"action":"write", "scope":"personal", "id":"preferences", "markdown":"中文", "path":"/tmp/other"})).is_err(), "模型不得通过额外路径绕过范围授权");
}
