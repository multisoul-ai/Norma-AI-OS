use crate::AdapterCommand;

/// Legacy realtime authorization retained for callers such as mobile execution.
/// GPT-Live delegates language intent to its model and does not call this parser.
pub fn realtime_command_authorized(command: &AdapterCommand, input: &str) -> bool {
    if command.is_read_only() {
        return true;
    }
    let observed = input.trim().to_lowercase();
    // An observed withdrawal/replacement revokes earlier clauses, even if a planner repeats them.
    let input = current_request(&observed);
    if input.is_empty() || input.len() > 32 * 1024 || input.contains(['?', '？']) {
        return false;
    }
    let mut action = imperative_action(input);
    if matches!(
        command,
        AdapterCommand::Memory(crate::MemoryCommand::Write { .. })
            | AdapterCommand::WorkspaceMemory {
                command: crate::MemoryCommand::Write { .. },
                ..
            }
    ) {
        let veto = ["不要", "别", "不用", "不需要", "先别", "取消"]
            .iter()
            .any(|negative| {
                [
                    "记住",
                    "记下",
                    "保存",
                    "更新记忆",
                    "纠正记忆",
                    "整理记忆",
                    "清理记忆",
                ]
                .iter()
                .any(|verb| input.contains(&format!("{negative}{verb}")))
            });
        if veto
            || contains_any(
                input,
                &[
                    "don't remember",
                    "do not remember",
                    "don't save",
                    "do not save",
                ],
            )
        {
            return false;
        }
        return [
            "记住",
            "记下",
            "更新记忆",
            "纠正记忆",
            "清理记忆",
            "整理记忆",
            "remember ",
            "update memory ",
            "correct memory ",
        ]
        .iter()
        .any(|verb| {
            action
                .strip_prefix(verb)
                .is_some_and(|content| !content.trim().is_empty())
        });
    }
    if [
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
    let named_agent = action.starts_with("让");
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
        AdapterCommand::CreateTerminal => {
            return creation_requested(action, &["终端", "terminal"], None);
        }
        AdapterCommand::CreateBrowser { url } => {
            return creation_requested(action, &["浏览器", "网页", "browser"], url.as_deref());
        }
        AdapterCommand::CreatePreview { path, .. } => {
            return creation_requested(
                action,
                &["预览", "preview"],
                Some(path.to_string_lossy().as_ref()),
            );
        }
        AdapterCommand::SendTerminalInput { .. } => {
            action = action.strip_prefix("在终端").unwrap_or(action).trim_start();
            &["输入", "执行", "运行", "input ", "execute ", "run "]
        }
        AdapterCommand::CancelAgentTask { .. } | AdapterCommand::CancelAgentReply { .. } => {
            &["取消", "停止", "终止", "cancel ", "stop "]
        }
        AdapterCommand::DeleteCanvasCard { .. }
        | AdapterCommand::DeleteCanvasCardAtPosition { .. }
        | AdapterCommand::DeleteAgentCard { .. } => {
            &["删除", "移除", "关闭", "delete ", "remove ", "close "]
        }
        AdapterCommand::FocusCard { .. } => &["聚焦", "切到", "打开", "focus ", "show "],
        AdapterCommand::AssignAgentTask { .. }
        | AdapterCommand::ReplyAgent { .. }
        | AdapterCommand::CreateAgent { .. } => {
            if !named_agent
                && contains_any(
                    action,
                    &["终端", "terminal", "浏览器", "browser", "预览", "preview"],
                )
                && ["创建", "新建", "打开", "create ", "open "]
                    .iter()
                    .any(|verb| action.starts_with(verb))
            {
                return false;
            }
            &[
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
            ]
        }
        // 想法：只有明确的记录/派发祈使句才算授权；“让 X 做…”保守视为草稿，需要再确认。
        AdapterCommand::CaptureIdea { .. } => &[
            "记一下",
            "记下",
            "记录",
            "记个",
            "记着",
            "别忘了",
            "提醒我",
            "加个待办",
            "添加待办",
            "note ",
            "remember ",
        ],
        AdapterCommand::AssignIdea { .. } => &["把", "交给", "派给", "指派", "分给", "assign "],
        AdapterCommand::Memory(_)
        | AdapterCommand::UpdateWorkingState(_)
        | AdapterCommand::ReadSharedContext { .. }
        | AdapterCommand::ContextCheckpoint { .. }
        | AdapterCommand::WorkspaceMemory { .. }
        | AdapterCommand::ListIdeas
        | AdapterCommand::FindAgentTasks { .. } => return false,
        AdapterCommand::WorkspaceSnapshot { .. }
        | AdapterCommand::AssignWorkspaceTask { .. }
        | AdapterCommand::HandoffToBrain { .. }
        | AdapterCommand::HandoffWithContext { .. }
        | AdapterCommand::OpenWorkspaceArtifact { .. }
        | AdapterCommand::LocateWorkspaceAgent { .. } => &[
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
            "打开",
            "定位",
            "重构",
            "implement ",
            "fix ",
            "create ",
            "build ",
            "run ",
            "open ",
            "locate ",
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

fn current_request(input: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    let mut clause_start = true;
    let mut request_start = 0;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(end) = quote {
            if ch == '\\' {
                escaped = true;
            } else if ch == end {
                quote = None;
            }
            continue;
        }
        let end_quote = closing_quote(ch);
        if end_quote.is_some() {
            quote = end_quote;
            clause_start = false;
            continue;
        }
        if clause_start && !ch.is_whitespace() {
            for marker in [
                "不对",
                "算了",
                "改成",
                "改为",
                "换成",
                "撤回",
                "取消刚才",
                "instead",
                "actually",
            ] {
                if let Some(rest) = input[index..].strip_prefix(marker) {
                    if !marker.is_ascii()
                        || rest.is_empty()
                        || rest.starts_with(|c: char| c.is_whitespace() || ",:;".contains(c))
                    {
                        request_start = index + marker.len();
                    }
                }
            }
            clause_start = false;
        }
        if ['，', ',', '。', '；', ';', '\n'].contains(&ch) {
            clause_start = true;
        }
    }
    input[request_start..].trim_start_matches(['，', ',', '。', ':', '：', ' '])
}

fn contains_any(input: &str, words: &[&str]) -> bool {
    words.iter().any(|word| input.contains(word))
}

fn creation_requested(input: &str, resources: &[&str], explicit: Option<&str>) -> bool {
    unquoted_clauses(input).into_iter().any(|clause| {
        let action = imperative_action(clause.trim().trim_start_matches("且"));
        let object = ["创建", "新建", "打开", "create ", "open "]
            .iter()
            .find_map(|verb| action.strip_prefix(verb))
            .or_else(|| {
                resources
                    .contains(&"preview")
                    .then(|| {
                        action
                            .strip_prefix("预览")
                            .or_else(|| action.strip_prefix("preview "))
                    })
                    .flatten()
            });
        let Some(mut object) = object else {
            return false;
        };
        object = object.trim_start();
        for prefix in [
            "一个新的",
            "一个",
            "两个",
            "一张",
            "新的",
            "a new ",
            "a ",
            "two ",
        ] {
            if let Some(rest) = object.strip_prefix(prefix) {
                object = rest.trim_start();
                break;
            }
        }
        explicit
            .filter(|value| !value.is_empty())
            .is_some_and(|value| object.starts_with(&value.to_lowercase()))
            || resources.iter().any(|resource| {
                object.strip_prefix(resource).is_some_and(|rest| {
                    rest.is_empty()
                        || rest.starts_with(char::is_whitespace)
                        || ["卡片", "窗口", "用于", "来", "名为", "叫"]
                            .iter()
                            .any(|prefix| rest.starts_with(prefix))
                })
            })
    })
}

fn closing_quote(ch: char) -> Option<char> {
    match ch {
        '“' => Some('”'),
        '‘' => Some('’'),
        '「' => Some('」'),
        '『' => Some('』'),
        '"' | '\'' | '`' => Some(ch),
        _ => None,
    }
}

fn unquoted_clauses(input: &str) -> Vec<&str> {
    let mut clauses = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(end) = quote {
            if ch == '\\' {
                escaped = true;
            } else if ch == end {
                quote = None;
            }
            continue;
        }
        if let Some(end) = closing_quote(ch) {
            quote = Some(end);
            continue;
        }
        if ['，', ',', '；', ';', '。', '\n', '并'].contains(&ch) {
            clauses.push(&input[start..index]);
            start = index + ch.len_utf8();
        }
    }
    clauses.push(&input[start..]);
    clauses
}

fn imperative_action(mut input: &str) -> &str {
    loop {
        match [
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
        .find_map(|prefix| input.strip_prefix(prefix))
        {
            Some(next) => input = next.trim_start(),
            None => return input,
        }
    }
}
