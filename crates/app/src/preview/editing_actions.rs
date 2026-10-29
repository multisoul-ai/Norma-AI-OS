use super::{
    editing::save_text_file, parse_markdown_blocks, MarkdownDocument, PreviewContent,
    PreviewEditCommand, PreviewEditCommandKind, PreviewEditor, PreviewLoadState,
};
use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::{AppContext, Context, Focusable, Window};
use ui::component::input::{InputEvent, InputState};

impl MultisoulApp {
    pub(crate) fn guard_preview_window_close(
        app: &gpui::Entity<Self>,
        window: &Window,
        cx: &gpui::App,
    ) {
        let app = app.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            app.update(cx, |app, cx| {
                let allowed = app.confirm_preview_edits_before_leaving(cx);
                if !allowed && app.desktop_workspace.workbench.has_edits() {
                    app.desktop_workspace.workbench.pending_close = Some(
                        crate::workspace::desktop::PendingWorkbenchAction::CloseWindow(
                            window.window_handle(),
                        ),
                    );
                }
                allowed
            })
            .unwrap_or(true)
        });
    }

    pub(crate) fn confirm_preview_edits_before_leaving(&mut self, cx: &mut Context<Self>) -> bool {
        if self.desktop_workspace.workbench.has_edits() {
            self.desktop_workspace.workbench.pending_close =
                Some(crate::workspace::desktop::PendingWorkbenchAction::Quit);
            self.desktop_workspace.error =
                Some(ui::i18n::LocalizedText::new("drawers.workbench_busy"));
            self.reveal_workbench_prompt(cx);
            self.hide_desktop_content_browsers();
            cx.notify();
            return false;
        }
        if self
            .previews
            .editors
            .keys()
            .any(|id| self.previews.has_pending_edits(*id))
        {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("preview.leave_unsaved"));
            cx.notify();
            return false;
        }
        true
    }

    pub(crate) fn on_preview_edit_command(
        &mut self,
        action: &PreviewEditCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.command {
            PreviewEditCommandKind::Edit => self.begin_preview_edit(action.node_id, window, cx),
            PreviewEditCommandKind::Save => self.save_preview_edit(action.node_id, cx),
            PreviewEditCommandKind::Cancel => {
                if self
                    .previews
                    .editor_for(action.node_id)
                    .is_some_and(|editor| editor.saving)
                {
                    return;
                }
                self.previews.editors.remove(&action.node_id);
                self.focus_handle.focus(window);
            }
        }
        cx.notify();
    }

    fn begin_preview_edit(&mut self, node_id: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.previews.editor_for(node_id) {
            editor.input.focus_handle(cx).focus(window);
            return;
        }
        let Some(PreviewLoadState::Ready(content)) = self.previews.state_for(node_id) else {
            return;
        };
        let Some(source) = content.text_source().map(str::to_owned) else {
            return;
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .soft_wrap(true)
                .default_value(source.clone())
        });
        let subscription = cx.subscribe(&input, move |app, input, event: &InputEvent, cx| {
            match event {
                InputEvent::Change => {
                    if let Some(editor) = app.previews.editors.get_mut(&node_id) {
                        editor.dirty = input.read(cx).value().as_ref() != editor.original;
                        editor.saved = false;
                    }
                }
                InputEvent::Focus => {
                    app.runtime.selection.select_only(node_id);
                    app.runtime.focus.exit_to_canvas();
                    app.interaction.space_held = false;
                    app.interaction.keyboard_selected_node = None;
                }
                _ => return,
            }
            cx.notify();
        });
        input.focus_handle(cx).focus(window);
        self.runtime.selection.select_only(node_id);
        self.runtime.focus.exit_to_canvas();
        self.interaction.keyboard_selected_node = None;
        self.interaction.space_held = false;
        self.previews.editors.insert(
            node_id,
            PreviewEditor {
                input,
                original: source,
                dirty: false,
                saving: false,
                saved: false,
                error: None,
                _subscription: subscription,
            },
        );
    }

    pub(crate) fn save_preview_edit(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        let Some((paths, _)) = self.preview_spec_for_node(node_id) else {
            return;
        };
        let [path] = paths.as_slice() else {
            return;
        };
        let path = path.clone();
        let markdown = matches!(
            self.previews.state_for(node_id),
            Some(PreviewLoadState::Ready(PreviewContent::Markdown(_)))
        );
        let Some(editor) = self.previews.editors.get_mut(&node_id) else {
            return;
        };
        if editor.saving || !editor.dirty {
            return;
        }
        let original = editor.original.clone();
        let edited = editor.input.read(cx).value().to_string();
        let input_id = editor.input.entity_id();
        editor.saving = true;
        editor.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    save_text_file(&path, &original, &edited)?;
                    Ok::<_, String>(if markdown {
                        PreviewContent::Markdown(MarkdownDocument {
                            blocks: parse_markdown_blocks(&edited),
                            source: edited,
                        })
                    } else {
                        PreviewContent::Text(edited)
                    })
                })
                .await;
            this.update(cx, |app, cx| {
                let Some(editor) = app.previews.editors.get_mut(&node_id) else {
                    return;
                };
                // A completed save may belong to a card from a previous workspace.
                if editor.input.entity_id() != input_id {
                    return;
                }
                editor.saving = false;
                match result {
                    Ok(content) => {
                        editor.original = content.text_source().unwrap_or_default().to_string();
                        editor.dirty = editor.input.read(cx).value().as_ref() != editor.original;
                        editor.saved = !editor.dirty;
                        app.previews
                            .states
                            .insert(node_id, PreviewLoadState::Ready(content));
                    }
                    Err(error) => {
                        editor.error = Some(
                            ui::i18n::LocalizedText::new("preview.save_failed")
                                .with_arg("reason", error),
                        )
                    }
                }
                cx.notify();
            })?;
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}
