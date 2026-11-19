use super::*;
use crate::NodeRunState;
use std::time::Instant;

/// 一张卡上的派发身份。`spawn` 是进程启动时写进环境变量的编号，改绑后不变；`current` 是事件要记到的这一轮。
#[derive(Clone, Debug)]
pub(crate) struct StaffBinding {
    pub spawn: String,
    pub current: String,
    pub model: Option<String>,
}

impl Runtime {
    /// Reserve a fresh card; persistence must succeed before the caller starts its terminal.
    pub fn create_staff_worker(
        &mut self,
        profile: AgentProfile,
        alias: String,
        bounds: CanvasRect,
        dispatch_id: String,
    ) -> NodeId {
        let node = self.add_agent_node(profile, None, Some(alias), bounds);
        self.staff_dispatches.insert(
            node,
            StaffBinding {
                spawn: dispatch_id.clone(),
                current: dispatch_id,
                model: None,
            },
        );
        node
    }

    /// 复用一张已经在跑的卡。进程里的 `NORMA_DISPATCH_ID` 保持启动时的值，事件改记到新编号。
    /// 用户自己开的卡还没有绑定，这时启动编号和当前编号都是这一轮。
    pub fn rebind_staff_dispatch(&mut self, node: NodeId, dispatch_id: String) {
        if let Some(binding) = self.staff_dispatches.get_mut(&node) {
            binding.current = dispatch_id;
        } else {
            self.staff_dispatches.insert(
                node,
                StaffBinding {
                    spawn: dispatch_id.clone(),
                    current: dispatch_id,
                    model: None,
                },
            );
        }
    }

    /// Model chosen for a Staff worker; applied when its native terminal is prepared.
    pub fn set_staff_model(&mut self, node: NodeId, model: String) {
        if let Some(entry) = self.staff_dispatches.get_mut(&node) {
            entry.model = Some(model);
        }
    }

    /// Append `--model`; engines without a verified flag refuse instead of silently ignoring it.
    pub(crate) fn with_staff_model(
        &self,
        node: NodeId,
        profile: canvas::AgentProfile,
        mut command: terminal::PtyCommand,
    ) -> Result<terminal::PtyCommand, terminal::PtyError> {
        let Some(model) = self
            .staff_dispatches
            .get(&node)
            .and_then(|binding| binding.model.as_deref())
        else {
            return Ok(command);
        };
        use canvas::AgentProfile::*;
        if !matches!(profile, Claude | Codex | Cursor | OpenCode | Kimi | Grok) {
            return Err(terminal::PtyError::Spawn(
                "model selection unsupported".into(),
            ));
        }
        let script = command
            .args
            .last_mut()
            .ok_or_else(|| terminal::PtyError::Spawn("empty agent command".into()))?;
        script.push_str(" --model ");
        script.push_str(&crate::session_snapshot::shell_quote(model));
        Ok(command)
    }

    /// 事件要记到的这一轮编号。改绑之后和进程环境变量里的启动编号可以不同。
    pub fn staff_dispatch_id(&self, node: NodeId) -> Option<&str> {
        self.staff_dispatches
            .get(&node)
            .map(|binding| binding.current.as_str())
    }

    /// 用户自己开的卡没有派发绑定；能认领的前提是这个进程的 run id 已经记下来。
    pub fn staff_card_claimable(&self, node: NodeId) -> bool {
        self.native_question_process_id(node).is_some()
    }

    /// 绑卡失败时撤掉刚加上的绑定，避免事件记到一个没有任务的编号。
    pub fn clear_staff_dispatch(&mut self, node: NodeId) {
        self.staff_dispatches.remove(&node);
    }

    /// 进程启动时注入的编号。hook 里的 `_norma_dispatch_id` 对的是它。
    pub fn staff_spawn_id(&self, node: NodeId) -> Option<&str> {
        self.staff_dispatches
            .get(&node)
            .map(|binding| binding.spawn.as_str())
    }

    pub fn accepts_staff_hook(&self, event: &norma::agent_hook::AgentHookEvent) -> bool {
        let Some(binding) = self.staff_dispatches.get(&event.node_id) else {
            return true;
        };
        match event.raw_json.get("_norma_dispatch_id") {
            Some(id) => id.as_str() == Some(binding.spawn.as_str()),
            // 旧 helper 不带派发编号，只认这个进程自己的 run id。
            None => event
                .control_run_id
                .as_deref()
                .is_some_and(|run| self.native_question_process_id(event.node_id) == Some(run)),
        }
    }

    pub fn last_output_at(&self, node: NodeId) -> Option<Instant> {
        self.live.get(&node).and_then(|live| live.last_output_at())
    }

    pub fn terminal_has_exited(&self, node: NodeId) -> bool {
        matches!(
            self.node_run_state(node),
            Some(
                NodeRunState::Finished { .. } | NodeRunState::Error { .. } | NodeRunState::Stopped
            )
        )
    }
}
