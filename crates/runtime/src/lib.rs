//! runtime — 域层（零 gpui）：节点生命周期、焦点、输入路由。
pub mod agent_hooks;
pub mod events;
pub mod focus;
pub mod live;
pub mod nodes;
pub mod route;
pub mod runtime;
mod runtime_support;
pub mod snapshot;

pub use agent_hooks::AgentHookRuntimeConfig;
pub use events::{
    AgentTaskState, AgentTaskUpdate, CardAttentionKind, CardAttentionState, NodeRunState,
    RuntimeEvent,
};
pub use focus::{FocusState, InputMode};
pub use live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeScroll, NodeView};
pub use route::{route, InputEvent, Route};
pub use runtime::{Runtime, DEFAULT_TERMINAL_SHELL};
