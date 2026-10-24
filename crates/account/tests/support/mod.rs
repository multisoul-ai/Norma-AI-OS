//! 内存假实现与最小驱动：证明 `AccountSessionStore` / `AuthApi` 接缝可替换。

use account::{
    AccountId, AccountSessionStore, AuthApi, AuthError, Flow, FlowAction, FlowEvent, Identity,
    Pkce, Profile, Provider, Session, StoreError, TokenRequest,
};
use std::cell::RefCell;

#[derive(Default)]
pub struct MemStore {
    pub session: RefCell<Option<Session>>,
    pub dismissed: RefCell<bool>,
}

impl AccountSessionStore for MemStore {
    fn load(&self) -> Option<Session> {
        self.session.borrow().clone()
    }
    fn save(&self, session: &Session) -> Result<(), StoreError> {
        *self.session.borrow_mut() = Some(session.clone());
        Ok(())
    }
    fn clear(&self) -> Result<(), StoreError> {
        *self.session.borrow_mut() = None;
        Ok(())
    }
    fn login_prompt_dismissed(&self) -> bool {
        *self.dismissed.borrow()
    }
    fn set_login_prompt_dismissed(&self, dismissed: bool) -> Result<(), StoreError> {
        *self.dismissed.borrow_mut() = dismissed;
        Ok(())
    }
}

pub struct FakeApi {
    pub fail_with: Option<AuthError>,
    pub seen: RefCell<Vec<TokenRequest>>,
}

impl FakeApi {
    pub fn ok() -> Self {
        Self {
            fail_with: None,
            seen: RefCell::default(),
        }
    }
    pub fn failing(error: AuthError) -> Self {
        Self {
            fail_with: Some(error),
            seen: RefCell::default(),
        }
    }
}

pub fn profile() -> Profile {
    Profile {
        account_id: AccountId::new("acc_1"),
        display_name: "Zhou Kai".to_string(),
        email: None,
        avatar_url: None,
        identities: vec![Identity {
            provider: Provider::Google,
            subject: "g-1".to_string(),
        }],
    }
}

impl AuthApi for FakeApi {
    fn exchange(&self, request: &TokenRequest) -> Result<Session, AuthError> {
        self.seen.borrow_mut().push(request.clone());
        match &self.fail_with {
            Some(error) => Err(error.clone()),
            None => Ok(Session::new("a".into(), "r".into(), 10_000, profile())),
        }
    }
    fn refresh(&self, _: &str) -> Result<Session, AuthError> {
        Err(AuthError::Unauthorized)
    }
    fn me(&self, _: &str) -> Result<Profile, AuthError> {
        Ok(profile())
    }
    fn logout(&self, _: &str) -> Result<(), AuthError> {
        Ok(())
    }
}

/// 执行动作并把结果回灌状态机，直到没有后续动作。
pub fn drive(
    flow: &mut Flow,
    api: &dyn AuthApi,
    store: &dyn AccountSessionStore,
    event: FlowEvent,
) {
    let mut next = flow.handle(event);
    while let Some(action) = next.take() {
        let follow_up = match action {
            FlowAction::OpenBrowser(_) => None,
            FlowAction::ExchangeCode(request) => Some(match api.exchange(&request) {
                Ok(session) => FlowEvent::ExchangeSucceeded(session),
                Err(error) => FlowEvent::ExchangeFailed(error.to_string()),
            }),
            FlowAction::PersistSession(session) => {
                store.save(&session).expect("内存存储不应失败");
                None
            }
            FlowAction::ClearSession => {
                store.clear().expect("内存存储不应失败");
                None
            }
        };
        if let Some(event) = follow_up {
            next = flow.handle(event);
        }
    }
}

pub fn attempt(provider: Provider) -> account::Attempt {
    account::Attempt {
        provider,
        pkce: Pkce::from_entropy(&[9; 32]),
        state: "st".to_string(),
    }
}
