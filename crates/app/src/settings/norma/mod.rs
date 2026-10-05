mod controller;
mod detail;
mod document;
mod import;
mod library;
pub(crate) mod persona;
mod view;
use agent::{MemoryPreferences, MemoryRevision, MemoryScopeAlias, MemorySummary};
use gpui::{App, AppContext, Context, Entity, EventEmitter, Subscription, Window};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use ui::component::input::{InputEvent, InputState};

pub(crate) struct MemoryRecallRevoked;
impl EventEmitter<MemoryRecallRevoked> for MemoryView {}

#[derive(Clone)]
struct PendingSave {
    expected: MemoryRevision,
    entries: Vec<(String, String)>,
    event: String,
    edit: String,
}

struct MemoryDraft {
    id: String,
    title: String,
    description: String,
    original_title: String,
    original_description: String,
    kind: Option<agent::MemoryKind>,
    modified: Option<String>,
    body: String,
    original: String,
    selected: Option<String>,
    version: MemoryRevision,
    imports: Vec<(String, String)>,
}

pub(crate) struct MemoryView {
    theme: ui::Theme,
    service: Arc<Mutex<storage::MemoryAccess>>,
    paths: storage::DatabasePaths,
    pub(super) root: Option<PathBuf>,
    scope: MemoryScopeAlias,
    entries: Vec<MemorySummary>,
    search_texts: std::collections::HashMap<String, String>,
    latest: Option<(MemoryRevision, agent::MemoryEntry)>,
    version: MemoryRevision,
    selected: Option<String>,
    sources: Vec<storage::MemorySourceDetail>,
    preferences: MemoryPreferences,
    search: Entity<InputState>,
    id_input: Entity<InputState>,
    title_input: Entity<InputState>,
    description_input: Entity<InputState>,
    original_title: String,
    original_description: String,
    kind: Option<agent::MemoryKind>,
    modified: Option<String>,
    body: Entity<InputState>,
    original: String,
    editing: bool,
    busy: bool,
    saving: bool,
    pending_save: Option<PendingSave>,
    generation: u64,
    drafts: std::collections::HashMap<MemoryScopeAlias, MemoryDraft>,
    status: ui::i18n::LocalizedText,
    error: bool,
    imports: Vec<(String, String)>,
    pub(crate) close_prompt: bool,
    _subscriptions: Vec<Subscription>,
}

impl MemoryView {
    fn new(
        theme: ui::Theme,
        paths: storage::DatabasePaths,
        root: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "norma.search_names_or_content",
                window,
                cx,
            ))
        });
        let id_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "norma.topic_id_e_g_preferences",
                window,
                cx,
            ))
        });
        let title_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "norma.e_g_communication_preferences",
                window,
                cx,
            ))
        });
        let description_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "norma.help_norma_decide_when_to_use_this_memory",
                window,
                cx,
            ))
        });
        let body = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .placeholder(ui::i18n::input_placeholder(
                    "norma.write_what_you_want_norma_to_remember_markdown_supported",
                    window,
                    cx,
                ))
        });
        let subscriptions = [&search, &id_input, &title_input, &description_input, &body]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        let mut view = Self {
            theme,
            service: Arc::new(Mutex::new(storage::MemoryAccess::new(
                paths.clone(),
                root.clone(),
            ))),
            paths,
            root,
            scope: MemoryScopeAlias::Personal,
            entries: vec![],
            search_texts: Default::default(),
            latest: None,
            version: MemoryRevision::INITIAL,
            selected: None,
            sources: vec![],
            preferences: Default::default(),
            search,
            id_input,
            title_input,
            description_input,
            original_title: String::new(),
            original_description: String::new(),
            kind: None,
            modified: None,
            body,
            original: String::new(),
            editing: false,
            busy: false,
            saving: false,
            pending_save: None,
            generation: 0,
            drafts: Default::default(),
            status: ui::i18n::LocalizedText::default(),
            error: false,
            imports: vec![],
            close_prompt: false,
            _subscriptions: subscriptions,
        };
        view.reload(window, cx);
        view
    }
    pub(crate) fn is_dirty(&self, cx: &App) -> bool {
        !self.drafts.is_empty() || self.current_is_dirty(cx)
    }
    fn current_is_dirty(&self, cx: &App) -> bool {
        self.saving
            || self.pending_save.is_some()
            || !self.imports.is_empty()
            || (self.editing
                && (self.body.read(cx).value().as_str() != self.original
                    || self.title_input.read(cx).value().as_str() != self.original_title
                    || self.description_input.read(cx).value().as_str()
                        != self.original_description
                    || (self.selected.is_none() && !self.id_input.read(cx).value().is_empty())))
    }
}

impl crate::app::MultisoulApp {
    pub(crate) fn ensure_norma_memory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.workspace_state.current_root.clone();
        if self
            .norma_memory
            .as_ref()
            .is_some_and(|view| view.read(cx).root != root)
        {
            if let Some(previous) = self.norma_memory.take() {
                self.norma_memory_cache
                    .insert(previous.read(cx).root.clone(), previous);
            }
        }
        let restore_cached = self.norma_memory.is_none();
        if restore_cached {
            self.norma_memory = self.norma_memory_cache.remove(&root);
        }
        if self.norma_memory.is_none() {
            let paths = self.database_paths.clone();
            let view = cx.new(|cx| MemoryView::new(self.theme, paths, root, window, cx));
            cx.observe(&view, |_, _, cx| cx.notify()).detach();
            cx.subscribe(&view, |app, _, _: &MemoryRecallRevoked, cx| {
                if let Some(runtime) = &app.agent_runtime {
                    let _ = runtime.submit(agent::AgentCommand::CancelLatest);
                }
                app.stop_doubao_voice(cx);
            })
            .detach();
            self.norma_memory = Some(view);
        }
        if let Some(view) = &self.norma_memory {
            view.update(cx, |view, cx| {
                view.theme = self.theme;
                if restore_cached {
                    view.reload(window, cx);
                }
            });
        }
    }
}

fn scope_label(scope: MemoryScopeAlias) -> &'static str {
    ui::i18n::text(match scope {
        MemoryScopeAlias::Personal => "norma.scope.personal",
        MemoryScopeAlias::Project => "norma.scope.project",
        MemoryScopeAlias::Worktree => "norma.scope.worktree",
    })
}
