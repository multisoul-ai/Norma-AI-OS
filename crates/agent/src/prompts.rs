/// The raw persona consumes substantial context; retain four recent complete turns to leave
/// room for card-tool schemas and the model's next response on local providers.
pub const MAX_DIALOGUE_HISTORY_TURNS: usize = 4;

const DIALOGUE_PRODUCT_RULES: &str = r#"## Norma OS 产品运行规则

以下规则与角色 persona 共同构成系统提示词，并在与 persona 表述冲突时优先保证产品行为正确：

- 对卡片任务只可使用 `card_task` 工具派发、查询或取消；不得伪造工具结果或画布状态。
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

/// Returns the independent execution prompt for CardTask subagents.
pub fn card_task_system_prompt() -> &'static str {
    include_str!("../prompts/card_task/system.md")
}
