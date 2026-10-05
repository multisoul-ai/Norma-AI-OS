//! Encrypted mobile transport and pairing. Independent of GPUI and Provider credentials.
mod approval;
pub use approval::{
    redacted_external_hook_id, ApprovalListRequest, ApprovalSnapshot, ApprovalTarget,
};
mod conversation;
mod conversation_events;
pub use conversation_events::{ConversationEvent, ConversationProjection};
mod conversation_transfer;
pub use conversation_transfer::{
    ConversationFragment, CONVERSATION_CHUNK_BYTES, MAX_CONVERSATION_BYTES,
};
mod cloud_registration;
mod identity;
mod relay;
mod relay_session;
mod thread_message;
mod workspace_chat;
mod workspace_terminal;
pub use workspace_terminal::{
    WorkspaceTerminalCell, WorkspaceTerminalFragment, WorkspaceTerminalFrame,
    WorkspaceTerminalRequest, WorkspaceTerminalUpdate,
};
mod agent_creation;
pub use agent_creation::WorkspaceAgentCreateRequest;
pub use thread_message::ThreadMessage;
pub use workspace_chat::{
    DesktopChatAgent, DesktopChatDefinition, DesktopChatProject, DesktopChatWorkspace,
    WorkspaceAgentTarget, WorkspaceChatMessage, WorkspaceConfigurationChange,
    WorkspaceMessageIntent,
};
mod thread_task;
pub use thread_task::ThreadTask;
mod thread_followup;
pub use relay_session::{RelayHandshake, RelayHello, RelaySession};
pub use thread_followup::ThreadFollowup;
mod voice_authorization;
pub use conversation::{ConversationItem, ConversationPage, ConversationRequest};
pub use voice_authorization::VOICE_GATEWAY_ORIGIN;
mod pairing;
mod project_events;
mod snapshot;
mod workspace_activity;
pub use project_events::{ProjectChange, ProjectEvent, ProjectProjection};
pub use workspace_activity::*;
mod terminal;
mod transport;
pub use terminal::{
    TerminalCursor, TerminalEvent, TerminalInput, TerminalPage, TerminalProjection, TerminalRequest,
};

pub use identity::ServerIdentity;
pub use pairing::{PairingCodeReply, PairingPending, PairingQr, PairingRequest, PairingSession};
pub use snapshot::{CardSnapshot, ProjectSnapshot, TaskProgress, TaskTiming};
pub use transport::{accept_mobile_websocket, MobileSocket, MobileTcp};
