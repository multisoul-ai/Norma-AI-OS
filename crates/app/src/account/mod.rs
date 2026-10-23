//! 账号登录的 App 适配层：回环监听、norma-cloud 客户端、AppDB 会话存储与启动提示决策。
//!
//! 纯逻辑与协议位于 `account` crate；这里只实现其端口并承载副作用。
//! 所有网络与监听都在后台线程执行，前台只更新状态。

mod client;
mod controller;
mod gate_phase;
mod install;
mod logos;
mod loopback;
mod store;

pub(crate) use client::{account_base_url, CloudAuthApi};
pub(crate) use controller::{AccountController, AccountHandle};
pub(crate) use gate_phase::{
    account_login_enabled, should_show_login, skip_login_requested, LoginGateInputs,
};
pub(crate) use install::install_account_controller;
pub(crate) use logos::{logo_asset, provider_logo};
pub(crate) use loopback::{LoopbackError, LoopbackListener};
pub(crate) use store::AppDbAccountStore;

#[cfg(test)]
#[path = "../../tests/unit/account_client_tests.rs"]
mod account_client_tests;
#[cfg(test)]
#[path = "../../tests/unit/account_gate_phase_tests.rs"]
mod account_gate_phase_tests;
#[cfg(test)]
#[path = "../../tests/unit/account_loopback_tests.rs"]
mod account_loopback_tests;
#[cfg(test)]
#[path = "../../tests/unit/account_store_tests.rs"]
mod account_store_tests;
