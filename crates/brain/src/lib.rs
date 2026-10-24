//! Norma 后台大脑：事件分流、ACP 会话与高风险规则。
//!
//! 本 crate 不依赖 GPUI、Runtime、Storage 与 Rig；动作一律通过 `norma` CLI 执行。

pub mod acp;
pub mod backend;
pub mod event;
pub mod goal;
pub mod history;
pub mod lane;
pub mod risk;
pub mod router;
pub mod service;

pub use backend::{Backend, BackendKind, Strategy};
pub use event::{ApprovalRequest, BrainEvent, BrainStatus, Outcome};
pub use risk::{is_inside_project, is_secret_path, single_command};
pub use risk::{risk_class, ApprovalSubject, HighRiskReason, RiskClass};
pub use router::{Action, Lane, Router, RouterConfig};
pub use service::{BrainConfig, BrainService};
