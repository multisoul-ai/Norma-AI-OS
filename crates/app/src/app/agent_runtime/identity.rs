use canvas::CanvasNodeKind;
use norma::protocol::{AgentErrorCode, AgentProfile, CardCommandError};

use super::super::MultisoulApp;

/// 一次经服务端鉴权后的来源 Agent 身份快照（Workspace 作用域）。
///
/// 身份判定不变量 I-1：同一 Workspace 内 `source_definition_id == target_definition_id`
/// 才是直接自派发；`name` 与持久化 `speech_alias` 只用于显示与寻址，不跨 Workspace 判同。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentIdentitySnapshot {
    pub(crate) workspace_id: String,
    pub(crate) definition_id: i64,
    pub(crate) node_id: u64,
    pub(crate) name: String,
    pub(crate) speech_alias: Option<String>,
    pub(crate) profile: AgentProfile,
}

/// 统一自派发指令性错误构造器（全局账本路径与 Workspace 内 coordinator 路径共用）。
///
/// 文案模板（见设计文档 §4）：
///   - `retryable` 恒为 `false`，确定性拒绝不得自动重试。
///   - 无持久化 `speech_alias` 时整段省略，不输出 `None`、空串或推导值。
pub(crate) fn self_dispatch_forbidden_error(identity: &AgentIdentitySnapshot) -> CardCommandError {
    let identity_line = match identity.speech_alias.as_deref() {
        Some(alias) if !alias.trim().is_empty() => format!(
            "你当前的 Norma OS Agent 身份是“{}”（语音称呼：“{}”）。",
            identity.name, alias
        ),
        _ => format!("你当前的 Norma OS Agent 身份是“{}”。", identity.name),
    };
    let message = format!(
        "{identity_line}\n目标“{}”解析到与你相同的 Workspace 和 definition，因此目标就是你自己。\n\
         不要再次调用 agent task assign，不要更换 operation_id；请直接执行任务，\n\
         或向用户说明该任务无需/无法再次委派。",
        identity.name
    );
    CardCommandError::agent(AgentErrorCode::SelfDispatchForbidden, message, false)
}

impl MultisoulApp {
    /// 从已验证的来源构建带 Workspace 作用域的 Agent 身份快照。
    ///
    /// 同时核对：definition 存在、该 definition 的活跃卡片就是 `source.node_id`、
    /// 且当前 App 有稳定 `workspace_id`。任何一项缺失都返回 `None`，绝不猜测名称。
    pub(crate) fn agent_identity_for_source(
        &self,
        source: agent::AgentDispatchSource,
    ) -> Option<AgentIdentitySnapshot> {
        let definition_id = source.definition_id?;
        let node_id = source.node_id?;
        let definition = self
            .agent_definitions
            .definitions()
            .iter()
            .find(|definition| definition.id == definition_id)?;
        let active_node_matches = self.runtime.document.nodes().iter().any(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return false;
            };
            spec.definition_id == Some(definition_id) && node.id == node_id
        });
        if !active_node_matches {
            return None;
        }
        Some(AgentIdentitySnapshot {
            workspace_id: self.workspace_id.clone()?,
            definition_id,
            node_id,
            name: definition.name.clone(),
            speech_alias: definition.speech_alias.clone(),
            profile: norma_agent_profile(definition.runtime_profile),
        })
    }
}

pub(super) fn norma_agent_profile(profile: canvas::AgentProfile) -> norma::protocol::AgentProfile {
    match profile {
        canvas::AgentProfile::Claude => norma::protocol::AgentProfile::Claude,
        canvas::AgentProfile::Codex => norma::protocol::AgentProfile::Codex,
        canvas::AgentProfile::Cursor => norma::protocol::AgentProfile::Cursor,
        canvas::AgentProfile::Grok => norma::protocol::AgentProfile::Grok,
        canvas::AgentProfile::OpenCode => norma::protocol::AgentProfile::OpenCode,
        canvas::AgentProfile::Kimi => norma::protocol::AgentProfile::Kimi,
        canvas::AgentProfile::ZCode => norma::protocol::AgentProfile::ZCode,
        canvas::AgentProfile::Pi => norma::protocol::AgentProfile::Pi,
        canvas::AgentProfile::Hermes => norma::protocol::AgentProfile::Hermes,
    }
}

#[cfg(test)]
mod tests {
    use super::{self_dispatch_forbidden_error, AgentIdentitySnapshot};

    /// U-1：统一自派发错误在带持久化语音称呼时，必须包含规范名与别名且 retryable=false。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   identity 字段 = 6 项（workspace_id、definition_id、node_id、name、speech_alias、profile）
    ///   name      = "Alan"（4 个英文字母）
    ///   alias     = "艾伦"（2 个汉字）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造带语音称呼的 AgentIdentitySnapshot。
    ///   2. 调用统一自派发错误构造器。
    ///   3. 断言错误码、retryable 与消息内容。
    ///
    /// 预期结果：
    ///   - 正断言：错误码为 SelfDispatchForbidden 且 retryable=false。
    ///   - 正断言：消息包含规范名 "Alan" 与语音称呼 "艾伦"。
    ///   - 正断言：消息明确禁止再次调用 assign 与更换 operation_id。
    #[test]
    fn self_dispatch_error_includes_name_and_alias() {
        let identity = AgentIdentitySnapshot {
            workspace_id: "workspace-test".to_string(),
            definition_id: 1,
            node_id: 7,
            name: "Alan".to_string(),
            speech_alias: Some("艾伦".to_string()),
            profile: norma::protocol::AgentProfile::Codex,
        };
        let error = self_dispatch_forbidden_error(&identity);
        let norma::protocol::CardCommandError::Agent(failure) = error else {
            panic!("统一自派发错误必须构造为 Agent 失败；实际：{error:?}");
        };
        assert_eq!(
            failure.code,
            norma::protocol::AgentErrorCode::SelfDispatchForbidden,
            "统一自派发错误必须使用稳定的 SelfDispatchForbidden 码"
        );
        assert!(!failure.retryable, "确定性拒绝 retryable 必须为 false");
        assert!(
            failure.message.contains("Alan"),
            "错误消息必须包含规范名 Alan，实际：{}",
            failure.message
        );
        assert!(
            failure.message.contains("艾伦"),
            "错误消息必须包含持久化语音称呼 艾伦，实际：{}",
            failure.message
        );
        assert!(
            failure.message.contains("不要更换 operation_id"),
            "错误消息必须明确禁止更换 operation_id，实际：{}",
            failure.message
        );
    }

    /// U-2：无持久化语音称呼时，统一文案必须整段省略语音称呼，不输出空串或推导值。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   identity 字段 = 6 项
    ///   name      = "Alan"
    ///   alias     = None（迁移后冲突被清空）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造无语音称呼的 AgentIdentitySnapshot。
    ///   2. 调用统一自派发错误构造器。
    ///   3. 检查消息是否包含"语音称呼"片段。
    ///
    /// 预期结果：
    ///   - 正断言：消息仍包含规范名 Alan。
    ///   - 负断言：消息不得包含"语音称呼"字样（含空串或推导值）。
    #[test]
    fn self_dispatch_error_omits_missing_alias() {
        let identity = AgentIdentitySnapshot {
            workspace_id: "workspace-test".to_string(),
            definition_id: 1,
            node_id: 7,
            name: "Alan".to_string(),
            speech_alias: None,
            profile: norma::protocol::AgentProfile::Codex,
        };
        let error = self_dispatch_forbidden_error(&identity);
        let norma::protocol::CardCommandError::Agent(failure) = error else {
            panic!("统一自派发错误必须构造为 Agent 失败；实际：{error:?}");
        };
        assert!(
            failure.message.contains("Alan"),
            "无别名时错误消息仍必须包含规范名 Alan，实际：{}",
            failure.message
        );
        assert!(
            !failure.message.contains("语音称呼"),
            "无持久化别名时不得凭空输出语音称呼片段，实际：{}",
            failure.message
        );
    }
}
