//! runtime — 域层（零 gpui）：节点生命周期、焦点、输入路由。
pub mod focus;
pub mod live;
pub mod nodes;
pub mod route;
pub mod runtime;
pub mod snapshot;

pub use focus::{FocusState, InputMode};
pub use live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeView};
pub use route::{route, InputEvent, Route};
pub use runtime::Runtime;
