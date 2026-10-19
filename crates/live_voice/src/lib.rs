//! Continuous voice sessions; audio hardware, credentials and desktop tools belong to callers.

mod connection;
mod context;
mod delegation;
mod pacing;
mod providers;
mod session;
mod transcript;

#[cfg(feature = "test-support")]
pub use connection::connect_loopback;
pub use connection::{connect, LiveConnection, LiveControl, LiveOutcome, UploadedAudio};
pub use context::{DelegationRequest, DelegationSnapshot, LiveContext};
pub use delegation::DelegationNotice;
pub use pacing::AudioPacer;
pub use session::{
    Acknowledgement, AppendKind, LiveCommand, LiveConfig, LiveError, LiveEvent, LiveSession,
    LiveUsage,
};
pub use transcript::{Speaker, TranscriptFragment};
