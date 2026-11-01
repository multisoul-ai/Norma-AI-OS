use canvas::NodeId;

use super::Runtime;

impl Runtime {
    pub fn agent_child_pid(&self, node_id: NodeId) -> Option<u32> {
        self.live.get(&node_id)?.child_pid()
    }
}
