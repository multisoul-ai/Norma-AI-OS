//! 最小 ACP client：只用到 initialize、session/new、session/set_mode、session/prompt、
//! session/request_permission。手写是为了少一层依赖，并完全掌控权限闸门。

pub mod client;
pub mod gate;
pub mod trace;
pub mod wire;
