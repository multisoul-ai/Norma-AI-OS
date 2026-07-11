use sha2::{Digest, Sha256};

/// P-1：默认 Dialogue persona 必须逐字保持用户指定的来源文本。
///
/// 数据构造：来源原文的 SHA-256 是
/// `56ff8153df1cfa1d103b67b314391f8aadd5c85ac38ba2c533617ce323e19e5d`；
/// 此值由原始 GitHub raw 文件在未作任何转码或规范化时计算得到。
/// 执行过程：通过 `agent` 的公开 prompt API 读取默认 persona，并计算其 UTF-8 字节的 SHA-256。
/// 预期结果：摘要完全相同；任何删改、换行或字符替换都会改变摘要并使测试失败。
#[test]
fn default_dialogue_persona_matches_approved_source_checksum() {
    let digest = Sha256::digest(agent::dialogue_persona().as_bytes());
    let actual = format!("{digest:x}");

    assert_eq!(
        actual, "56ff8153df1cfa1d103b67b314391f8aadd5c85ac38ba2c533617ce323e19e5d",
        "Dialogue persona 必须与用户指定来源逐字一致，实际 SHA-256：{actual}"
    );
}

/// P-2：Dialogue system prompt 必须在不改写 persona 的前提下附加产品运行约束。
///
/// 数据构造：组合 prompt 由固定产品层与 P-1 已校验的 persona 源文本组成。
/// 执行过程：通过公开组合 API 读取最终 Dialogue system prompt，并分别查找产品层和完整 persona。
/// 预期结果：产品层要求任务状态如实表达；persona 完整嵌入；最终 prompt 不得只返回其中任一部分。
#[test]
fn dialogue_system_prompt_combines_product_rules_with_full_persona() {
    let prompt = agent::dialogue_system_prompt();

    assert!(
        prompt.contains("## Norma OS 产品运行规则"),
        "组合后的 Dialogue prompt 必须包含固定产品运行层"
    );
    assert!(
        prompt.contains("不得把 queued 或 running 的任务说成已完成"),
        "产品运行层必须约束任务状态的真实性"
    );
    assert!(
        prompt.contains("必须立即调用 `card_task`") && prompt.contains("不得要求用户二次确认"),
        "明确且信息充分的画布操作必须立即派发，不能继续追问确认"
    );
    assert!(
        prompt.contains("创建终端") && prompt.contains("创建 Codex"),
        "产品规则必须给模型提供明确创建类任务的工具选择锚点"
    );
    assert!(
        prompt.contains(agent::dialogue_persona()),
        "组合后的 Dialogue prompt 必须完整保留 persona 源文本"
    );
    assert_ne!(
        prompt,
        agent::dialogue_persona(),
        "组合后的 Dialogue prompt 不得退化为只有 persona、缺少产品运行规则"
    );
}

/// P-3：CardTask 必须使用独立、严谨的执行型 prompt，不能继承 Dialogue persona。
///
/// 数据构造：CardTask 只有白名单卡片工具，且 `send_text` 默认提交；Dialogue persona 含 QQ 私聊角色设定。
/// 执行过程：通过公开 prompt API 读取 CardTask system prompt，并检查执行规则与 persona 隔离标记。
/// 预期结果：存在查证、创建回执和真实结果规则；不存在 QQ 私聊或角色核心数据等 persona 内容。
#[test]
fn card_task_system_prompt_is_rigorous_and_persona_isolated() {
    let prompt = agent::card_task_system_prompt();

    assert!(
        prompt.contains("## Norma OS CardTask 执行规则"),
        "CardTask prompt 必须声明独立执行职责"
    );
    assert!(
        prompt.contains("既有卡片"),
        "CardTask prompt 必须要求既有卡片先查证目标状态"
    );
    assert!(
        prompt.contains("Created 回执"),
        "CardTask prompt 必须允许新建卡片直接使用创建回执的 node_id"
    );
    assert!(
        prompt.contains("默认 `submit=true`"),
        "CardTask prompt 必须保留用户确认的自动提交策略"
    );
    assert!(
        prompt.contains("不得把意图当作成功"),
        "CardTask prompt 必须要求最终摘要基于真实工具回执"
    );
    assert!(
        !prompt.contains("QQ私聊环境"),
        "CardTask prompt 不得继承 Dialogue persona 的 QQ 私聊设定"
    );
    assert!(
        !prompt.contains("角色核心数据"),
        "CardTask prompt 不得泄漏 Dialogue persona 的角色资料"
    );
}
