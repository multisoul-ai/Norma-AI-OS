use super::catalog::VoiceCatalog;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, App, AppContext, Context, Entity, FocusHandle, Subscription, WeakEntity,
    Window,
};
use tts::providers::doubao::DoubaoVoice;
use ui::component::{
    input::{IndentInline, OutdentInline},
    list::{List, ListEvent, ListState},
    IndexPath,
};
use ui::Theme;

pub(super) struct VoiceMenuState {
    pub(super) list: Entity<ListState<VoiceCatalog>>,
    pub(super) body: Entity<VoiceMenuBody>,
    pub(super) open: bool,
    pub(super) theme: Theme,
    pub(super) selected: &'static DoubaoVoice,
    pub(super) trigger_focus: FocusHandle,
    app: WeakEntity<MultisoulApp>,
    subscription: Option<Subscription>,
}

impl VoiceMenuState {
    pub(super) fn new(
        theme: Theme,
        selected: &'static DoubaoVoice,
        app: WeakEntity<MultisoulApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = cx.new(|cx| {
            ListState::new(VoiceCatalog::new(theme, selected), window, cx).searchable(true)
        });
        let body = VoiceMenuBody::new_entity(list.clone(), cx);
        Self {
            list,
            body,
            open: false,
            theme,
            selected,
            trigger_focus: cx.focus_handle().tab_stop(true),
            app,
            subscription: None,
        }
    }

    pub(super) fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.open = true;
        // A fresh list on each opening clears the old search and its transient row index.
        // This also avoids carrying a query or highlight across settings visits.
        let catalog = VoiceCatalog::new(self.theme, self.selected);
        let selected_index = catalog
            .selected_row()
            .map(|row| IndexPath::default().row(row));
        self.list = cx.new(|cx| {
            let mut list = ListState::new(catalog, window, cx).searchable(true);
            list.set_selected_index(selected_index, window, cx);
            list.scroll_to_item(
                selected_index.unwrap_or_default(),
                gpui::ScrollStrategy::Top,
                window,
                cx,
            );
            list
        });
        self.body = VoiceMenuBody::new_entity(self.list.clone(), cx);
        self.subscription = Some(cx.subscribe_in(
            &self.list,
            window,
            |menu, list, event, window, cx| match event {
                ListEvent::Confirm(ix) => {
                    let Some(voice) = list.read(cx).delegate().voice_at(ix.row) else {
                        return;
                    };
                    let _ = menu.app.update(cx, |app, cx| {
                        if app.voice_settings.select_doubao_voice(&voice.id) {
                            app.persist_voice_settings_in_background(cx);
                            cx.notify();
                        }
                    });
                    menu.dismiss(window, cx);
                }
                ListEvent::Cancel => menu.dismiss(window, cx),
                ListEvent::Select(_) => {}
            },
        ));
        self.list.update(cx, |list, cx| list.focus(window, cx));
        // Mouse-down's default focus can run after the popover callback. Focus the
        // search again once the freshly created input is present in the focus tree.
        let list = self.list.clone();
        let menu = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            if menu.read_with(cx, |menu, _| menu.open).unwrap_or(false) {
                list.update(cx, |list, cx| list.focus(window, cx));
            }
        });
        window.refresh();
        cx.notify();
    }

    pub(super) fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.trigger_focus.focus(window);
        window.refresh();
        cx.notify();
    }

    pub(super) fn sync(&mut self, theme: Theme, selected: &'static DoubaoVoice, cx: &mut App) {
        self.theme = theme;
        self.selected = selected;
        self.list.update(cx, |list, _| {
            list.delegate_mut().theme = theme;
            list.delegate_mut().selected = selected;
        });
    }
}

pub(super) struct VoiceMenuBody {
    list: Entity<ListState<VoiceCatalog>>,
    menu: WeakEntity<VoiceMenuState>,
}

impl VoiceMenuBody {
    fn new_entity(
        list: Entity<ListState<VoiceCatalog>>,
        cx: &mut Context<VoiceMenuState>,
    ) -> Entity<Self> {
        let menu = cx.weak_entity();
        cx.new(|cx| {
            cx.observe(&list, |_, _, cx| cx.notify()).detach();
            Self { list, menu }
        })
    }
}

impl Render for VoiceMenuBody {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 40px search + 6 × 48px rows = 328px. Keep this stable while filtering
        // so a menu snapped above a low trigger does not move under the pointer.
        let height = 328.0;
        div()
            .w_full()
            .h(px(height).min(window.viewport_size().height - px(56.0)))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            // GPUI resolves Input's Tab actions before raw key callbacks. Capture
            // both directions here so leaving search cancels instead of indenting.
            .capture_action(cx.listener(|body, _: &IndentInline, window, cx| {
                let _ = body.menu.update(cx, |menu, cx| menu.dismiss(window, cx));
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|body, _: &OutdentInline, window, cx| {
                let _ = body.menu.update(cx, |menu, cx| menu.dismiss(window, cx));
                cx.stop_propagation();
            }))
            .child(List::new(&self.list).search_placeholder(ui::i18n::text("doubao.search_voices")))
    }
}
