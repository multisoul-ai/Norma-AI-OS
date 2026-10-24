//! `AccountSessionStore` 的 AppDB 实现（ADR-0028：macOS 凭据存本机 `app.sqlite`）。

use account::{AccountSessionStore, Session, StoreError};
use storage::SettingsStore;

const SESSION_KEY: &str = "account.session";
const DISMISSED_KEY: &str = "account.login_prompt.dismissed";

#[derive(Clone, Debug)]
pub(crate) struct AppDbAccountStore {
    settings: SettingsStore,
}

impl AppDbAccountStore {
    pub(crate) fn new(settings: SettingsStore) -> Self {
        Self { settings }
    }
}

impl AccountSessionStore for AppDbAccountStore {
    /// 读取失败、损坏或版本过高一律视为未登录，不影响启动。
    fn load(&self) -> Option<Session> {
        let raw = self.settings.get_string(SESSION_KEY).ok().flatten()?;
        Session::decode(&raw).ok()
    }

    fn save(&self, session: &Session) -> Result<(), StoreError> {
        self.settings
            .set_string(SESSION_KEY, &session.encode())
            .map_err(|error| StoreError(error.to_string()))
    }

    fn clear(&self) -> Result<(), StoreError> {
        self.settings
            .delete_string(SESSION_KEY)
            .map_err(|error| StoreError(error.to_string()))
    }

    fn login_prompt_dismissed(&self) -> bool {
        matches!(
            self.settings.get_string(DISMISSED_KEY),
            Ok(Some(value)) if value == "1"
        )
    }

    fn set_login_prompt_dismissed(&self, dismissed: bool) -> Result<(), StoreError> {
        let result = if dismissed {
            self.settings.set_string(DISMISSED_KEY, "1")
        } else {
            self.settings.delete_string(DISMISSED_KEY)
        };
        result.map_err(|error| StoreError(error.to_string()))
    }
}
