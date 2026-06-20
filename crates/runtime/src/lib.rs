//! runtime — 域层（零 gpui）：节点生命周期、焦点、输入路由。
pub mod event_router;
pub mod focus;
pub mod runtime;
pub mod snapshot;

pub use event_router::{route, InputEvent, Route};
pub use focus::{FocusState, InputMode};
pub use runtime::Runtime;
