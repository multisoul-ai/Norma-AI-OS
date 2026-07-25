//! Terminal prepared-frame cache.

use crate::canvas::text_selection::TextRange;
use crate::cards::term::paint::TerminalRenderMetrics;
use canvas::NodeId;
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use ui::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalRenderKey {
    node_id: NodeId,
    revision: u64,
    metrics: TerminalRenderMetrics,
    theme: TerminalThemeKey,
    selection: Option<TextRange>,
}

impl TerminalRenderKey {
    pub(super) fn new(
        node_id: NodeId,
        revision: u64,
        metrics: TerminalRenderMetrics,
        theme: Theme,
        selection: Option<TextRange>,
    ) -> Self {
        Self {
            node_id,
            revision,
            metrics,
            theme: TerminalThemeKey::from_theme(theme),
            selection,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalThemeKey {
    term_fg: u32,
    term_bg: u32,
    focused_stroke: u32,
    edge_highlight: u32,
}

impl TerminalThemeKey {
    fn from_theme(theme: Theme) -> Self {
        Self {
            term_fg: theme.term_fg,
            term_bg: theme.term_bg,
            focused_stroke: theme.focused_stroke,
            edge_highlight: theme.edge_highlight,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalCacheOutcome {
    Hit,
    Miss,
}

struct CacheEntry<T> {
    key: TerminalRenderKey,
    prepared: Arc<T>,
}

pub(crate) struct TerminalRenderCache<T> {
    entries: HashMap<NodeId, CacheEntry<T>>,
    prepare_count: usize,
    prepared_item_count: usize,
    enabled: bool,
    item_counter: fn(&T) -> usize,
}

impl<T> TerminalRenderCache<T> {
    #[cfg(test)]
    pub(super) fn new_enabled() -> Self {
        Self::new(true, |_| 0)
    }

    fn new(enabled: bool, item_counter: fn(&T) -> usize) -> Self {
        Self {
            entries: HashMap::new(),
            prepare_count: 0,
            prepared_item_count: 0,
            enabled,
            item_counter,
        }
    }

    pub(super) fn resolve(
        &mut self,
        key: TerminalRenderKey,
        prepare: impl FnOnce() -> T,
    ) -> (Arc<T>, TerminalCacheOutcome) {
        if self.enabled {
            if let Some(entry) = self.entries.get(&key.node_id) {
                if entry.key == key {
                    return (Arc::clone(&entry.prepared), TerminalCacheOutcome::Hit);
                }
            }
        }

        let prepared = Arc::new(prepare());
        self.prepare_count = self.prepare_count.saturating_add(1);
        self.prepared_item_count = self
            .prepared_item_count
            .saturating_add((self.item_counter)(&prepared));
        if self.enabled {
            self.entries.insert(
                key.node_id,
                CacheEntry {
                    key,
                    prepared: Arc::clone(&prepared),
                },
            );
        }
        (prepared, TerminalCacheOutcome::Miss)
    }

    pub(crate) fn remove_node(&mut self, node_id: NodeId) {
        self.entries.remove(&node_id);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    pub(super) fn retain_nodes(&mut self, live_nodes: impl IntoIterator<Item = NodeId>) {
        let live_nodes: HashSet<NodeId> = live_nodes.into_iter().collect();
        self.entries
            .retain(|node_id, _entry| live_nodes.contains(node_id));
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(super) fn prepare_count(&self) -> usize {
        self.prepare_count
    }

    #[cfg(test)]
    pub(super) fn prepared_item_count(&self) -> usize {
        self.prepared_item_count
    }

    #[cfg(test)]
    pub(super) fn entry_for_test(&self, node_id: NodeId) -> Option<Arc<T>> {
        self.entries
            .get(&node_id)
            .map(|entry| Arc::clone(&entry.prepared))
    }
}

pub(crate) type TerminalRenderCacheHandle =
    Rc<RefCell<TerminalRenderCache<super::paint::PreparedTerminalFrame>>>;

pub(crate) fn terminal_render_cache_handle_from_env() -> TerminalRenderCacheHandle {
    let enabled = std::env::var("NORMA_DISABLE_TERMINAL_RENDER_CACHE")
        .map(|value| value != "1" && !value.eq_ignore_ascii_case("true"))
        .unwrap_or(true);
    terminal_render_cache_handle(enabled)
}

#[cfg(test)]
pub(super) fn new_terminal_render_cache_handle_enabled() -> TerminalRenderCacheHandle {
    terminal_render_cache_handle(true)
}

fn terminal_render_cache_handle(enabled: bool) -> TerminalRenderCacheHandle {
    Rc::new(RefCell::new(TerminalRenderCache::new(
        enabled,
        |frame: &super::paint::PreparedTerminalFrame| frame.shaped_text_runs.len(),
    )))
}

#[cfg(test)]
#[path = "../../../tests/unit/cards/term/render_cache_tests.rs"]
mod tests;
