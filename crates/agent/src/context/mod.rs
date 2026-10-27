//! Shared context domain. No database, network, UI, or audio dependencies.
mod assembly;
mod gate;
mod handoff;
mod history;
pub use handoff::context_task_handoff;
mod projection;
mod read;
pub use read::context_read_result;
mod render;
mod selection;
mod snapshot;
mod update;
pub use render::{render_context_frame, render_context_frame_measured, SHARED_CONTEXT_RULES};
pub(crate) mod tools;
pub use assembly::{
    assemble_context, AssemblyRequest, ContextAssembly, ContextAssemblyError, ContextBudget,
    ContextQuery,
};
pub use gate::ContextExecutionGate;
pub use norma::protocol::{
    ConstraintEnforcement, ContextBundle, ContextConstraint, ContextConsumer, ContextEvent,
    ContextFact, ContextHistoryTurn, ContextInput, ContextManifest, ContextMaterial,
    ContextMaterialKind, ContextOperation, ContextOrigin, ContextPayload, ContextProject,
    ContextRef, ContextReport, ContextReportIndex, ContextScope, ContextSelection, ContextSnapshot,
    ContextSource, ContextStateView, DecisionStatus, ReceiptStatus, RecordedContextEvent,
    ReportDelivery, WorkingState,
};
pub use norma::protocol::{ContextStateChange, ContextStateUpdate};
pub use projection::{check_context_operation, project_event, report_delivery_is_terminal};
pub use update::validate_state_update;

#[derive(Clone, Debug)]
pub struct ContextFrame {
    pub input: ContextRef,
    pub snapshot: ContextSnapshot,
}
