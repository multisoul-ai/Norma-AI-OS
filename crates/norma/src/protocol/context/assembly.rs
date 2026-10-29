use super::{ContextRef, ContextScope, ReportDelivery, WorkingState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextConsumer {
    Legacy,
    Doubao,
    LiveConversation,
    LivePlanner,
    Brain,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMaterialKind {
    Memory,
    Brain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextMaterial {
    pub reference: ContextRef,
    pub kind: ContextMaterialKind,
    pub text: String,
    pub related: Vec<ContextRef>,
    pub delivery: Option<ReportDelivery>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextInput {
    Text {
        text: String,
    },
    /// Audio and native history remain in the transport. This is only their identity.
    NativeAudio {
        session_id: String,
        revision: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextHistoryTurn {
    pub turn_id: String,
    pub required: bool,
    pub messages: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextStateView {
    pub scope: ContextScope,
    pub revision: u64,
    pub state: WorkingState,
    pub recent_reports: Vec<ContextReportIndex>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextReportIndex {
    pub reference: ContextRef,
    pub delivery: ReportDelivery,
}

/// Seven logical groups. Adapters must keep groups 2–5 as attributed data,
/// never concatenate them into the trusted instruction group or a user utterance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextBundle {
    pub consumer: ContextConsumer,
    pub turn_id: String,
    pub snapshot_id: String,
    pub scope: ContextScope,
    pub system_rules: String,
    pub environment: String,
    pub working_state: Vec<ContextStateView>,
    pub memories: Vec<ContextMaterial>,
    pub brain: Vec<ContextMaterial>,
    pub history: Vec<ContextHistoryTurn>,
    pub input: ContextInput,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextSelection {
    pub reference: ContextRef,
    pub selected: bool,
    pub reason: String,
    pub bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextManifest {
    pub snapshot_id: String,
    pub consumer: ContextConsumer,
    /// UTF-8 bytes of the serialized bundle, not a vendor token count.
    pub bundle_bytes: usize,
    pub reserved_bytes: usize,
    pub entries: Vec<ContextSelection>,
    pub dropped_history: Vec<String>,
}
