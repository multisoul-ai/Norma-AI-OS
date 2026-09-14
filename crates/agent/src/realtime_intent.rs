use crate::AdapterCommand;

/// Realtime speech tools use the server-observed user turn, never a model's claimed authorization.
/// This gate supplements project permissions and the adapter's exact-target deletion checks.
pub fn realtime_command_authorized(command: &AdapterCommand, input: &str) -> bool {
    if matches!(command, AdapterCommand::FindAgentTasks { .. }) {
        return true;
    }
    let input = input.trim().to_lowercase();
    if input.is_empty()
        || input.len() > 32 * 1024
        || input.contains(['?', '？'])
        || [
            "不要",
            "先别",
            "不用",
            "不需要",
            "要不要",
            "是否",
            "如果",
            "假如",
            "之前",
            "先讨论",
            "先分析",
            "先解释",
            "什么意思",
            "是什么",
            "可以吗",
            "好吗",
            "怎么",
            "如何",
            "能否",
            "能不能",
            "了吗",
            "吗",
            "呢",
            "don't",
            "do not",
            "should we",
            "would you",
            "what if",
        ]
        .iter()
        .any(|phrase| input.contains(phrase))
    {
        return false;
    }
    // ponytail: conservative imperative prefixes cover the current Chinese/English speech UI;
    // ambiguous phrasing stays a draft. Replace with a validated semantic gate if rejection
    // examples justify it; never let a model-supplied boolean bypass the original user turn.
    let mut action = input.as_str();
    loop {
        let next = [
            "请",
            "帮我",
            "麻烦",
            "现在",
            "立即",
            "马上",
            "好的，",
            "可以，",
            "please ",
        ]
        .iter()
        .find_map(|prefix| action.strip_prefix(prefix));
        match next {
            Some(next) => action = next.trim_start(),
            None => break,
        }
    }
    if let Some(delegated) = action.strip_prefix("让") {
        let target = match command {
            AdapterCommand::CreateAgent { profile, .. } => {
                crate::environment::profile_name(*profile).to_lowercase()
            }
            AdapterCommand::AssignAgentTask { agent, .. }
            | AdapterCommand::ReplyAgent { agent, .. } => agent.trim().to_lowercase(),
            _ => return false,
        };
        if target.is_empty() {
            return false;
        }
        let Some(remainder) = delegated.trim_start().strip_prefix(&target) else {
            return false;
        };
        action = remainder.trim_start();
    }
    if matches!(
        command,
        AdapterCommand::CancelAgentTask { .. } | AdapterCommand::CancelAgentReply { .. }
    ) && (["汇报", "播报", "报告", "report"]
        .iter()
        .any(|word| action.contains(word))
        || !["任务", "执行", "回复", "task", "agent", "reply"]
            .iter()
            .any(|target| action.contains(target)))
    {
        return false;
    }
    let verbs: &[&str] = match command {
        AdapterCommand::CancelAgentTask { .. } | AdapterCommand::CancelAgentReply { .. } => {
            &["取消", "停止", "终止", "cancel ", "stop "]
        }
        AdapterCommand::DeleteCanvasCard { .. }
        | AdapterCommand::DeleteCanvasCardAtPosition { .. }
        | AdapterCommand::DeleteAgentCard { .. } => {
            &["删除", "移除", "关闭", "delete ", "remove ", "close "]
        }
        AdapterCommand::FocusCard { .. } => &["聚焦", "切到", "打开", "focus ", "show "],
        _ => &[
            "实现",
            "修复",
            "创建",
            "新建",
            "安排",
            "执行",
            "开始执行",
            "继续执行",
            "继续实现",
            "继续修复",
            "修改",
            "调整",
            "添加",
            "测试",
            "运行",
            "重构",
            "implement ",
            "fix ",
            "create ",
            "build ",
            "run ",
            "continue implementing ",
            "continue fixing ",
        ],
    };
    verbs.iter().any(|verb| {
        action.strip_prefix(verb).is_some_and(|target| {
            // Nominal phrases such as “实现方案不错” are discussion, not imperatives.
            ![
                "方案", "思路", "结果", "报告", "建议", "方式", "原因", "步骤", "流程", "的",
            ]
            .iter()
            .any(|noun| target.trim_start().starts_with(noun))
        })
    })
}
