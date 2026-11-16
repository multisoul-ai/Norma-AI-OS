//! 幕僚回复：一个幕僚轮次的全部输出（文字、过程摘要、卡片引用）。
//! 过程摘要只存标题、状态和耗时；工具入参与输出永不落盘。
use super::{StaffQuestion, Task, TaskMessage, TaskPhase};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReplyStatus {
    #[default]
    Running,
    Done,
    Stopped,
    Failed,
}

/// 谁触发了这一轮；只有用户触发的轮次可以重新生成。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReplyTrigger {
    #[default]
    User,
    Event,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StepStatus {
    Running,
    Completed,
    Failed,
    Denied,
    Interrupted,
}

/// 结构化步骤标题，展示边界再翻译。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StepTitle {
    /// `norma` 子命令词，如 ["brain", "agents"]；不含路径与参数值。
    Norma(Vec<String>),
    Tool(String),
    /// 工具名加它碰的文件或目录（已缩短为末三段）；旧快照没有这个变体。
    ToolAt(String, String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanItem {
    pub status: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StaffPart {
    Text(String),
    Step {
        id: String,
        title: StepTitle,
        status: StepStatus,
        elapsed_ms: Option<u64>,
    },
    Plan(Vec<PlanItem>),
    /// `Task.proposals` 的引用。
    Proposal(u64),
    /// `Task.questions` 的引用。
    Question(String),
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StaffReply {
    pub round: u64,
    pub trigger: ReplyTrigger,
    pub parts: Vec<StaffPart>,
    pub status: ReplyStatus,
    pub elapsed_ms: u64,
}

impl StaffReply {
    pub fn new(round: u64, trigger: ReplyTrigger) -> Self {
        Self {
            round,
            trigger,
            ..Self::default()
        }
    }

    pub fn is_running(&self) -> bool {
        self.status == ReplyStatus::Running
    }

    /// 流式文字追加到最后一段；前一个 part 不是文字（步骤、卡片）时另起一段。
    pub fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.parts.last_mut() {
            Some(StaffPart::Text(last)) => last.push_str(text),
            _ => self.parts.push(StaffPart::Text(text.into())),
        }
    }

    /// 模型重写时清掉本轮已流式的文字，步骤和卡片保留。
    pub fn clear_text(&mut self) {
        self.parts
            .retain(|part| !matches!(part, StaffPart::Text(_)));
    }

    pub fn push_card(&mut self, card: StaffPart) {
        if !self.parts.contains(&card) {
            self.parts.push(card);
        }
    }

    /// 按 id 更新步骤；被拒绝（Denied）的步骤不会被后续事件改回。
    pub fn upsert_step(
        &mut self,
        id: &str,
        title: Option<StepTitle>,
        status: Option<StepStatus>,
        elapsed_ms: Option<u64>,
    ) {
        let found = self.parts.iter_mut().find_map(|part| match part {
            StaffPart::Step {
                id: step_id,
                title,
                status,
                elapsed_ms,
            } if step_id == id => Some((title, status, elapsed_ms)),
            _ => None,
        });
        let Some((old_title, old_status, old_elapsed)) = found else {
            self.parts.push(StaffPart::Step {
                id: id.into(),
                title: title.unwrap_or_else(|| StepTitle::Tool(id.into())),
                status: status.unwrap_or(StepStatus::Running),
                elapsed_ms,
            });
            return;
        };
        if let Some(title) = title {
            *old_title = merge_title(old_title, title);
        }
        if *old_status == StepStatus::Denied {
            return;
        }
        if let Some(status) = status {
            *old_status = status;
        }
        if elapsed_ms.is_some() && old_elapsed.is_none() {
            *old_elapsed = elapsed_ms;
        }
    }

    pub fn set_plan(&mut self, items: Vec<PlanItem>) {
        match self
            .parts
            .iter_mut()
            .find(|part| matches!(part, StaffPart::Plan(_)))
        {
            Some(part) => *part = StaffPart::Plan(items),
            None => self.parts.push(StaffPart::Plan(items)),
        }
    }

    pub fn step_count(&self) -> usize {
        self.parts
            .iter()
            .filter(|part| matches!(part, StaffPart::Step { .. }))
            .count()
    }

    /// 正文文字，各段之间空一行；复制与模型快照都用它。
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                StaffPart::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// 结束回合：仍在运行的步骤记为中断，固定耗时。
    pub fn finish(&mut self, status: ReplyStatus, elapsed_ms: u64) {
        for part in &mut self.parts {
            if let StaffPart::Step { status, .. } = part {
                if *status == StepStatus::Running {
                    *status = StepStatus::Interrupted;
                }
            }
        }
        self.status = status;
        self.elapsed_ms = elapsed_ms;
    }
}

impl Task {
    /// 开始一轮：追加一条 Running 的幕僚回复。
    pub fn begin_reply(&mut self, trigger: ReplyTrigger) -> &mut StaffReply {
        self.messages
            .push(TaskMessage::Staff(StaffReply::new(self.round, trigger)));
        self.updated_at = super::timestamp();
        match self.messages.last_mut() {
            Some(TaskMessage::Staff(reply)) => reply,
            _ => unreachable!("a staff reply was just pushed"),
        }
    }

    pub fn has_running_reply(&self) -> bool {
        self.messages
            .iter()
            .rev()
            .find_map(|message| match message {
                TaskMessage::Staff(reply) => Some(reply.is_running()),
                _ => None,
            })
            .unwrap_or(false)
    }

    /// 进行中的回合是否由用户消息触发；事件触发的回合没有用户指令。
    pub fn user_turn_running(&self) -> bool {
        self.messages
            .iter()
            .rev()
            .find_map(|message| match message {
                TaskMessage::Staff(reply) => {
                    Some(reply.is_running() && reply.trigger == ReplyTrigger::User)
                }
                _ => None,
            })
            .unwrap_or(false)
    }

    pub fn running_reply_mut(&mut self) -> Option<&mut StaffReply> {
        self.messages
            .iter_mut()
            .rev()
            .find_map(|message| match message {
                TaskMessage::Staff(reply) if reply.is_running() => Some(reply),
                _ => None,
            })
    }

    /// 卡片挂进进行中的回复；没有进行中的回复（命令在轮次之外到达）时另起一条已完成的事件回复。
    pub fn attach_card(&mut self, card: StaffPart) {
        if let Some(reply) = self.running_reply_mut() {
            reply.push_card(card);
        } else {
            let mut reply = StaffReply::new(self.round, ReplyTrigger::Event);
            reply.status = ReplyStatus::Done;
            reply.parts.push(card);
            self.messages.push(TaskMessage::Staff(reply));
        }
        self.updated_at = super::timestamp();
    }

    /// 回答问题：重新打开已完成任务并追加 Answered；回答正文在 `questions` 里，不再伪装成用户消息。
    pub fn answered_message(&mut self, question_id: &str) {
        self.archived = false;
        if self.phase == TaskPhase::Done {
            self.phase = TaskPhase::Active;
            self.phase_manual = false;
            self.messages.push(TaskMessage::Reopened);
        }
        self.messages.push(TaskMessage::Answered {
            question_id: question_id.into(),
        });
        self.updated_at = super::timestamp();
    }

    pub fn question(&self, id: &str) -> Option<&StaffQuestion> {
        self.questions.iter().find(|question| question.id == id)
    }

    /// 加载时修复崩溃遗留：仍是 Running 的回复记为 Failed。
    pub fn recover_interrupted(&mut self) -> bool {
        let mut changed = false;
        for message in &mut self.messages {
            if let TaskMessage::Staff(reply) = message {
                if reply.is_running() {
                    reply.finish(ReplyStatus::Failed, reply.elapsed_ms);
                    changed = true;
                }
            }
        }
        changed
    }

    /// 最近一条用户输入；重新生成用它重新提问。
    pub fn last_user_text(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find_map(|message| match message {
                TaskMessage::User { text } => Some(text.as_str()),
                _ => None,
            })
    }
}

/// 后续事件常常只带部分信息：没有路径的同名或占位名标题不覆盖已知详情，占位名也不覆盖已知工具名。
fn merge_title(old: &StepTitle, new: StepTitle) -> StepTitle {
    let placeholder = |name: &str| name == "tool";
    match (old, new) {
        (StepTitle::ToolAt(old_name, detail), StepTitle::Tool(name))
            if *old_name == name || placeholder(&name) =>
        {
            StepTitle::ToolAt(old_name.clone(), detail.clone())
        }
        (StepTitle::Tool(old_name) | StepTitle::ToolAt(old_name, _), StepTitle::Tool(name))
            if placeholder(&name) =>
        {
            StepTitle::Tool(old_name.clone())
        }
        (
            StepTitle::Tool(old_name) | StepTitle::ToolAt(old_name, _),
            StepTitle::ToolAt(name, detail),
        ) if placeholder(&name) => StepTitle::ToolAt(old_name.clone(), detail),
        (_, new) => new,
    }
}

/// 由工具调用的名称和入参得到结构化步骤标题：`norma` 子命令只保留命令词，
/// 其余只保留工具名；参数与参数值都不进入快照，工具碰的路径另由 [`step_title_at`] 补充。
pub fn step_title(name: Option<&str>, input: Option<&str>) -> StepTitle {
    // 入参通常是 `{"command": "..."}` 这样的 JSON；先取出命令再找 norma 子命令。
    let command = input.and_then(command_of);
    for text in [name, command.as_deref(), input].into_iter().flatten() {
        let words = norma_words(text);
        if !words.is_empty() {
            return StepTitle::Norma(words);
        }
    }
    let first = name
        .and_then(|name| name.split_whitespace().next())
        .map(|token| token.trim_matches(|c| c == '"' || c == '\''))
        .map(|token| token.rsplit('/').next().unwrap_or(token))
        .filter(|token| !token.is_empty())
        .unwrap_or("tool");
    StepTitle::Tool(first.chars().take(40).collect())
}

/// 在 [`step_title`] 之上补充工具碰的文件或目录：优先 ACP `locations` 的路径，
/// 其次 `rawInput` 的 `file_path`/`path`，再从命令里取 `cd` 目录或最后一个路径参数，最后才是查询模式。
/// 全都没有时不编造，返回纯工具名；路径只留末三段，绝对前缀不进快照。
pub fn step_title_at(name: Option<&str>, input: Option<&str>, path: Option<&str>) -> StepTitle {
    let title = step_title(name, input);
    let StepTitle::Tool(tool) = title else {
        return title;
    };
    match tool_detail(input, path) {
        Some(detail) => StepTitle::ToolAt(tool, detail),
        None => StepTitle::Tool(tool),
    }
}

fn tool_detail(input: Option<&str>, path: Option<&str>) -> Option<String> {
    let value = input.and_then(|input| serde_json::from_str::<serde_json::Value>(input).ok());
    let field = |keys: &[&str]| {
        let value = value.as_ref()?;
        keys.iter()
            .find_map(|key| value.get(*key)?.as_str().filter(|text| !text.is_empty()))
            .map(str::to_owned)
    };
    let raw = path
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .or_else(|| field(&["file_path", "path", "filePath", "notebook_path"]))
        .or_else(|| command_of(input?).and_then(|command| command_target(&command)))?;
    Some(shorten(&raw))
}

/// 命令里的目标：`cd` 取目录；其余取最后一个像路径的词。参数、URL、`KEY=值` 赋值和搜索模式都不是路径，不进快照。
fn command_target(command: &str) -> Option<String> {
    let mut words = split_words(command);
    // `zsh -lc 'cd x && ...'`：穿透外壳，引号里的整条命令重新拆词。
    if words
        .first()
        .is_some_and(|word| matches!(word.rsplit('/').next(), Some("zsh" | "bash" | "sh")))
    {
        words.remove(0);
        while words.first().is_some_and(|word| word.starts_with('-')) {
            words.remove(0);
        }
        if words.len() == 1 {
            words = split_words(&words[0]);
        }
    }
    let first = words.first()?;
    if first == "cd" {
        return words.get(1).filter(|dir| !dir.starts_with('-')).cloned();
    }
    let args: Vec<&String> = words[1..]
        .iter()
        .take_while(|word| {
            !matches!(word.as_str(), "&&" | "||" | ";" | "|") && !word.starts_with(['>', '<'])
        })
        .collect();
    args.into_iter()
        .rev()
        .find(|word| looks_like_path(word))
        .cloned()
}

/// 带目录分隔符或以 `.`、`~` 开头的词才算路径；URL、`KEY=值`、带 `?` 的词与参数不算。
fn looks_like_path(word: &str) -> bool {
    !word.starts_with('-')
        && !word.contains("://")
        && !word.contains(['=', '?'])
        && (word.contains('/') || word.starts_with(['.', '~']))
}

/// 路径只留末三段，超过时以「…/」开头；总长不超过 80 个字符。
fn shorten(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('/').collect();
    let short = if parts.len() > 4 {
        format!("…/{}", parts[parts.len() - 3..].join("/"))
    } else {
        raw.to_owned()
    };
    short.chars().take(80).collect()
}

/// ACP 的 `rawInput` 里的命令：字符串，或拆开的字符串数组。
fn command_of(input: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(input).ok()?;
    match value.get("command")? {
        serde_json::Value::String(command) => Some(command.clone()),
        serde_json::Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|part| part.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

fn norma_words(text: &str) -> Vec<String> {
    let tokens = split_words(text);
    let Some(at) = tokens
        .iter()
        .position(|token| token.rsplit('/').next() == Some("norma"))
    else {
        return Vec::new();
    };
    tokens[at + 1..]
        .iter()
        .take_while(|token| {
            !token.is_empty() && token.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        })
        .take(3)
        .filter(|token| !token.starts_with('-'))
        .cloned()
        .collect()
}

/// 按空白拆词，引号内的空白保留。
fn split_words(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            (None, c) => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
