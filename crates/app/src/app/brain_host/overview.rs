use super::{proposals, questions::AnswerSuggestion};
use std::collections::HashSet;

/// 概览需要的、分散在各项目会话里的大脑状态。
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct OverviewSnapshot {
    /// 大脑正在判断的审批：（项目区域 id，卡片 id）。
    pub judging: HashSet<(String, u64)>,
    /// 至少一个项目里的大脑在运行；工作空间窗口自己不跑大脑，设置页靠它显示“运行中”。
    pub running: bool,
    /// 等用户确认的下一步，新的在前。
    pub proposals: Vec<OverviewProposal>,
    pub questions: Vec<OverviewQuestion>,
    pub approvals: Vec<(String, norma::mobile::ApprovalSnapshot)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OverviewProposal {
    pub area_id: String,
    pub proposal: proposals::Proposal,
    /// 目标 Agent 还没有卡片，派发时会新建。
    pub new_card: bool,
    pub block: Option<proposals::ProposalBlock>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OverviewQuestion {
    pub area_id: String,
    pub agent: String,
    pub snapshot: norma::protocol::QuestionSnapshot,
    pub suggestion: Option<AnswerSuggestion>,
    pub available: bool,
    pub delegated: bool,
    pub can_delegate: bool,
}

impl crate::app::MultisoulApp {
    /// Read the overview's exact cached request; expansion never drains providers or performs IO.
    pub(crate) fn overview_approval_details(
        &self,
        area: &str,
        node: u64,
        run: &str,
        request: &str,
        _cx: &gpui::App,
    ) -> Option<(String, String)> {
        let (_, approval) = self.brain.overview.approvals.iter().find(|(id, entry)| {
            id == area
                && entry.target.node_id == node
                && entry.target.run_id == run
                && entry.target.request_id == request
        })?;
        let what = approval.details["command"]
            .as_str()
            .or_else(|| approval.details["file_path"].as_str())
            .unwrap_or(&approval.tool)
            .to_owned();
        let original = serde_json::to_string_pretty(&serde_json::json!({
            "target": approval.target, "tool": approval.tool, "input": approval.details,
        }))
        .ok()?;
        Some((what, original))
    }
}
