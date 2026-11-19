//! 最小 ACP client：只用到 initialize、session/new、session/set_mode、session/prompt、
//! session/request_permission。手写是为了少一层依赖，权限请求一律放行。

mod claude_auth;
pub mod client;
mod codex_compat;
mod content;
mod handshake;
pub mod hub;
mod permission;
pub mod questions;
mod route;
mod staff_shell;
pub mod trace;
pub mod wire;

pub(crate) use content::PromptImage;
