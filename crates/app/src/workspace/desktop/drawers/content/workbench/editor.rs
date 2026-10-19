use super::*;
use crate::app::MultisoulApp;
use gpui::{AppContext, Context, Focusable, Window};
use std::path::Path;
use ui::component::input::{InputEvent, InputState};

impl MultisoulApp {
    pub(crate) fn edit_workbench_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let s = &mut self.desktop_workspace.workbench;
        let (Some(key), Some(original)) = (s.draft_key(), s.file.clone()) else {
            return;
        };
        if !s.drafts.contains_key(&key) {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .soft_wrap(false)
                    .default_value(original.text.clone())
            });
            let draft_key = key.clone();
            cx.subscribe(&input, move |app, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(draft) = app.desktop_workspace.workbench.drafts.get_mut(&draft_key)
                    {
                        draft.dirty = input.read(cx).value().as_ref() != draft.original.text;
                    }
                    cx.notify();
                }
            })
            .detach();
            s.drafts.insert(
                key.clone(),
                Draft {
                    input,
                    original,
                    project: s.project.clone().expect("selected project"),
                    path: s.saved.selected.clone().expect("selected path"),
                    saving: false,
                    dirty: false,
                },
            );
        }
        s.show_source = true;
        s.diff = None;
        s.drafts[&key].input.focus_handle(cx).focus(window);
        self.hide_desktop_content_browsers();
        cx.notify();
    }

    pub(crate) fn save_workbench_file(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.desktop_workspace.workbench.draft_key() {
            self.save_workbench_draft(key, cx);
        }
    }

    pub(crate) fn save_all_workbench_drafts(&mut self, cx: &mut Context<Self>) {
        let keys = self
            .desktop_workspace
            .workbench
            .drafts
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.save_workbench_draft(key, cx);
        }
    }

    fn save_workbench_draft(&mut self, key: String, cx: &mut Context<Self>) {
        let s = &mut self.desktop_workspace.workbench;
        let Some(draft) = s.drafts.get_mut(&key) else {
            return;
        };
        if draft.saving || !draft.dirty {
            return;
        }
        let project = draft.project.clone();
        let path = draft.path.clone();
        let version = draft.original.version.clone();
        let text = draft.input.read(cx).value().to_string();
        let input_id = draft.input.entity_id();
        draft.saving = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let saved = project.save_text(Path::new(&path), &version, &text)?;
                    let preview = if matches!(
                        local_preview::kind(&path),
                        local_preview::Kind::Markdown | local_preview::Kind::Html
                    ) {
                        Some(local_preview::LocalPreview::new(
                            project.root(),
                            &path,
                            Some(&saved.text),
                        )?)
                    } else {
                        None
                    };
                    Ok::<_, String>((saved, preview))
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                let s = &mut app.desktop_workspace.workbench;
                let Some(draft) = s
                    .drafts
                    .get_mut(&key)
                    .filter(|d| d.input.entity_id() == input_id)
                else {
                    return;
                };
                draft.saving = false;
                match result {
                    Ok((file, preview)) => {
                        draft.dirty = draft.input.read(cx).value().as_ref() != file.text;
                        draft.original = file.clone();
                        if s.draft_key().as_deref() == Some(&key) {
                            s.file = Some(file);
                            if preview.is_some() {
                                s.preview = preview;
                            }
                        }
                        s.notice = Some("文件已保存".into());
                        s.error = None;
                        app.refresh_file_workbench(cx);
                        app.finish_workbench_leaving(cx);
                    }
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn handle_workbench_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.workbench_editor_focused(window, cx) {
            return false;
        }
        if self.shortcuts.command_for_keystroke(&event.keystroke)
            == Some(crate::shortcuts::ShortcutCommandId::SavePreviewFile)
        {
            self.save_workbench_file(cx);
            cx.stop_propagation();
        }
        true
    }

    pub(crate) fn workbench_editor_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.desktop_workspace
            .workbench
            .drafts
            .values()
            .any(|d| d.input.focus_handle(cx).is_focused(window))
    }
}
