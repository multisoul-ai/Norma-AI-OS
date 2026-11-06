use super::{actions::IslandTarget, controller::alert_key, view::IslandView};
use crate::workspace::desktop::QuestionEditor;
use gpui::{Context, Subscription, Window};
use runtime::island::{IslandEvent, IslandMode};
use ui::component::input::InputEvent;

#[cfg(test)]
#[path = "../../tests/unit/island_question_input_tests.rs"]
mod tests;

pub(super) struct EditingQuestion {
    pub route: IslandTarget,
    pub editor: QuestionEditor,
    window: gpui::AnyWindowHandle,
    _subscriptions: Vec<Subscription>,
}

impl EditingQuestion {
    pub fn mount(
        route: IslandTarget,
        editor: QuestionEditor,
        display: u32,
        window: &mut Window,
        cx: &mut Context<super::controller::IslandController>,
    ) -> Self {
        let id = alert_key(route.source, &route.workspace, &route.area, editor.target());
        let subscriptions = (0..editor.item_count())
            .map(|index| {
                let id = id.clone();
                cx.subscribe_in(
                    editor.input(index),
                    window,
                    move |state, input, event, _, cx| match event {
                        InputEvent::Focus => {
                            if state.displays.active_display() != Some(display) {
                                return;
                            }
                            state.focused_question = Some((id.clone(), input.entity_id()));
                            state.event(display, IslandEvent::InputFocusChanged(true), cx);
                        }
                        InputEvent::Blur => {
                            if state
                                .focused_question
                                .as_ref()
                                .is_some_and(|(_, entity)| *entity == input.entity_id())
                            {
                                state.focused_question = None;
                                state.event(display, IslandEvent::InputFocusChanged(false), cx);
                            }
                        }
                        InputEvent::Change => {
                            if let Some(editing) = state.editing.get(&id) {
                                editing.prepare(cx);
                            }
                            let has_text = !input.read(cx).value().trim().is_empty();
                            if let Some(editing) = state.editing.get_mut(&id) {
                                if editing.editor.custom_selected(index) != has_text {
                                    editing.editor.toggle_custom(index);
                                }
                            }
                            cx.notify();
                        }
                        _ => {}
                    },
                )
            })
            .collect();
        Self {
            route,
            editor,
            window: window.window_handle(),
            _subscriptions: subscriptions,
        }
    }

    pub fn prepare(&self, cx: &mut gpui::App) -> bool {
        self.route.host(cx).is_some_and(|host| {
            host.update(cx, |app, cx| {
                app.prepare_overview_question_edit_at(
                    &self.route.workspace,
                    &self.route.area,
                    self.editor.target(),
                    cx,
                )
            })
        })
    }
}

impl IslandView {
    pub fn mount_questions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.controller.update(cx, |state, cx| {
            state.mount_questions(self.display, window, cx)
        });
    }
}

impl super::controller::IslandController {
    /// Mount controls for visible questions without revoking delegation merely by displaying them.
    /// Input focus / choosing an answer and the final submit still use the original permission gate.
    pub fn mount_questions(&mut self, display: u32, window: &mut Window, cx: &mut Context<Self>) {
        let state = &*self;
        let mode = state.displays.mode_on(display);
        if !matches!(mode, IslandMode::Expanded | IslandMode::Alert) {
            return;
        }
        let selected = state.displays.state.alert_id().map(str::to_owned);
        let mut questions = Vec::new();
        for workspace in &state.visible_workspaces(cx) {
            for project in &workspace.projects {
                let route = IslandTarget::new(workspace, project);
                for row in &project.questions {
                    let id = alert_key(
                        route.source,
                        &route.workspace,
                        &route.area,
                        &row.snapshot.question.target,
                    );
                    questions.push((id, route.clone(), row.clone()));
                }
            }
        }
        let state = self;
        state
            .editing
            .retain(|key, _| questions.iter().any(|(id, _, _)| id == key));
        for (id, route, row) in questions {
            if mode == IslandMode::Alert && selected.as_deref() != Some(&id) {
                continue;
            }
            if state
                .editing
                .get(&id)
                .is_some_and(|e| e.window == window.window_handle())
            {
                continue;
            }
            state.release_question_focus(&id, cx);
            let editor = match state.editing.remove(&id) {
                Some(editing) => editing.editor.remount(window, cx),
                None => QuestionEditor::new(&row, window, cx),
            };
            state.editing.insert(
                id,
                EditingQuestion::mount(route, editor, display, window, cx),
            );
        }
    }
}

impl super::controller::IslandController {
    pub fn release_question_focus(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .focused_question
            .as_ref()
            .is_some_and(|(focused, _)| focused == id)
        {
            self.focused_question = None;
            self.event(
                self.displays.active_display().unwrap_or_default(),
                IslandEvent::InputFocusChanged(false),
                cx,
            );
        }
    }
}
