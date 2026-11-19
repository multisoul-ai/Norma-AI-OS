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

    /// 编排里注册成功的引擎，顺序同设置页，不看画布上有没有卡片。
    pub(crate) fn registered_staff_engines(
        &self,
    ) -> Vec<norma::orchestration::OrchestrationProvider> {
        norma::orchestration::OrchestrationProvider::ALL
            .into_iter()
            .filter(|provider| {
                self.orchestration_settings
                    .integration(*provider)
                    .aggregate_status
                    == norma::orchestration::AggregateStatus::Ready
            })
            .collect()
    }

    /// 建议指派可选的执行类型（engine 标识、类型）。只含注册成功的引擎。
    pub(crate) fn staff_agent_choices(&self) -> Vec<(String, AgentProfile)> {
        self.registered_staff_engines()
            .into_iter()
            .map(|provider| (provider.slug().to_owned(), provider.profile()))
            .collect()
    }

    /// 建议指派按钮上的类型名。注册成功时用编排显示名，否则保留原文，不借卡片改名。
    pub(crate) fn staff_engine_label(&self, agent: &str) -> String {
        self.registered_provider(agent)
            .map(|provider| provider.display_name().to_owned())
            .unwrap_or_else(|| agent.to_owned())
    }

    /// 提案里的 engine 标识对应的类型。不拿名册名字或画布卡片推断。
    pub(crate) fn staff_engine_profile(&self, agent: &str) -> Option<AgentProfile> {
        self.registered_provider(agent)
            .map(|provider| provider.profile())
    }

    fn registered_provider(
        &self,
        agent: &str,
    ) -> Option<norma::orchestration::OrchestrationProvider> {
        self.registered_staff_engines()
            .into_iter()
            .find(|provider| provider.slug().eq_ignore_ascii_case(agent))
    }

    /// 该类型可选的模型 (id, 显示名)；没有已知列表的类型返回空，只能用 CLI 默认。
    pub(crate) fn staff_model_choices(&self, agent: &str) -> Vec<(String, String)> {
        let Some(profile) = self.staff_engine_profile(agent) else {
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
