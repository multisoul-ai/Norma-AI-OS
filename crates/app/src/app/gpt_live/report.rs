pub(super) enum PlannerFailure {
    Configuration,
    ToolDefinition,
    Timeout,
    Cancelled,
    Runtime,
    Budget,
    Unknown,
}

impl From<agent::NormaSessionError> for PlannerFailure {
    fn from(error: agent::NormaSessionError) -> Self {
        use agent::NormaSessionError::*;
        match error {
            Configuration(_) => Self::Configuration,
            Runtime(message) if message.contains("invalid_function_parameters") => {
                Self::ToolDefinition
            }
            Timeout => Self::Timeout,
            Cancelled => Self::Cancelled,
            ToolBudgetExceeded | ToolFailureBudgetExceeded => Self::Budget,
            UnknownSideEffect => Self::Unknown,
            _ => Self::Runtime,
        }
    }
}

pub(super) struct DelegatedReport {
    pub receipts: Vec<Result<agent::AdapterResult, agent::AdapterError>>,
    pub pending: bool,
    pub effects: bool,
    pub planner: Result<Option<String>, PlannerFailure>,
}

impl Default for DelegatedReport {
    fn default() -> Self {
        Self {
            receipts: Vec::new(),
            pending: false,
            effects: false,
            planner: Ok(None),
        }
    }
}

impl DelegatedReport {
    pub fn message(&self) -> String {
        if self.pending || (self.effects && self.receipts.is_empty()) {
            return "操作结果仍待核对，不会自动重做。请查询待核对操作。".into();
        }
        let mut message = String::new();
        if let Err(error) = &self.planner {
            message.push_str(match error {
                PlannerFailure::Configuration => "GPT-Live API Key不可用，请重新保存并连接。",
                PlannerFailure::ToolDefinition => {
                    "GPT-Live工具定义未通过OpenAI校验，这是应用兼容性错误。"
                }
                PlannerFailure::Timeout => "后台规划超时，请核对已有回执后再提出新请求。",
                PlannerFailure::Cancelled => "后台规划已取消。",
                PlannerFailure::Budget => "后台已达到工具预算，请核对已有回执。",
                PlannerFailure::Unknown => "后台发现待核对结果，不会自动重做。",
                PlannerFailure::Runtime => {
                    "GPT-Live项目工具调用失败，请检查OpenAI Key的模型权限或网络连接。"
                }
            });
        }
        if self.receipts.is_empty() {
            message.push_str("未执行任何操作。背景文字不是执行回执。");
        }
        let mut receipts = self.receipts.iter().collect::<Vec<_>>();
        receipts.sort_by_key(|receipt| match receipt {
            Err(_) => 0,
            Ok(result) if result.status == "unknown" => 0,
            Ok(result)
                if result.node_id.is_some()
                    || result.task_id.is_some()
                    || result.reply_id.is_some() =>
            {
                1
            }
            _ => 2,
        });
        if receipts.len() > 3 {
            message.push_str("其余回执请在项目中核对。");
        }
        // Facts and identities precede optional query bodies; a long read must not hide a write.
        for receipt in receipts.iter().take(3) {
            let fact = match receipt {
                Ok(result) => format!(
                    "状态{}，task={:?}，node={:?}，reply={:?}。",
                    result.status, result.task_id, result.node_id, result.reply_id
                ),
                Err(_) => "操作结果待核对，不得宣称成功。".into(),
            };
            message.push_str(&truncate(&fact, 110));
        }
        for result in receipts.iter().copied().flatten() {
            for task in result.tasks.iter().take(3) {
                let identity = serde_json::json!({"task_id":task.get("task_id"),"task_ref":task.get("task_ref"),"state":task.get("state"),"agent":task.get("agent")});
                message.push_str(&bounded(&identity.to_string()));
            }
        }
        let mut bodies = String::new();
        for receipt in receipts {
            let Ok(result) = receipt else {
                continue;
            };
            if let Some(memory) = &result.memory {
                bodies.push_str("记忆查询数据：");
                if let Some(markdown) = &memory.markdown {
                    bodies.push_str(&bounded(markdown));
                } else {
                    bodies.push_str(&bounded(
                        &serde_json::to_string(&memory.entries).unwrap_or_default(),
                    ));
                }
            }
            bodies.push_str(&truncate(&result.message, 120));
        }
        message.push_str(&truncate(&bodies, 160));
        if !self.effects {
            if let Ok(Some(text)) = &self.planner {
                if !self.receipts.is_empty() {
                    message.push_str("后台草案不是执行回执：");
                }
                message.push_str(&truncate(text, 120));
            }
        }
        bounded(&message)
    }
}

fn bounded(message: &str) -> String {
    // 480 UTF-8 bytes stays below the Live append token budget for Chinese and JSON.
    truncate(message, 480)
}

fn truncate(message: &str, limit: usize) -> String {
    let mut end = message.len().min(limit);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].into()
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_report_tests.rs"]
mod tests;
pub(super) fn admission(
    project: Option<&str>,
    backend_ready: bool,
    vad_ready: bool,
) -> Option<&'static str> {
    if project.is_none_or(str::is_empty) {
        Some("请先打开项目。未执行任何操作，实时聊天仍可继续。")
    } else if !backend_ready {
        Some("GPT-Live API Key尚未保存就绪，请重新连接。未执行任何操作，实时聊天仍可继续。")
    } else if !vad_ready {
        Some("语音检测自动准备失败，请结束通话后重新连接以重试。未执行任何操作，实时聊天仍可继续。")
    } else {
        None
    }
}

#[derive(Default)]
pub(super) struct FinalReports(std::collections::VecDeque<super::session::CallCommand>);
impl FinalReports {
    pub fn push(&mut self, delegation_id: String, content: String) -> Result<(), String> {
        if self.0.len() >= 16 {
            return Err("Live 最终反馈积压，请重新连接并核对操作回执".into());
        }
        self.0.push_back(super::session::CallCommand::AppendResult {
            event_id: uuid::Uuid::new_v4().to_string(),
            kind: live_voice::AppendKind::Commentary,
            delegation_id: Some(delegation_id),
            content,
        });
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn flush(
        &mut self,
        sender: &async_channel::Sender<super::session::CallCommand>,
    ) -> Result<(), String> {
        while let Some(command) = self.0.pop_front() {
            match sender.try_send(command) {
                Ok(()) => {}
                Err(async_channel::TrySendError::Full(command)) => {
                    self.0.push_front(command);
                    break;
                }
                Err(async_channel::TrySendError::Closed(_)) => {
                    return Err("Live 反馈连接已关闭，请重新连接并核对操作回执".into())
                }
            }
        }
        Ok(())
    }
}
