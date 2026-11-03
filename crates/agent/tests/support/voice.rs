use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentRosterEntry,
    AgentTaskState, AgentTrackingMode, ToolContext,
};
use futures::future::BoxFuture;
use norma::protocol::AgentProfile;
use std::sync::Mutex;

#[derive(Default)]
pub struct Host(pub Mutex<Vec<AdapterCommand>>);
impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0.lock().expect("测试宿主记录锁").push(command);
            Ok(AdapterResult::accepted("fixture-received").with_task_id(7))
        })
    }
}

pub fn member(id: i64, name: &str, node: Option<u64>) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: id,
        name: name.into(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: node,
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: AgentTaskState::Ready,
    }
}
