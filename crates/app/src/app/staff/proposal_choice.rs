//! 采纳前为提案选择智能体类型与模型；选择随 approve 一并落库后才会派发。
use super::*;
use gpui::{AppContext, Window};
use ui::component::input::InputEvent;

impl MultisoulApp {
    pub(super) fn ensure_proposal_model_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open: Vec<_> = self
            .staff
            .tasks
            .values()
            .flat_map(|task| {
                task.proposals
                    .iter()
                    .filter(|p| !p.ignored && p.dispatch_id.is_none())
                    .map(|p| (task.id.clone(), p.id))
            })
            .filter(|key| !self.staff.proposal_models.contains_key(key))
            .collect();
        for key in open {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(ui::i18n::input_placeholder(
                        "staff.model_placeholder",
                        window,
                        cx,
                    ))
                    .validate(|value, _| !value.contains(['\n', '\r']))
            });
            cx.subscribe(&input, |app, _, _: &InputEvent, cx| app.notify_staff(cx))
                .detach();
            self.staff.proposal_models.insert(key, input);
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

    /// 在已启用的智能体之间循环切换。
    pub(crate) fn cycle_staff_proposal_agent(
        &mut self,
        task_id: &str,
        proposal_id: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(proposal) = self
            .staff
            .tasks
            .get(task_id)
            .and_then(|task| task.proposals.iter().find(|p| p.id == proposal_id))
        else {
            return;
        };
        let names: Vec<String> = self
            .agent_roster_for_provider()
            .into_iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.name)
            .collect();
        let current = self.staff_proposal_agent(task_id, proposal);
        let next = names
            .iter()
            .position(|name| *name == current)
            .map_or(names.first(), |i| names.get((i + 1) % names.len()));
        if let Some(next) = next.cloned() {
            self.staff
                .proposal_agents
                .insert((task_id.to_owned(), proposal_id), next);
            self.notify_staff(cx);
        }
    }

    /// 采纳时提交的 (智能体, 模型)；模型留空表示用 CLI 默认。
    pub(crate) fn staff_proposal_choice(
        &self,
        task_id: &str,
        proposal_id: u64,
        cx: &gpui::App,
    ) -> (Option<String>, Option<String>) {
        let key = (task_id.to_owned(), proposal_id);
        let model = self
            .staff
            .proposal_models
            .get(&key)
            .map(|input| input.read(cx).value().to_string());
        (self.staff.proposal_agents.get(&key).cloned(), model)
    }
}
