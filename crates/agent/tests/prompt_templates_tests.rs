use sha2::{Digest, Sha256};

/// P-1：默认 Dialogue persona 必须逐字保持用户指定的来源文本。
///
/// 数据构造：来源原文的 SHA-256 是
/// `ad3e3878679c9db3740353f97c90ab47d82078822f6e8df1875772aeccdda948`；
/// 此值由 `prompts/dialogue/norma-persona.txt` 在未作任何转码或规范化时计算得到。
/// 执行过程：通过 `agent` 的公开 prompt API 读取默认 persona，并计算其 UTF-8 字节的 SHA-256。
/// 预期结果：摘要完全相同；任何删改、换行或字符替换都会改变摘要并使测试失败。
#[test]
fn default_dialogue_persona_matches_approved_source_checksum() {
    let digest = Sha256::digest(agent::dialogue_persona().as_bytes());
    let actual = format!("{digest:x}");

    assert_eq!(
        actual, "ad3e3878679c9db3740353f97c90ab47d82078822f6e8df1875772aeccdda948",
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
/// 数据构造：CardTask 只有白名单卡片工具，且 `send_text` 默认提交；Dialogue persona 含 Norma 多人工智能助理人设。
/// 执行过程：通过公开 prompt API 读取 CardTask system prompt，并检查执行规则与 persona 隔离标记。
/// 预期结果：存在查证、创建回执和真实结果规则；不存在 Dialogue persona 的角色核心标记。
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
        prompt.contains("不生成用户可见总结"),
        "CardTask prompt 必须把用户话术交给运行时的真实回执模板"
    );
    assert!(
        !prompt.contains("Norma 多人工智能助理"),
        "CardTask prompt 不得继承 Dialogue persona 的助理人设"
    );
    assert!(
        !prompt.contains("【角色核心数据】"),
        "CardTask prompt 不得泄漏 Dialogue persona 的角色资料"
    );
}

/// P-4：Agent 委派提示词必须要求轻量润色并完整保留用户任务语义。
///
/// 数据构造（含关键数值的推导过程）：
///   prompt layers = Dialogue 产品规则 1 层 + CardTask 执行规则 1 层，共 2 层；
///   semantic fields = 目标、范围、先后顺序、约束、交付要求、明确命令，共 6 类；
///   context rule = 指代候选恰好 1 个时可展开，候选数不等于 1 时不得猜测。
///   supported Agent profiles = Codex、Claude、Cursor、OpenCode，共 4 种。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取生产 Dialogue system prompt；
///   2. 读取生产 CardTask system prompt；
///   3. 检查两层都约束轻量润色，且分别覆盖上下文指代与最终任务正文；
///   4. 检查 Dialogue 明确支持全部 4 种 Agent profile。
///
/// 预期结果：
///   - 正断言：两层都明确要求轻量润色和 6 类语义保留；
///   - 正断言：Dialogue 只允许展开唯一明确的上下文指代；
///   - 正断言：Dialogue 同时列出 Codex、Claude、Cursor、OpenCode；
///   - 负断言：CardTask 不得扩写成长篇结构化简报或新增用户未提出的要求。
#[test]
fn agent_delegation_prompts_require_light_polish_without_semantic_loss() {
    let dialogue = agent::dialogue_system_prompt();
    let card_task = agent::card_task_system_prompt();
    let semantic_fields = "目标、范围、先后顺序、约束、交付要求和明确命令";

    assert!(
        dialogue.contains("轻量润色") && dialogue.contains(semantic_fields),
        "Dialogue prompt 必须要求轻量润色并保留全部 6 类任务语义"
    );
    assert!(
        dialogue.contains("唯一明确") && dialogue.contains("不得猜测"),
        "Dialogue prompt 只有在上下文指代唯一明确时才可展开，否则必须停止猜测"
    );
    assert!(
        dialogue.contains("Codex、Claude、Cursor 或 OpenCode")
            && dialogue.contains("Codex/Claude/Cursor/OpenCode"),
        "Dialogue prompt 必须一致支持 Codex、Claude、Cursor、OpenCode 四种 Agent"
    );
    assert!(
        card_task.contains("轻量润色") && card_task.contains(semantic_fields),
        "CardTask prompt 必须把 Dialogue 意图整理成不丢语义的轻量润色任务"
    );
    assert!(
        card_task.contains("不得扩写成长篇结构化简报")
            && card_task.contains("不得新增用户未提出的要求"),
        "CardTask 不得把轻量润色退化为重型模板或臆造额外要求"
    );
}

/// P-5：Agent 委派只能做文字润色和中转，不得把任务简略误判为需要确认。
///
/// 数据构造（含关键数值的推导过程）：
///   prompt layers = Dialogue 产品规则 1 层 + CardTask 执行规则 1 层，共 2 层；
///   dispatch threshold = 目标 Agent 1 个 + 可识别动作 1 个，共 2 项最低条件；
///   allowed clarification = 核心意图无法识别时最多 1 个最小必要问题。
///   missing profile = 缺少 1 个 Agent 类型时，只询问 1 个类型问题；
///   terse answer = Agent 名称、动作或类型中的任意 1 项，可与上一轮原任务合并。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取生产 Dialogue system prompt；
///   2. 读取生产 CardTask system prompt；
///   3. 检查两层都把职责限定为润色与中转，并明确任务简短不构成澄清理由；
///   4. 检查澄清仅限目标、动作或多候选指代无法确定，且禁止反复确认；
///   5. 检查缺少 profile 时只问类型，用户简短回答后结合原任务直接派发。
///
/// 预期结果：
///   - 正断言：目标 Agent 和动作可识别时必须直接派发；
///   - 正断言：只有核心意图不明时才可提出一次最小澄清；
///   - 正断言：缺少 profile 时只问类型，简短回答无需重述原任务；
///   - 负断言：不得因缺少实现细节、背景或验收标准而拦截任务。
#[test]
fn agent_delegation_prompts_only_polish_and_relay_actionable_tasks() {
    let dialogue = agent::dialogue_system_prompt();
    let card_task = agent::card_task_system_prompt();

    assert!(
        dialogue.contains("职责仅限文字润色和中转")
            && dialogue.contains("目标 Agent 和要做的动作可以识别"),
        "Dialogue 必须把 Agent 委派限定为润色与中转，并以目标和动作可识别作为直接派发门槛"
    );
    assert!(
        dialogue.contains("不得因任务简短、缺少实现细节、背景或验收标准而确认")
            && dialogue.contains("不得反复确认"),
        "Dialogue 不得把任务不够详细误判为需要确认，也不得反复追问用户"
    );
    assert!(
        card_task.contains("只负责文字润色和中转")
            && card_task.contains("不得重新评审任务是否足够完整"),
        "CardTask 必须直接中转 Dialogue 已识别的任务，不能再次评审需求完整度"
    );
    assert!(
        card_task.contains("目标 Agent、要做的动作或多候选指代")
            && card_task.contains("一个最小必要问题"),
        "CardTask 只有在核心意图无法识别时才可提出一次最小澄清"
    );
    assert!(
        dialogue.contains("使用哪种 Agent 类型")
            && card_task.contains("使用哪种 Agent 类型")
            && dialogue.contains("不得要求用户重新完整表达")
            && card_task.contains("不得要求用户重新完整表达"),
        "缺少创建类型时只能询问一个最小问题，不能要求用户重新描述整个任务"
    );
    assert!(
        dialogue.contains("用户可只回答一个 Agent 名称、动作或类型")
            && dialogue.contains("结合上一轮原任务恢复完整意图并直接派发")
            && card_task.contains("不得要求用户复述任务"),
        "用户用 Agent 名称、动作或类型解除歧义后，必须结合原任务派发而不是要求重述"
    );
    assert!(
        !card_task.contains("先理解任务目标"),
        "CardTask 不得保留会诱导二次需求分析和过度确认的宽泛指令"
    );
}
