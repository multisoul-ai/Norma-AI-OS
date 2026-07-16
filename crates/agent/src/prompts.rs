/// The raw persona consumes substantial context; retain four recent complete turns to leave
/// room for card-tool schemas and the model's next response on local providers.
pub const MAX_DIALOGUE_HISTORY_TURNS: usize = 4;

const DIALOGUE_PRODUCT_RULES: &str = r#"## Norma OS 产品运行规则

以下规则与角色 persona 共同构成系统提示词，并在与 persona 表述冲突时优先保证产品行为正确：

- 对卡片任务只可使用 `card_task` 工具派发、查询或取消；不得伪造工具结果或画布状态。
- 当前项目的 `<agent-roster>` 是 Agent 寻址的唯一事实源。只有“Alan，…”、“让/请/交给/打开 Alan …”等明确呼叫或明确分工才派发；普通提及名称不得触发执行。
- Agent 委派的 `card_task.intent` 只做轻量润色：移除已由寻址承载的呼叫词和口语冗余、调整语序，但必须保留用户原意中的目标、范围、先后顺序、约束、交付要求和明确命令。它是执行载荷，不受角色 persona 的用户可见回复字数限制；不得摘要化删减、扩写成长篇结构化简报或新增要求。
- “刚才那个问题”这类上下文指代只有在候选唯一明确时才可展开；存在多个候选或缺少会改变执行结果的关键信息时必须澄清，不得猜测。任务信息充分时不要求确认，直接派发。
- 创建 Agent 必须有 Codex、Claude 或 OpenCode 类型；类型缺失时简短要求用户重新完整表达，不得静默默认。
- 多 Agent 指令只有每个 Agent 都有独立任务正文时才派发。不得把 queued、submitted 或 untracked 回执说成任务已完成。
- 当用户明确要求创建终端、创建 Codex/Claude/OpenCode Agent 卡片，或执行其他信息充分的画布操作时，必须立即调用 `card_task`；不得只给操作建议，也不得要求用户二次确认。
- 只有缺少会改变实际执行结果的关键信息时才可澄清；“创建一个终端卡片”本身信息充分，必须直接派发。
- 不得把 queued 或 running 的任务说成已完成；只有收到成功终态事件后才能表达任务完成。
- 面对不完整、冲突或无法验证的信息，应明确说明限制，不得编造事实。
- 保持对话上下文中的工具调用与工具回执一致；不要承诺尚未执行的副作用。
"#;

/// Returns the immutable source text for Norma's default Dialogue persona.
pub fn dialogue_persona() -> &'static str {
    include_str!("../prompts/dialogue/norma-persona.txt")
}

/// Builds the full system prompt for a Dialogue request.
pub fn dialogue_system_prompt() -> String {
    format!(
        "{DIALOGUE_PRODUCT_RULES}\n## 不可变角色 persona\n\n{}",
        dialogue_persona()
    )
}

pub fn dialogue_system_prompt_with_roster(roster: &str) -> String {
    if roster.is_empty() {
        dialogue_system_prompt()
    } else {
        format!(
            "{}\n\n## 当前项目 Agent 名册\n\n{roster}",
            dialogue_system_prompt()
        )
    }
}

/// Returns the independent execution prompt for CardTask subagents.
pub fn card_task_system_prompt() -> &'static str {
    include_str!("../prompts/card_task/system.md")
}

pub fn card_task_system_prompt_with_roster(roster: &str) -> String {
    if roster.is_empty() {
        card_task_system_prompt().to_string()
    } else {
        format!(
            "{}\n\n### 当前项目 Agent 名册\n\n{roster}",
            card_task_system_prompt()
        )
    }
}
