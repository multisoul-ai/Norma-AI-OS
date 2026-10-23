//! 登录控制器：把 `account::Flow` 的动作翻译成后台任务，并向界面暴露只读状态。
//!
//! 决策全部在 `Flow` 中；这里只做副作用（回环监听、网络、持久化、打开浏览器）。
//! 每个异步结果都带「代数」，取消或重新开始后迟到的结果会被丢弃。

use super::{LoopbackError, LoopbackListener};
use account::{
    random_state, start_url, AccountSessionStore, Attempt, AuthApi, AuthorizeRequest, Flow,
    FlowAction, FlowEvent, FlowState, Pkce, Profile, Provider, Session,
};
use gpui::{ClipboardItem, Context, Entity, Global};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// 等待浏览器完成授权的上限（规格：5 分钟）。
const BROWSER_TIMEOUT: Duration = Duration::from_secs(5 * 60);

type Store = Arc<dyn AccountSessionStore + Send + Sync>;
type Api = Arc<dyn AuthApi + Send + Sync>;

/// 全局句柄：GateRoot 与设置页通过它读取同一个控制器。
pub(crate) struct AccountHandle(pub(crate) Entity<AccountController>);
impl Global for AccountHandle {}

pub(crate) struct AccountController {
    flow: Flow,
    session: Option<Session>,
    store: Store,
    api: Api,
    base_url: String,
    cancel: Arc<AtomicBool>,
    generation: u64,
    /// 当前授权地址：供「重新打开浏览器」与「复制链接」复用。
    authorize_url: Option<String>,
}

impl AccountController {
    /// 启动时只读本地值，不联网；读取失败视为未登录。
    pub(crate) fn new(store: Store, api: Api, base_url: String) -> Self {
        let session = store.load();
        let flow = match &session {
            Some(session) => Flow::signed_in(session.profile.clone()),
            None => Flow::new(),
        };
        Self {
            flow,
            session,
            store,
            api,
            base_url,
            cancel: Arc::new(AtomicBool::new(false)),
            generation: 0,
            authorize_url: None,
        }
    }

    pub(crate) fn state(&self) -> &FlowState {
        self.flow.state()
    }

    pub(crate) fn profile(&self) -> Option<&Profile> {
        match self.flow.state() {
            FlowState::SignedIn(profile) => Some(profile),
            _ => None,
        }
    }

    pub(crate) fn has_session(&self) -> bool {
        self.session.is_some()
    }

    pub(crate) fn login_prompt_dismissed(&self) -> bool {
        self.store.login_prompt_dismissed()
    }

    /// 「稍后再说」：写入跳过标记；之后不再自动弹出启动登录页。
    pub(crate) fn dismiss_login_prompt(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = store.set_login_prompt_dismissed(true) {
                    tracing::warn!(error = %error.0, "failed to persist login prompt dismissal");
                }
            })
            .detach();
    }

    pub(crate) fn begin(&mut self, provider: Provider, cx: &mut Context<Self>) {
        let attempt = Attempt {
            provider,
            pkce: Pkce::generate(),
            state: random_state(),
        };
        self.cancel.store(true, Ordering::SeqCst); // 结束上一次尝试的监听
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.authorize_url = None;
        let action = self.flow.handle(FlowEvent::Begin(attempt));
        cx.notify();
        if let Some(FlowAction::OpenBrowser(request)) = action {
            self.launch(request, cx);
        }
    }

    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.cancel.store(true, Ordering::SeqCst);
        self.generation += 1;
        self.authorize_url = None;
        self.flow.handle(FlowEvent::Cancel);
        cx.notify();
    }

    pub(crate) fn reopen_browser(&self, cx: &mut Context<Self>) {
        if let Some(url) = &self.authorize_url {
            cx.open_url(url);
        }
    }

    pub(crate) fn copy_authorize_link(&self, cx: &mut Context<Self>) {
        if let Some(url) = &self.authorize_url {
            cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
        }
    }

    /// 退出登录：清本机会话并尽力通知服务端；之后不再自动弹出登录页。
    pub(crate) fn sign_out(&mut self, cx: &mut Context<Self>) {
        let token = self.session.take().map(|session| session.access_token);
        let action = self.flow.handle(FlowEvent::SignOut);
        cx.notify();
        if !matches!(action, Some(FlowAction::ClearSession)) {
            return;
        }
        let (store, api) = (self.store.clone(), self.api.clone());
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = store.clear() {
                    tracing::warn!(error = %error.0, "failed to clear account session");
                }
                if let Err(error) = store.set_login_prompt_dismissed(true) {
                    tracing::warn!(error = %error.0, "failed to persist login prompt dismissal");
                }
                if let Some(token) = token {
                    if let Err(error) = api.logout(&token) {
                        tracing::info!(%error, "server logout failed; local session already cleared");
                    }
                }
            })
            .detach();
    }

    fn launch(&mut self, request: AuthorizeRequest, cx: &mut Context<Self>) {
        let (generation, cancel, base_url) =
            (self.generation, self.cancel.clone(), self.base_url.clone());
        let bind = cx.background_executor().spawn(async move {
            let listener = LoopbackListener::bind()?;
            let url = start_url(&base_url, &request, listener.port());
            Ok::<_, LoopbackError>((url, listener))
        });
        cx.spawn(async move |this, cx| {
            let (url, listener) = match bind.await {
                Ok(bound) => bound,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.apply(
                            generation,
                            FlowEvent::ListenerFailed(format!("{error:?}")),
                            cx,
                        )
                    })
                    .ok();
                    return;
                }
            };
            let opened = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return false;
                }
                cx.open_url(&url);
                this.authorize_url = Some(url);
                true
            });
            if !matches!(opened, Ok(true)) {
                return;
            }
            let wait = cx
                .background_executor()
                .spawn(async move { listener.wait_for_callback(BROWSER_TIMEOUT, &cancel) });
            let event = match wait.await {
                Ok(target) => FlowEvent::CallbackReceived(target),
                Err(LoopbackError::Timeout) => FlowEvent::TimedOut,
                Err(LoopbackError::Cancelled) => return,
                Err(LoopbackError::Io(message)) => FlowEvent::ListenerFailed(message),
            };
            this.update(cx, |this, cx| this.apply(generation, event, cx))
                .ok();
        })
        .detach();
    }

    /// 把事件交给状态机并执行返回的动作；过期代数的事件直接丢弃。
    fn apply(&mut self, generation: u64, event: FlowEvent, cx: &mut Context<Self>) {
        if generation != self.generation {
            return;
        }
        let action = self.flow.handle(event);
        cx.notify();
        match action {
            Some(FlowAction::ExchangeCode(request)) => {
                let api = self.api.clone();
                let work = cx
                    .background_executor()
                    .spawn(async move { api.exchange(&request) });
                cx.spawn(async move |this, cx| {
                    let event = match work.await {
                        Ok(session) => FlowEvent::ExchangeSucceeded(session),
                        Err(error) => FlowEvent::ExchangeFailed(error.to_string()),
                    };
                    this.update(cx, |this, cx| this.apply(generation, event, cx))
                        .ok();
                })
                .detach();
            }
            Some(FlowAction::PersistSession(session)) => {
                self.session = Some(session.clone());
                let store = self.store.clone();
                cx.background_executor()
                    .spawn(async move {
                        if let Err(error) = store.save(&session) {
                            tracing::warn!(error = %error.0, "failed to persist account session");
                        }
                    })
                    .detach();
            }
            Some(FlowAction::OpenBrowser(_) | FlowAction::ClearSession) | None => {}
        }
    }
}
