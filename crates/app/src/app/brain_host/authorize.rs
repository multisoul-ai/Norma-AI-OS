//! 命令入口的统一授权：按 token 认出角色，再按角色的白名单和当前轮次的任务检查。
use super::MultisoulApp;
use brain::staff::gate::{authorize, BrainRole, GateContext};
use brain::staff::policy::Verdict;
use norma::protocol::{BrainOp, TaskPhase};

impl MultisoulApp {
    /// 返回拒绝码（`staff.*`），由展示边界翻译成中英文。
    pub(crate) fn authorize_brain(&mut self, token: &str, op: &BrainOp) -> Result<(), String> {
        let role = self
            .brain
            .role(token)
            .ok_or_else(|| "staff.invalid_source".to_owned())?;
        let active_task = self
            .staff
            .activity
            .iter()
            .find(|(_, activity)| activity.active)
            .map(|(id, _)| id.as_str());
        let task_done = match op {
            BrainOp::Task { op } => op
                .task_id()
                .and_then(|id| self.staff.tasks.get(id))
                .is_some_and(|task| task.phase == TaskPhase::Done),
            _ => false,
        };
        let active_task = active_task.map(str::to_owned);
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

    /// 白名单之外的两条幕僚限制：history 只读本任务派出过的 Agent，主动 report 有频率上限。
    fn check_staff_extras(&mut self, op: &BrainOp, active: Option<&str>) -> Result<(), String> {
        match op {
            BrainOp::History { agent, .. } => {
                let dispatched = active
                    .and_then(|id| self.staff.tasks.get(id))
                    .is_some_and(|task| task.workers.iter().any(|worker| worker.agent == *agent));
                dispatched
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
