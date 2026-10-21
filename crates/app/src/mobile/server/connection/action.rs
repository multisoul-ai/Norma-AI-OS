use super::push;
use norma::mobile::PairingRequest;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::mobile) enum Action {
    WorkspaceActivity {
        device_id: String,
        token: String,
        request: Box<crate::mobile::activity::Request>,
    },
    WorkspaceTerminal {
        device_id: String,
        token: String,
        request: Box<norma::mobile::WorkspaceTerminalRequest>,
    },
    WorkspaceAgentCreate {
        device_id: String,
        token: String,
        request: Box<norma::mobile::WorkspaceAgentCreateRequest>,
    },
    WorkspaceAgentCreationReceipt {
        device_id: String,
        token: String,
        desktop_workspace_id: String,
        operation_id: String,
    },
    WorkspaceCatalog {
        device_id: String,
        token: String,
    },
    WorkspaceMessage {
        device_id: String,
        token: String,
        message: Box<norma::mobile::WorkspaceChatMessage>,
    },
    WorkspaceReceipt {
        device_id: String,
        token: String,
        desktop_workspace_id: String,
        operation_id: String,
    },
    WorkspaceEvents {
        device_id: String,
        token: String,
        desktop_workspace_id: String,
        after: u64,
    },
    WorkspaceConfigure {
        device_id: String,
        token: String,
        desktop_workspace_id: String,
        expected_revision: u64,
        change: norma::mobile::WorkspaceConfigurationChange,
    },
    CloudRegistration {
        device_id: String,
        token: String,
        cloud_device_id: String,
        token_hash: String,
    },
    ThreadTaskFollowupReceipt {
        device_id: String,
        token: String,
        followup: Box<norma::mobile::ThreadFollowup>,
    },
    ThreadTaskTerminal {
        device_id: String,
        token: String,
        request: crate::mobile::thread_task_terminal::Request,
    },
    ThreadTaskAcceptance {
        device_id: String,
        token: String,
        decision: crate::mobile::thread_task_acceptance::Decision,
    },
    ThreadTaskReview {
        device_id: String,
        token: String,
        request: crate::mobile::thread_task_review::Request,
    },
    ThreadTaskReceipt {
        device_id: String,
        token: String,
        collaboration_workspace_id: String,
        thread_id: String,
        operation_id: String,
    },
    ThreadReceipt {
        device_id: String,
        token: String,
        collaboration_workspace_id: String,
        thread_id: String,
        operation_id: String,
    },
    ThreadEvents {
        device_id: String,
        token: String,
        collaboration_workspace_id: String,
        thread_id: String,
        after: u64,
    },
    VoiceAuthorization {
        device_id: String,
        token: String,
    },
    RealtimeOperations {
        device_id: String,
        token: String,
        workspace_id: String,
        #[serde(default)]
        after: u64,
    },
    BindRealtime {
        device_id: String,
        token: String,
        binding: super::realtime::Binding,
        gateway: Option<super::realtime::Gateway>,
    },
    UploadImage {
        device_id: String,
        token: String,
        workspace_id: String,
        grant_revision: u64,
        image_id: String,
        byte_count: usize,
    },
    ChatImage {
        device_id: String,
        token: String,
        workspace_id: String,
        image_id: String,
    },
    Terminal {
        device_id: String,
        token: String,
        request: norma::mobile::TerminalRequest,
        #[serde(default)]
        watch: bool,
    },
    WatchPlan {
        device_id: String,
        token: String,
        workspace_id: String,
        plan_id: Option<String>,
    },
    WatchApprovals {
        device_id: String,
        token: String,
    },
    ArchivedConversation {
        device_id: String,
        token: String,
        request: norma::mobile::ConversationRequest,
        profile: String,
    },
    SessionArchive {
        device_id: String,
        token: String,
        request: crate::mobile::ProjectReviewRequest,
        before: Option<u64>,
    },
    PairingCode {
        message: String,
    },
    TaskDeliveries {
        #[serde(default)]
        watch: bool,
        #[serde(default)]
        pending_only: bool,
        device_id: String,
        token: String,
        request: crate::mobile::ProjectReviewRequest,
        node_id: Option<u64>,
        before: Option<u64>,
    },
    ProjectReview {
        device_id: String,
        token: String,
        request: crate::mobile::ProjectReviewRequest,
    },
    ProjectPlan {
        device_id: String,
        token: String,
        workspace_id: String,
        plan_id: String,
    },
    WorkspaceIdeas {
        device_id: String,
        token: String,
        workspace_id: String,
    },
    ProjectPlans {
        device_id: String,
        token: String,
        workspace_id: String,
        before: Option<u64>,
    },
    WatchConversation {
        device_id: String,
        token: String,
        request: norma::mobile::ConversationRequest,
    },
    WatchProject {
        device_id: String,
        token: String,
        workspace_id: String,
    },
    Pair {
        request: PairingRequest,
    },
    Authenticate {
        device_id: String,
        token: String,
    },
    PushSubscription {
        device_id: String,
        token: String,
    },
    SetPushSubscription {
        device_id: String,
        token: String,
        expected_revision: u64,
        #[serde(deserialize_with = "push::registration")]
        registration: Option<storage::MobilePushRegistration>,
    },
    Snapshot {
        device_id: String,
        token: String,
        workspace_id: Option<String>,
    },
    Command {
        device_id: String,
        token: String,
        request: Box<crate::mobile::MobileCommandRequest>,
    },
    Approvals {
        device_id: String,
        token: String,
        request: norma::mobile::ApprovalListRequest,
    },
    Conversation {
        device_id: String,
        token: String,
        request: norma::mobile::ConversationRequest,
    },
    QueuedMessage {
        device_id: String,
        token: String,
        workspace_id: String,
        operation_id: String,
    },
    CancelQueuedMessage {
        device_id: String,
        token: String,
        workspace_id: String,
        operation_id: String,
        grant_revision: u64,
    },
    Receipt {
        device_id: String,
        token: String,
        workspace_id: String,
        operation_id: String,
    },
}
