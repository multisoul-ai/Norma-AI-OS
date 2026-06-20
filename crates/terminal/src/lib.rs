//! terminal — 域层（零 gpui）：pty、session、grid、input、通道消息。
pub mod input;
pub mod message;
pub mod pty;

pub use input::{encode, Key, KeyInput};
pub use message::PtyEvent;
pub use pty::{PtyError, PtyHandle};
