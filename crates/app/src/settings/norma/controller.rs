use super::*;
use gpui::Focusable;

impl MemoryView {
    pub(super) fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.generation += 1;
        let generation = self.generation;
        let service = self.service.clone();
        let scope = self.scope;
        let selected = self.selected.clone();
        let editing = self.editing || !self.imports.is_empty();
        let task = cx.background_executor().spawn(async move {
            let mut service = service
                .lock()
                .map_err(|_| ui::i18n::text("norma.memory_service_unavailable").to_string())?;
            let snapshot = service
                .settings_snapshot(scope)
                .map_err(|e| e.to_string())?;
            let mut corpus = std::collections::HashMap::new();
            for summary in snapshot.entries() {
                let entry = service
                    .settings_read_snapshot(scope, &snapshot, summary.id.as_str())
                    .map_err(|e| e.to_string())?;
                corpus.insert(
                    summary.id.as_str().into(),
                    format!(
                        "{} {} {}",
                        summary.name, summary.description, entry.markdown
                    )
                    .to_lowercase(),
                );
            }
            let detail = if let Some(id) = selected {
                let entry = service
                    .settings_read_snapshot(scope, &snapshot, &id)
                    .map_err(|e| e.to_string())?;
                let sources = service
                    .source_details(&entry.sources)
                    .map_err(|e| e.to_string())?;
                Some((entry, sources))
            } else {
                None
            };
            Ok::<_, String>((
                snapshot.revision(),
                snapshot.entries().cloned().collect::<Vec<_>>(),
                service.preferences().map_err(|e| e.to_string())?,
                corpus,
                detail,
            ))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                if view.generation != generation {
                    return;
                }
                view.busy = false;
                match result {
                    Ok((version, entries, preferences, corpus, detail)) => {
                        if !view.editing && view.imports.is_empty() {
                            view.version = version;
                        }
                        view.search_texts = corpus;
                        if let Some((entry, sources)) = detail {
                            if editing {
                                view.latest = (version != view.version).then_some((version, entry));
                            } else {
                                view.apply_entry(entry, sources, window, cx);
                            }
                        }
                        view.entries = entries;
                        view.preferences = preferences;
                        view.error = false;
                        view.status = ui::i18n::LocalizedText::new("norma.memories_loaded").into();
                    }
                    Err(error) => {
                        view.status = error.into();
                        view.error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn change_scope(
        &mut self,
        scope: MemoryScopeAlias,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if scope == self.scope || self.busy || self.pending_save.is_some() {
            return;
        }
        if self.current_is_dirty(cx) {
            self.drafts.insert(
                self.scope,
                MemoryDraft {
                    id: self.id_input.read(cx).value().to_string(),
                    title: self.title_input.read(cx).value().to_string(),
                    description: self.description_input.read(cx).value().to_string(),
                    original_title: self.original_title.clone(),
                    original_description: self.original_description.clone(),
                    kind: self.kind,
                    modified: self.modified.clone(),
                    body: self.body.read(cx).value().to_string(),
                    original: self.original.clone(),
                    selected: self.selected.clone(),
                    version: self.version,
                    imports: self.imports.clone(),
                },
            );
        }
        self.scope = scope;
        self.selected = None;
        self.modified = None;
        self.editing = false;
        self.original.clear();
        self.original_title.clear();
        self.original_description.clear();
        self.title_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.description_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.imports.clear();
        self.latest = None;
        self.sources.clear();
        let draft = self.drafts.remove(&scope);
        let (id, body) = if let Some(draft) = draft {
            self.original = draft.original;
            self.original_title = draft.original_title;
            self.original_description = draft.original_description;
            self.kind = draft.kind;
            self.modified = draft.modified;
            self.title_input
                .update(cx, |input, cx| input.set_value(draft.title, window, cx));
            self.description_input.update(cx, |input, cx| {
                input.set_value(draft.description, window, cx)
            });
            self.selected = draft.selected;
            self.version = draft.version;
            self.imports = draft.imports;
            self.editing = self.imports.is_empty();
            (draft.id, draft.body)
        } else {
            (String::new(), String::new())
        };
        self.id_input
            .update(cx, |input, cx| input.set_value(id, window, cx));
        self.body
            .update(cx, |input, cx| input.set_value(body, window, cx));
        self.reload(window, cx);
    }
    pub(super) fn select(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_is_dirty(cx) {
            self.status =
                ui::i18n::LocalizedText::new("norma.save_or_cancel_the_current_draft_first").into();
            self.error = true;
            cx.notify();
            return;
        }
        self.busy = true;
        self.generation += 1;
        let generation = self.generation;
        let service = self.service.clone();
        let scope = self.scope;
        let task = cx.background_executor().spawn(async move {
            let mut access = service
                .lock()
                .map_err(|_| ui::i18n::text("norma.memory_service_unavailable").to_string())?;
            let (version, entry) = access
                .settings_read(scope, &id)
                .map_err(|e| e.to_string())?;
            let sources = access
                .source_details(&entry.sources)
                .map_err(|e| e.to_string())?;
            Ok::<_, String>((version, entry, sources))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                if view.generation != generation {
                    return;
                }
                view.busy = false;
                match result {
                    Ok((version, entry, sources)) => {
                        view.version = version;
                        view.apply_entry(entry, sources, window, cx);
                        view.error = false;
                        view.status.clear();
                    }
                    Err(error) => {
                        view.status = error.into();
                        view.error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn new_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_is_dirty(cx) {
            self.status =
                ui::i18n::LocalizedText::new("norma.save_or_cancel_the_current_draft_first").into();
            cx.notify();
            return;
        }
        self.selected = None;
        self.modified = None;
        self.original.clear();
        self.original_title.clear();
        self.original_description.clear();
        self.title_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.description_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.kind = Some(if self.scope == MemoryScopeAlias::Personal {
            agent::MemoryKind::User
        } else {
            agent::MemoryKind::Project
        });
        self.editing = true;
        self.sources.clear();
        self.status.clear();
        self.error = false;
        self.id_input.update(cx, |input, cx| {
            input.set_value(
                format!("memory-{}", uuid::Uuid::new_v4().simple()),
                window,
                cx,
            )
        });
        self.body
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.title_input.focus_handle(cx).focus(window);
        cx.notify();
    }
    pub(super) fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending_save.is_some() {
            return;
        }
        self.editing = false;
        self.imports.clear();
        self.close_prompt = false;
        self.error = false;
        self.status = ui::i18n::LocalizedText::new("norma.draft_discarded").into();
        self.body.update(cx, |input, cx| {
            input.set_value(self.original.clone(), window, cx)
        });
        self.title_input.update(cx, |input, cx| {
            input.set_value(self.original_title.clone(), window, cx)
        });
        self.description_input.update(cx, |input, cx| {
            input.set_value(self.original_description.clone(), window, cx)
        });
        cx.notify();
    }
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let entries = if self.imports.is_empty() {
            vec![(
                self.id_input.read(cx).value().to_string(),
                super::document::encode(
                    self.title_input.read(cx).value().as_str(),
                    self.description_input.read(cx).value().as_str(),
                    self.kind,
                    self.modified.as_deref(),
                    self.body.read(cx).value().as_str(),
                ),
            )]
        } else {
            self.imports.clone()
        };
        let saved_id = if self.imports.is_empty() {
            Some(entries[0].0.clone())
        } else {
            None
        };
        let service = self.service.clone();
        let scope = self.scope;
        let expected = self.version;
        let edit = uuid::Uuid::new_v4().to_string();
        let body = self.body.read(cx).value().to_string();
        let title = self.title_input.read(cx).value().to_string();
        let description = self.description_input.read(cx).value().to_string();
        let event = if self.imports.is_empty() {
            format!(
                "用户在 Norma 设置页{}主题 {}",
                if self.selected.is_some() {
                    "纠正"
                } else {
                    "新增"
                },
                entries[0].0
            )
        } else {
            format!(
                "用户在 Norma 设置页导入主题：{}",
                entries
                    .iter()
                    .map(|entry| entry.0.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            )
        };
        let was_uncertain = self.pending_save.is_some();
        let submission = self.pending_save.clone().unwrap_or(PendingSave {
            expected,
            entries,
            event,
            edit,
        });
        self.pending_save = Some(submission.clone());
        self.busy = true;
        self.saving = true;
        self.status = ui::i18n::LocalizedText::new("norma.saving").into();
        self.error = false;
        let task = cx.background_executor().spawn(async move {
            service
                .lock()
                .map_err(|_| {
                    agent::AdapterError::Failed(
                        ui::i18n::text("norma.memory_service_unavailable").into(),
                    )
                })?
                .settings_save(
                    scope,
                    submission.expected,
                    submission.entries,
                    &submission.event,
                    &submission.edit,
                )
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.busy = false;
                view.saving = false;
                match result {
                    Ok(output) => {
                        view.pending_save = None;
                        view.version = output.revision;
                        view.selected = saved_id;
                        view.sources.clear();
                        view.original = body;
                        view.original_title = title;
                        view.original_description = description;
                        view.editing = false;
                        view.close_prompt = false;
                        view.imports.clear();
                        view.error = false;
                        view.status = ui::i18n::LocalizedText::new("norma.saved").into();
                        view.reload(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Norma settings memory save did not return a confirmed receipt");
                        view.error = true;
                        if was_uncertain || matches!(error, agent::AdapterError::Unknown(_)) {
                            view.status = ui::i18n::LocalizedText::new("norma.save_result_is_unconfirmed_your_draft_is_preserved_select_verify_save").into();
                        } else {
                            view.pending_save = None;
                            view.status = ui::i18n::LocalizedText::new("norma.save_failed_your_draft_is_preserved_refresh_and_compare_the_latest").into();
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn toggle(&mut self, recall: bool, value: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let previous = self.preferences;
        if recall {
            self.preferences.recall = value;
        } else {
            self.preferences.save = value;
        }

        let service = self.service.clone();
        self.busy = true;
        let task = cx.background_executor().spawn(async move {
            let mut access = service
                .lock()
                .map_err(|_| ui::i18n::text("norma.memory_service_unavailable").to_string())?;
            if recall {
                access.set_recall_enabled(value)
            } else {
                access.set_save_enabled(value)
            }
            .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(preferences) => {
                        view.preferences = preferences;
                        if recall && !value {
                            cx.emit(MemoryRecallRevoked);
                        }
                        view.error = false;
                        view.status = ui::i18n::LocalizedText::new("norma.settings_saved").into();
                    }
                    Err(error) => {
                        view.preferences = previous;
                        view.status = error.into();
                        view.error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
