use super::item::VoiceMenuItem;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, App, Context, Task, Window};
use tts::providers::doubao::{voices, DoubaoVoice};
use ui::component::{
    list::{ListDelegate, ListState},
    IndexPath,
};
use ui::Theme;

pub(super) struct VoiceCatalog {
    pub(super) theme: Theme,
    pub(super) selected: &'static DoubaoVoice,
    matching: Vec<&'static DoubaoVoice>,
}

impl VoiceCatalog {
    pub(super) fn new(theme: Theme, selected: &'static DoubaoVoice) -> Self {
        Self {
            theme,
            selected,
            matching: voices().iter().collect(),
        }
    }

    pub(super) fn search(&mut self, query: &str) {
        self.matching = VoiceSettingsState::doubao_voices_matching(query);
    }

    pub(super) fn selected_row(&self) -> Option<usize> {
        self.matching
            .iter()
            .position(|voice| voice.id == self.selected.id)
    }

    pub(super) fn voice_at(&self, row: usize) -> Option<&'static DoubaoVoice> {
        self.matching.get(row).copied()
    }
}

impl ListDelegate for VoiceCatalog {
    type Item = VoiceMenuItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.matching.len()
    }

    fn perform_search(
        &mut self,
        query: &str,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        self.search(query);
        cx.notify();
        Task::ready(())
    }

    fn set_selected_index(
        &mut self,
        _: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let voice = self.voice_at(ix.row)?;
        Some(VoiceMenuItem::new(
            self.theme,
            voice,
            voice.id == self.selected.id,
        ))
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .py(px(32.0))
            .w_full()
            .text_center()
            .text_size(px(13.0))
            .text_color(rgba(self.theme.text_secondary))
            .child(ui::i18n::text("doubao.no_matching_voices"))
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/doubao_voice_menu_tests.rs"]
mod tests;
