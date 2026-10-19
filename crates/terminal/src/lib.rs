//! terminal — 域层（零 gpui）：pty、session、grid、input、通道消息。
pub mod grid;
pub mod input;
pub mod message;
mod paste;
mod process_tree;
pub mod pty;
pub mod session;
pub mod test_backend;

pub use grid::{
    parse_path_ref, terminal_wheel_action, Cell, CellAttrs, GridSnapshot, PathRef, PathRefSpan,
    PathTier, TerminalColor, TerminalGrid, TerminalNamedColor, TerminalWheelAction,
};
pub use input::{encode, Key, KeyInput};
pub use message::PtyEvent;
pub use process_tree::ChildProcessObserver;
pub use pty::{PtyBackend, PtyCommand, PtyError, PtyHandle};
pub use session::{TerminalSession, TerminalSessionId, TerminalStatus};
pub use test_backend::{ScriptRule, TestPtyBackend};
