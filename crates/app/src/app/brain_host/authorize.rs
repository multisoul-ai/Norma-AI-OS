//! 命令入口的统一授权：按 token 认出角色，再按角色的白名单和当前轮次的任务检查。
use super::MultisoulApp;
use brain::staff::gate::{authorize, history_in_task, BrainRole, GateContext, HISTORY_MAX_LAST};
use brain::staff::policy::Verdict;
use norma::protocol::{BrainOp, TaskPhase};

impl MultisoulApp {
    /// 返回拒绝码（`staff.*`），由展示边界翻译成中英文。
    pub(crate) fn authorize_brain(&mut self, token: &str, op: &BrainOp) -> Result<(), String> {
        let role = self
            .brain
            .role(token)
            .ok_or_else(|| "staff.invalid_source".to_owned())?;
        let active_task = self.single_active_task();
        // 幕僚自己设的 done 不算数：归档要等用户确认完成。
        let task_done = match op {
            BrainOp::Task { op } => op
                .task_id()
                .and_then(|id| self.staff.tasks.get(id))
                .is_some_and(|task| task.phase == TaskPhase::Done && task.phase_manual),
            _ => false,
        };
        let verdict = authorize(
            role,
            op,
            &GateContext {
                active_task: active_task.as_deref(),
                task_done,
            },
        );
        if let Verdict::Denied { code } = verdict {
            return Err(code.to_owned());
        }
        if role == BrainRole::Staff {
            self.check_staff_extras(op, active_task.as_deref())?;
        }
        Ok(())
    }

    /// 恰好一个任务正在进行一轮时返回它；一个都没有或同时有多个，都当作没有（失败即拒绝）。
    fn single_active_task(&self) -> Option<String> {
        let mut active = self
            .staff
            .tasks
            .values()
            .filter(|task| task.has_running_reply())
            .map(|task| task.id.clone());
        let first = active.next()?;
        active.next().is_none().then_some(first)
    }

    /// 白名单之外的幕僚限制：history 只读本任务派出过的 Agent，主动 report 有频率上限。
    fn check_staff_extras(&mut self, op: &BrainOp, active: Option<&str>) -> Result<(), String> {
        match op {
            BrainOp::History { agent, last, .. } => {
                let open_card = self
                    .agent_roster_for_provider()
                    .into_iter()
                    .find(|entry| entry.name == *agent)
                    .and_then(|entry| entry.active_node_id);
                let in_task = active
                    .and_then(|id| self.staff.tasks.get(id))
                    .is_some_and(|task| history_in_task(task, open_card));
                let within_cap = last.is_none_or(|n| n <= HISTORY_MAX_LAST);
                (in_task && within_cap)
                    .then_some(())
                    .ok_or_else(|| "staff.denied.history_scope".to_owned())
            }
            BrainOp::Report { .. } => self
                .brain
                .report_limiter
                .allow(active.unwrap_or_default(), std::time::Instant::now())
                .then_some(())
                .ok_or_else(|| "staff.denied.report_rate".to_owned()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_authorize_tests.rs"]
mod tests;
