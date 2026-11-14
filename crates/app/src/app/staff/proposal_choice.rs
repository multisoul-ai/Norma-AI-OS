//! 采纳前为提案选择智能体与模型；选择随 approve 一并落库后才会派发。
use super::*;
use norma::protocol::AgentProfile;

impl MultisoulApp {
    /// 模型下拉需要 Codex、OpenCode 的模型列表；设置页没打开过时在这里读一次。
    pub(super) fn ensure_proposal_model_lists(&mut self, cx: &mut Context<Self>) {
        if self.staff.models_probed {
            return;
        }
        let has_open = self.staff.tasks.values().any(|task| {
            task.proposals
                .iter()
                .any(|p| !p.ignored && p.dispatch_id.is_none())
        });
        if has_open {
            self.staff.models_probed = true;
            self.refresh_brain_settings(cx);
        }
    }

    /// 当前生效的智能体：用户改选优先，否则是提案原值。
    pub(crate) fn staff_proposal_agent(
        &self,
        task_id: &str,
        proposal: &norma::protocol::StaffProposal,
    ) -> String {
        self.staff
            .proposal_agents
            .get(&(task_id.to_owned(), proposal.id))
            .cloned()
            .unwrap_or_else(|| proposal.agent.clone())
    }

    /// 已启用的智能体（名字、类型），顺序同名册。
    pub(crate) fn staff_agent_choices(&self) -> Vec<(String, AgentProfile)> {
        self.agent_roster_for_provider()
            .into_iter()
            .filter(|entry| entry.enabled)
            .map(|entry| (entry.name, entry.profile))
            .collect()
    }

    /// 该智能体可选的模型 (id, 显示名)；没有已知列表的类型返回空，只能用 CLI 默认。
    pub(crate) fn staff_model_choices(&self, agent: &str) -> Vec<(String, String)> {
        let Some((_, profile)) = self
            .staff_agent_choices()
            .into_iter()
            .find(|(name, _)| name == agent)
        else {
            return Vec::new();
        };
        match profile {
            AgentProfile::Claude => ["opus", "sonnet", "haiku"]
                .map(|alias| (alias.to_owned(), alias[..1].to_uppercase() + &alias[1..]))
                .into(),
            AgentProfile::Codex => self.brain_model_choices(brain::BackendKind::Codex),
            AgentProfile::OpenCode => self.brain_model_choices(brain::BackendKind::OpenCode),
            _ => Vec::new(),
        }
    }

    /// 改选智能体；上一个类型的模型对新类型无效，一并清掉。
    pub(crate) fn set_staff_proposal_agent(
        &mut self,
        task_id: &str,
        proposal_id: u64,
        agent: String,
        cx: &mut Context<Self>,
    ) {
        let key = (task_id.to_owned(), proposal_id);
        self.staff.proposal_agents.insert(key.clone(), agent);
        self.staff.proposal_models.remove(&key);
        self.notify_staff(cx);
    }

    pub(crate) fn set_staff_proposal_model(
        &mut self,
        task_id: &str,
        proposal_id: u64,
        model: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let key = (task_id.to_owned(), proposal_id);
        match model {
            Some(model) => self.staff.proposal_models.insert(key, model),
            None => self.staff.proposal_models.remove(&key),
        };
        self.notify_staff(cx);
    }

    /// 采纳时提交的 (智能体, 模型)；模型缺省表示用 CLI 默认。
    pub(crate) fn staff_proposal_choice(
        &self,
        task_id: &str,
        proposal_id: u64,
    ) -> (Option<String>, Option<String>) {
        let key = (task_id.to_owned(), proposal_id);
        (
            self.staff.proposal_agents.get(&key).cloned(),
            self.staff.proposal_models.get(&key).cloned(),
        )
    }
}
