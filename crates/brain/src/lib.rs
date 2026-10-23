//! Norma 后台大脑：事件分流、ACP 会话与高风险规则。
//!
//! 本 crate 不依赖 GPUI、Runtime、Storage 与 Rig；动作一律通过 `norma` CLI 执行。

pub mod risk;

pub use risk::{risk_class, ApprovalSubject, HighRiskReason, RiskClass};
