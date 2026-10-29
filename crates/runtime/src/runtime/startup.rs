//! Agent 启动看门狗在 Runtime 层的武装、推进与查询入口。
use super::*;
use crate::startup_watch::StartupWatch;
use std::time::Instant;

/// 武装条件的两半：哪些 profile 的 hook 已安装，以及每个新建节点的计时器。
#[derive(Debug, Default)]
pub(crate) struct StartupWatchdog {
    hook_profiles: Vec<AgentProfile>,
    watches: HashMap<NodeId, StartupWatch>,
}

impl StartupWatchdog {
    pub(super) fn clear_watches(&mut self) {
        self.watches.clear();
    }
}

impl Runtime {
    /// 设置「hook 已安装」的 profile 集合；只影响之后新建的卡片，已有计时器不受影响。
    pub fn set_startup_watch_profiles(&mut self, profiles: impl IntoIterator<Item = AgentProfile>) {
        self.startup_watchdog.hook_profiles = profiles.into_iter().collect();
    }

    /// 仅当该 profile 已装 hook 且 hook 通道可用时才计时；否则 hook 永远不会到，不得误报。
    pub(super) fn arm_startup_watch(&mut self, node_id: NodeId, profile: AgentProfile) {
        if self.agent_hook_config.is_some()
            && self.startup_watchdog.hook_profiles.contains(&profile)
        {
            self.startup_watchdog
                .watches
                .insert(node_id, StartupWatch::new(Instant::now()));
        }
    }

    /// 仅被武装且仍未收到 hook 的 Agent 节点报告 stalled；Terminal 与不存在的节点恒为 false。
    pub fn agent_startup_stalled(&self, node_id: NodeId) -> bool {
        self.startup_watchdog
            .watches
            .get(&node_id)
            .is_some_and(StartupWatch::is_stalled)
            && self.events.agent_task_state(node_id).is_none()
    }

    /// 推进所有被武装节点的看门狗；任一节点刚越过阈值返回 true。
    pub fn tick_startup_stalls(&mut self, now: Instant) -> bool {
        let live = &self.live;
        let events = &self.events;
        let watches = &mut self.startup_watchdog.watches;
        watches.retain(|node_id, _| live.contains_key(node_id));
        let mut crossed = false;
        for (node_id, watch) in watches.iter_mut() {
            if events.agent_task_state(*node_id).is_some() {
                watch.note_hook_seen();
            }
            crossed |= watch.tick(now);
        }
        crossed
    }
}
