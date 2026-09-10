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
mod identity;
pub use conversation::{ConversationItem, ConversationPage, ConversationRequest};
#[cfg(target_os = "macos")]
mod keychain;
mod pairing;
mod project_events;
mod snapshot;
pub use project_events::{ProjectChange, ProjectEvent, ProjectProjection};
mod terminal;
mod transport;
pub use terminal::{
    TerminalEvent, TerminalInput, TerminalPage, TerminalProjection, TerminalRequest,
};

pub use identity::ServerIdentity;
pub use pairing::{PairingCodeReply, PairingPending, PairingQr, PairingRequest, PairingSession};
pub use snapshot::{CardSnapshot, ProjectSnapshot, TaskProgress, TaskTiming};
pub use transport::{accept_mobile_websocket, MobileSocket, MobileTcp};
