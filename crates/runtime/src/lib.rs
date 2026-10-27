//! runtime — 域层（零 gpui）：节点生命周期、焦点、输入路由。
pub mod agent_hooks;
mod agent_install_hint;
mod agent_session_bindings;
mod agent_task_label;
pub mod command;
pub mod desktop_drawers;
pub mod desktop_navigation;
pub mod ego;
pub mod events;
pub mod focus;
pub mod live;
pub(crate) mod macos_proc;
pub mod nodes;
mod pi_launch;
mod preview;
pub mod route;
pub mod runtime;
mod runtime_support;
pub(crate) mod session_restore;
pub mod session_snapshot;
pub mod snapshot;
pub mod startup_watch;
mod zcode_launch;

pub use agent_hooks::{
    path_convention_enabled, set_path_convention_enabled, AgentHookRuntimeConfig,
};
pub use agent_session_bindings::AgentSessionHydrationReport;
pub use ego::{EgoOwnership, EgoTaskCommand, EgoTaskError, EgoTaskState};
pub use events::{
    AgentTaskState, AgentTaskUpdate, AgentTaskUpdateOutcome, CardAttentionKind, CardAttentionState,
    NodeRunState, RuntimeEvent,
};
pub use focus::{FocusState, InputMode};
pub use live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeScroll, NodeView};
pub use nodes::term::PtyFactory;
pub use preview::PreviewSpawnError;
pub use route::{route, InputEvent, Route};
pub use runtime::{Runtime, TerminalInputLease, DEFAULT_TERMINAL_SHELL};
pub use session_restore::merge_session_checkpoints;
