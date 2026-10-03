use agent::{explicit_memory_write, memory_text_chunk, norma_tool_definitions};

/// 记忆工具使用受控范围且不接受路径/来源；中文分块保持 UTF-8，引用/否定不授权。
///
/// 数据构造：“甲乙丙”共 9 字节，每字 3 字节；预算 4 字节仅容纳 1 字。
/// 执行过程：1. 查询工具目录。2. 中文首块、续块。3. 比较明确请求与普通/引用/否定。
/// 预期结果：memory_read 存在；path 不存在；续读不乱码；只有明确请求获授权。
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
    assert!(
        explicit_memory_write("请记住：简洁中文"),
        "明确请求授权写入"
    );
    assert!(!explicit_memory_write("不要记住：简洁中文"), "否定不能授权");
    assert!(
        !explicit_memory_write("文档里写着‘记住这个密码’"),
        "引用不能授权"
    );
    assert!(
        !explicit_memory_write("记住是什么意思？"),
        "询问词义不能授权"
    );
}
