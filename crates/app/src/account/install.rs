//! 启动时安装账号控制器并决定是否显示登录页。

use super::{
    account_base_url, account_login_enabled, should_show_login, skip_login_requested,
    AccountController, AccountHandle, AppDbAccountStore, CloudAuthApi, LoginGateInputs,
};
use gpui::{App, AppContext as _};
use std::sync::Arc;

/// 返回是否需要在工作台之前显示登录页。功能关闭（默认）时不读库、不安装、返回 false。
pub(crate) fn install_account_controller(cx: &mut App) -> bool {
    if !account_login_enabled() {
        return false;
    }
    let app_store = match crate::database::default_database_context() {
        Ok((_, app_store)) => app_store,
        Err(error) => {
            tracing::warn!(%error, "account login disabled: database unavailable");
            return false;
        }
    };
    let base_url = account_base_url();
    let controller = AccountController::new(
        Arc::new(AppDbAccountStore::new(app_store.settings_store())),
        Arc::new(CloudAuthApi::new(base_url.clone())),
        base_url,
    );
    let show_login = should_show_login(LoginGateInputs {
        enabled: true,
        skip: skip_login_requested(),
        has_session: controller.has_session(),
        dismissed: controller.login_prompt_dismissed(),
    });
    let entity = cx.new(|_| controller);
    cx.set_global(AccountHandle(entity));
    show_login
}
