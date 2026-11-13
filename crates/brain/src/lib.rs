//! Norma 后台大脑：事件分流、ACP 会话与高风险规则。
//!
//! 本 crate 不依赖 GPUI、Runtime、Storage 与 Rig；动作一律通过 `norma` CLI 执行。

pub mod acp;
pub mod backend;
pub mod event;
pub mod goal;
pub mod history;
pub mod lane;
pub mod migration;
pub mod persistence;
pub mod preferences;
pub mod risk;
pub mod router;
pub mod scoped_router;
pub mod service;
pub mod staff;
pub mod task_outline;
pub mod triage;

pub use acp::client::discover_models;
pub use backend::{Backend, BackendKind, ModelChoice, Strategy};
pub use event::{
    ApprovalRequest, BrainEvent, BrainEventKind, BrainStatus, BrainStatusKind, Outcome, Trigger,
    TriggerKind,
};
pub use risk::ApprovalSubject;
pub use risk::{is_inside_project, is_secret_path, single_command};
pub use router::{Action, Lane, Router, RouterConfig};
pub use scoped_router::{ScopedAction, ScopedRouter};
pub use service::{BrainConfig, BrainService};
