//! WebView2 pumps Win32 messages while constructing COM controllers. All native work
//! must run on the foreground STA with no GPUI App or RefCell borrow held.
use super::super::generation::{Generation, GenerationOwner};
use super::{BrowserEvent, NodeId, Sender};
use gpui::{AnyWindowHandle, App, AppContext, AsyncApp, ForegroundExecutor, Window};
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

type Action = Box<dyn FnOnce(Result<&wry::WebView, &str>)>;
struct Pending {
    key: Option<&'static str>,
    action: Action,
}

#[derive(Clone)]
struct Lease {
    app: AsyncApp,
    window: AnyWindowHandle,
    parent: RawWindowHandle,
}
impl Lease {
    fn alive(&self) -> bool {
        let mut app = self.app.clone();
        app.update_window(self.window, |_, window, _| {
            HasWindowHandle::window_handle(window)
                .is_ok_and(|handle| handle.as_raw() == self.parent)
        })
        .unwrap_or(false)
    }
}
struct Parent(RawWindowHandle);
impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // The generational GPUI window lease is checked before and after construction.
        // This copies the OS handle; it never retains a Rust Window reference across a message pump.
        Ok(unsafe { WindowHandle::borrow_raw(self.0) })
    }
}

#[derive(Clone)]
pub(super) struct Events {
    owner: Generation,
    lease: Lease,
    sender: Option<Sender<BrowserEvent>>,
}
impl Events {
    pub(super) fn send(&self, event: BrowserEvent) {
        if self.owner.is_current() && self.lease.alive() {
            if let Some(sender) = &self.sender {
                let _ = sender.send(BrowserEvent::Native(self.owner.queue(Box::new(event))));
            }
        }
    }
}
struct State {
    executor: ForegroundExecutor,
    lease: Lease,
    native: RefCell<Option<wry::WebView>>,
    failure: RefCell<Option<String>>,
    pending: RefCell<VecDeque<Pending>>,
    running: Cell<bool>,
    cancelled: Cell<bool>,
    generation: Generation,
}
impl State {
    fn cancel(&self) {
        self.cancelled.set(true);
        self.generation.invalidate();
        self.pending.borrow_mut().clear();
    }
}
pub(super) struct DeferredWebView {
    state: Rc<State>,
    _owner: GenerationOwner,
    pub(super) events: Events,
}
impl DeferredWebView {
    pub(super) fn create(
        window: &Window,
        cx: &App,
        node: Option<NodeId>,
        sender: Option<Sender<BrowserEvent>>,
        builder: impl FnOnce(Events) -> wry::WebViewBuilder<'static>,
    ) -> wry::Result<Self> {
        let parent = HasWindowHandle::window_handle(window)?.as_raw();
        let lease = Lease {
            app: cx.to_async(),
            window: window.window_handle(),
            parent,
        };
        let owner = GenerationOwner::new();
        let events = Events {
            owner: owner.lease(),
            lease: lease.clone(),
            sender,
        };
        let state = Rc::new(State {
            executor: cx.foreground_executor().clone(),
            lease,
            native: RefCell::new(None),
            failure: RefCell::new(None),
            pending: RefCell::new(VecDeque::new()),
            running: Cell::new(true),
            cancelled: Cell::new(false),
            generation: owner.lease(),
        });
        let weak = Rc::downgrade(&state);
        let builder = builder(events.clone());
        let failure_events = events.clone();
        state
            .executor
            .spawn(async move {
                let Some(state) = weak.upgrade() else { return };
                if state.cancelled.get() || !state.lease.alive() {
                    state.cancel();
                    return;
                }
                // No App/Window/RefCell borrow may span this call: Wry dispatches arbitrary Win32 messages.
                let create_started = std::time::Instant::now();
                let result = builder.build_as_child(&Parent(parent));
                if state.cancelled.get() || !state.lease.alive() {
                    state.cancel();
                    drop(result);
                    return;
                }
                match result {
                    Ok(view) => {
                        tracing::info!(node_id = ?node, create_ms = create_started.elapsed().as_secs_f64() * 1000.0, "native_webview_ready");
                        state.native.replace(Some(view));
                    }
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(%message, "native WebView2 creation failed");
                        state.failure.replace(Some(message.clone()));
                        if let Some(node_id) = node {
                            failure_events
                                .send(BrowserEvent::NativeViewFailed { node_id, message });
                        }
                    }
                }
                drain(&state);
            })
            .detach();
        Ok(Self {
            state,
            _owner: owner,
            events,
        })
    }
    pub(super) fn is_attached_to(&self, window: &Window) -> bool {
        !self.state.cancelled.get() && self.state.lease.window == window.window_handle()
    }
    pub(super) fn run(&self, action: impl FnOnce(&wry::WebView) + 'static) {
        self.enqueue(None, move |result| {
            if let Ok(view) = result {
                action(view);
            }
        });
    }
    pub(super) fn latest(&self, key: &'static str, action: impl FnOnce(&wry::WebView) + 'static) {
        self.enqueue(Some(key), move |result| {
            if let Ok(view) = result {
                action(view);
            }
        });
    }
    pub(super) fn outcome(&self, action: impl FnOnce(Result<&wry::WebView, &str>) + 'static) {
        self.enqueue(None, action);
    }
    fn enqueue(
        &self,
        key: Option<&'static str>,
        action: impl FnOnce(Result<&wry::WebView, &str>) + 'static,
    ) {
        if self.state.cancelled.get() {
            return;
        }
        {
            let mut pending = self.state.pending.borrow_mut();
            if let Some(key) = key {
                // Navigation/cookie commands are ordering barriers. Coalesce only the
                // declarative layout tail after the most recent ordered command.
                let index = pending
                    .iter()
                    .enumerate()
                    .rev()
                    .take_while(|(_, item)| item.key.is_some())
                    .find(|(_, item)| item.key == Some(key))
                    .map(|(index, _)| index);
                if let Some(index) = index {
                    pending.remove(index);
                }
            }
            pending.push_back(Pending {
                key,
                action: Box::new(action),
            });
        }
        schedule(&self.state);
    }
}
impl Drop for DeferredWebView {
    fn drop(&mut self) {
        self.state.cancel();
        let native = self.state.native.borrow_mut().take();
        // Closing a COM controller may pump messages too; never destroy it from an App update.
        self.state
            .executor
            .spawn(async move {
                drop(native);
            })
            .detach();
    }
}
fn schedule(state: &Rc<State>) {
    if state.running.replace(true) {
        return;
    }
    let weak = Rc::downgrade(state);
    state
        .executor
        .spawn(async move {
            if let Some(state) = weak.upgrade() {
                drain(&state);
            }
        })
        .detach();
}
fn drain(state: &Rc<State>) {
    // Take both native state and each command out before calling native code. A nested message
    // pump may enqueue/cancel, but running remains true so it cannot start a second native owner.
    let native = state.native.borrow_mut().take();
    let failure = state.failure.borrow().clone();
    for _ in 0..32 {
        if state.cancelled.get() || !state.lease.alive() {
            state.cancel();
            return; // native is dropped here, outside the App borrow.
        }
        let pending = { state.pending.borrow_mut().pop_front() };
        let Some(pending) = pending else { break };
        (pending.action)(
            native
                .as_ref()
                .ok_or(failure.as_deref().unwrap_or("WebView2 is unavailable")),
        );
    }
    if state.cancelled.get() || !state.lease.alive() {
        state.cancel();
        return;
    }
    state.native.replace(native);
    state.running.set(false);
    if !state.pending.borrow().is_empty() {
        schedule(state);
    }
}
